//! Ressort amorti : l'unique système qui écrit dans le `Transform` d'une
//! entité de jeu.
//!
//! # Pourquoi un seul
//!
//! Deux requêtes sur `&mut Transform` dans un même système — l'une sur les dés,
//! l'autre sur les cases de relique — **paniquent au démarrage du schedule**,
//! même sur des ensembles d'entités disjoints : Bevy raisonne sur les types
//! accédés, pas sur les entités effectivement touchées. `Without<..>` et
//! `ParamSet` fonctionnent, et recréent exactement le couplage que cette
//! architecture supprime : chaque nouveau type animable obligerait à rouvrir
//! tous les autres.
//!
//! La caméra est la seule exception, traitée à part par `apply_screen_shake`.
//!
//! # Le dépileur ne touche jamais un `Transform`
//!
//! Il **pose un `PunchScale` par commande**, et rien d'autre. Il n'a même pas
//! besoin de lire un `Transform` : toute entité animable est instanciée à
//! `Transform.scale == Vec3::ONE`, sa taille visuelle venant du sprite ou du
//! nœud d'interface, et `impulse()` fixe `base_scale` en conséquence.
//!
//! # Aucune garde d'état
//!
//! Le système n'habite aucun set gardé. Au moment où le commit bascule vers la
//! fin de manche, plusieurs ressorts oscillent encore ; coupé net, le système
//! figerait `Transform.scale` à sa valeur courante — entité restée gonflée.

use bevy::prelude::*;

/// Impulsion d'échelle, posée par commande sur l'entité à secouer.
///
/// **`Component` uniquement.** En 0.19, `Resource` est un sous-trait de
/// `Component` : insérer une copie d'un type `Resource` en tant que composant
/// despawne silencieusement les autres copies.
///
/// Les quatre dérivés au-delà de `Component` sont conservés : `PunchScale` est
/// un type public que le dépileur construira, et sans `Debug` aucune assertion
/// ne peut l'afficher, sans `Copy` toute lecture le déplace.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PunchScale {
    /// Échelle de base, `Vec3::ONE` par convention.
    pub base_scale: Vec3,
    /// Écart courant à l'échelle nominale.
    pub offset: f32,
    /// Vitesse de cet écart.
    pub velocity: f32,
    /// Raideur du ressort.
    pub elasticity: f32,
    /// Amortissement.
    pub decay: f32,
}

impl PunchScale {
    /// Une frappe. **`amount` est une vitesse initiale, pas une amplitude** :
    /// avec les constantes ci-dessous, `impulse(0.35)` produit un pic d'écart
    /// d'environ 1 %, soit une échelle de 1,0106. Voir
    /// `test_impulse_amount_is_a_velocity_not_an_amplitude`, qui le mesure.
    pub fn impulse(amount: f32) -> Self {
        Self {
            base_scale: Vec3::ONE,
            offset: 0.0,
            velocity: amount,
            elasticity: 220.0,
            decay: 14.0,
        }
    }

    /// Au repos : l'écart **et** la vitesse sous leur seuil. Un ressort qui
    /// passe par sa position de repos à pleine vitesse n'est pas au repos.
    fn settled(&self) -> bool {
        self.offset.abs() < 1e-3 && self.velocity.abs() < 1e-3
    }
}

/// `Update`, dans `JuiceSet::Animation`, sans garde ni filtre.
///
/// La requête est déjà restreinte aux entités portant le composant : la
/// filtrer davantage n'accélérerait rien et rouvrirait le couplage.
pub fn animate_punch_scale(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Transform, &mut PunchScale)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut punch) in &mut query {
        // Ressort amorti : a = -k·x - c·v
        let accel = -punch.elasticity * punch.offset - punch.decay * punch.velocity;
        punch.velocity += accel * dt;

        // La vitesse est intégrée avant la position : c'est une intégration
        // **semi-implicite**, stable aux pas de temps d'une frame là où la
        // forme explicite diverge. L'extraction rend cet ordre visible.
        //
        // Le corpus justifiait cette extraction par un piège d'emprunt —
        // « un `DerefMut` pendant qu'un `Deref` est vivant ». **Mesuré : il
        // n'existe pas.** La forme fusionnée compile, l'opérande droit d'un
        // `+=` sur primitif étant évalué avant la place de gauche. Documenter
        // un danger inexistant apprend une fausse règle au lecteur suivant.
        let dv = punch.velocity * dt;
        punch.offset += dv;

        transform.scale = punch.base_scale * (1.0 + punch.offset);

        if punch.settled() {
            // Retour **exact** à la base, et retrait du composant : le laisser
            // à zéro ferait itérer le système indéfiniment sur des entités
            // inertes.
            transform.scale = punch.base_scale;
            commands.entity(entity).remove::<PunchScale>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimePlugin;
    use game_state::{AppState, DieView, RelicSlotUI};
    use std::time::Duration;

    /// Application d'animation, **sans `TimePlugin`**.
    ///
    /// Mesuré à TASK-39 : avec lui, `Time::advance_by` est réécrit à chaque
    /// frame depuis l'horloge réelle — les pas observés valaient 0,0007 s puis
    /// 0,0002 s, et le ressort ne convergeait pas. Sans lui, chaque frame vaut
    /// exactement le pas demandé, et « en un nombre borné de frames » redevient
    /// une vraie borne.
    fn app_animation() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins.build().disable::<TimePlugin>(),
            StatesPlugin,
            crate::JuicePlugin,
        ));
        app.init_resource::<Time>();
        app
    }

    /// Une frame de 16,7 ms.
    fn avancer(app: &mut App) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_micros(16_667));
        app.update();
    }

    fn echelle(app: &App, entite: Entity) -> Vec3 {
        app.world()
            .get::<Transform>(entite)
            .expect("transform")
            .scale
    }

    fn ressort(app: &App, entite: Entity) -> Option<&PunchScale> {
        app.world().get::<PunchScale>(entite)
    }

    /// Fait tourner jusqu'à extinction, ou jusqu'à la borne. Rend le nombre de
    /// frames et le pic d'écart **relatif à la base** observé.
    fn jusqu_a_extinction(app: &mut App, entite: Entity, base: Vec3, borne: usize) -> (usize, f32) {
        let mut pic = 0.0_f32;
        for tour in 0..borne {
            if ressort(app, entite).is_none() {
                return (tour, pic);
            }
            avancer(app);
            pic = pic.max((echelle(app, entite).x / base.x - 1.0).abs());
        }
        (borne, pic)
    }

    #[test]
    fn test_punch_scale_settles_to_base() {
        let mut app = app_animation();
        let entite = app
            .world_mut()
            .spawn((Transform::default(), PunchScale::impulse(0.35)))
            .id();

        avancer(&mut app);
        assert_ne!(
            echelle(&app, entite),
            Vec3::ONE,
            "le ressort n'a pas bougé l'échelle"
        );

        let (frames, _) = jusqu_a_extinction(&mut app, entite, Vec3::ONE, 200);
        assert!(frames < 200, "le ressort ne s'est pas éteint en 200 frames");
        assert!(
            ressort(&app, entite).is_none(),
            "le composant éteint est resté sur l'entité"
        );
    }

    #[test]
    fn test_punch_scale_final_scale_is_exact() {
        let mut app = app_animation();
        let entite = app
            .world_mut()
            .spawn((Transform::default(), PunchScale::impulse(0.35)))
            .id();
        jusqu_a_extinction(&mut app, entite, Vec3::ONE, 200);

        // Égalité **exacte**, jamais approchée : un résidu d'un millième reste
        // un dé qui n'est pas tout à fait revenu à sa taille.
        assert_eq!(echelle(&app, entite), Vec3::ONE);
    }

    #[test]
    fn test_punch_scale_reinsertion_restarts_spring() {
        let mut app = app_animation();
        let entite = app
            .world_mut()
            .spawn((Transform::default(), PunchScale::impulse(0.35)))
            .id();

        // Quelques frames d'oscillation, puis une seconde frappe.
        for _ in 0..5 {
            avancer(&mut app);
        }
        let avant = *ressort(&app, entite).expect("ressort en cours");
        assert_ne!(avant.offset, 0.0, "le ressort n'oscille pas encore");

        app.world_mut()
            .entity_mut(entite)
            .insert(PunchScale::impulse(0.35));
        let apres = *ressort(&app, entite).expect("ressort ré-inséré");

        // Une nouvelle frappe, jamais une somme.
        assert_eq!(apres.velocity, 0.35);
        assert_eq!(apres.offset, 0.0);
    }

    #[test]
    fn test_punch_scale_animates_heterogeneous_entities() {
        // Un dé et une case de relique, deux types sans rapport, animés par le
        // **même** système dans la même frame. C'est la solution structurelle
        // au conflit d'accès : deux requêtes sur `&mut Transform` paniqueraient
        // au démarrage du schedule, **même sur des ensembles disjoints**.
        let mut app = app_animation();
        let de = app
            .world_mut()
            .spawn((
                Transform::default(),
                DieView { order: 0 },
                PunchScale::impulse(0.35),
            ))
            .id();
        let relique = app
            .world_mut()
            .spawn((
                Transform::default(),
                RelicSlotUI(0),
                PunchScale::impulse(0.35),
            ))
            .id();

        avancer(&mut app);

        assert_ne!(echelle(&app, de), Vec3::ONE, "le dé n'a pas été animé");
        assert_ne!(
            echelle(&app, relique),
            Vec3::ONE,
            "la case de relique n'a pas été animée"
        );
        assert_eq!(echelle(&app, de), echelle(&app, relique));
    }

    #[test]
    fn test_extinction_requires_both_offset_and_velocity() {
        // **Les deux conditions, et non l'une des deux.** Migré de TASK-39, où
        // le banc avait montré qu'aucun test ne les distinguait : un ressort
        // nominal ne passe jamais assez près de zéro à pleine vitesse pour que
        // le cas se présente par hasard. Les deux montages ci-dessous sont
        // dégénérés — ni rappel, ni amortissement — et c'est voulu.
        let mut app = app_animation();

        let lance = app
            .world_mut()
            .spawn((
                Transform::default(),
                PunchScale {
                    base_scale: Vec3::ONE,
                    offset: 0.0,
                    velocity: 0.05,
                    elasticity: 0.0,
                    decay: 0.0,
                },
            ))
            .id();
        let ecarte = app
            .world_mut()
            .spawn((
                Transform::default(),
                PunchScale {
                    base_scale: Vec3::ONE,
                    offset: 0.3,
                    velocity: 0.0,
                    elasticity: 0.0,
                    decay: 0.0,
                },
            ))
            .id();

        avancer(&mut app);

        assert!(
            ressort(&app, lance).is_some(),
            "éteint alors que la vitesse est franche"
        );
        assert!(
            ressort(&app, ecarte).is_some(),
            "éteint alors que l'écart est franc"
        );
    }

    #[test]
    fn test_punch_scale_runs_across_state_changes() {
        // Un ressort en cours doit se résoudre **après** une transition : le
        // système n'est sous aucune garde d'état, et n'habite aucun set gardé.
        // Coupé net, il figerait l'entité à une échelle gonflée.
        let mut app = app_animation();
        app.init_state::<AppState>();
        let entite = app
            .world_mut()
            .spawn((Transform::default(), PunchScale::impulse(0.35)))
            .id();
        avancer(&mut app);

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InRun);
        let (frames, _) = jusqu_a_extinction(&mut app, entite, Vec3::ONE, 200);

        assert!(frames < 200, "le ressort s'est arrêté au changement d'état");
        assert_eq!(echelle(&app, entite), Vec3::ONE);
    }

    #[test]
    fn test_impulse_amount_is_a_velocity_not_an_amplitude() {
        // **Ce que `impulse(0.35)` produit vraiment.** `amount` est une
        // **vitesse**, pas une amplitude : avec les constantes normatives
        // — raideur 220, amortissement 14 — le pic d'écart vaut environ 1 %,
        // soit une échelle de 1,0106. Sur un dé de 64 pixels, c'est moins d'un
        // pixel. Le juice visé par cette étape se situe entre 10 et 30 %, ce
        // qui demande un `amount` d'un ordre de grandeur au-dessus.
        //
        // Ce test ne juge pas la valeur : il la **consigne**, pour que TASK-49
        // choisisse ses amplitudes en connaissance de cause plutôt qu'en
        // recopiant 0.35. La borne haute tombera si quelqu'un recalibre les
        // constantes sans revenir ici.
        let mut app = app_animation();
        let entite = app
            .world_mut()
            .spawn((Transform::default(), PunchScale::impulse(0.35)))
            .id();

        let (frames, pic) = jusqu_a_extinction(&mut app, entite, Vec3::ONE, 200);

        assert!(pic > 0.005, "le ressort n'a pas bougé : pic {pic}");
        assert!(
            pic < 0.05,
            "pic mesuré {pic} : l'ordre de grandeur a changé, TASK-49 doit revoir ses amplitudes"
        );

        // **La durée compte autant que l'amplitude**, et c'est elle qui pince
        // les constantes. Mesuré : 32 frames, soit un peu plus d'un demi-
        // seconde. Une intégration explicite au lieu de semi-implicite en
        // demande 43 ; une raideur de 20 au lieu de 220 en demande 139, soit
        // deux secondes et demie de dé qui tremblote. Aucune des deux ne se
        // voit sur le pic seul — le banc l'a montré.
        assert!(
            frames <= 40,
            "extinction en {frames} frames : le ressort traîne, la raideur ou l'ordre d'intégration a changé"
        );
    }

    #[test]
    fn test_base_scale_is_honoured() {
        // `base_scale` n'est pas décoratif : une entité dont l'échelle nominale
        // n'est pas l'unité doit osciller **autour d'elle** et y revenir. Tous
        // les autres tests emploient `Vec3::ONE`, où ignorer le champ est
        // rigoureusement indistinguable — le banc l'a montré.
        let mut app = app_animation();
        let base = Vec3::splat(2.0);
        let entite = app
            .world_mut()
            .spawn((
                Transform::default(),
                PunchScale {
                    base_scale: base,
                    offset: 0.0,
                    velocity: 0.35,
                    elasticity: 220.0,
                    decay: 14.0,
                },
            ))
            .id();

        avancer(&mut app);
        let en_vol = echelle(&app, entite);
        assert!(
            en_vol.x > base.x,
            "l'oscillation ne part pas de la base : {en_vol:?} contre {base:?}"
        );

        jusqu_a_extinction(&mut app, entite, base, 200);
        assert_eq!(
            echelle(&app, entite),
            base,
            "l'échelle n'est pas revenue à sa base"
        );
    }
}
