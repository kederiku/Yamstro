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

use crate::settings::JuiceSettings;

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

/// Compteur interpolé : Chips, Mult et Total défilent au lieu de sauter.
///
/// **Seule dérogation `f64` du projet, et elle est bornée à l'affichage.**
/// Aucune de ces valeurs ne remonte vers le moteur, le contexte de blind ou la
/// session (ADR-003) : le score commis reste le `u64` du pipeline, transporté
/// tel quel. Un `displayed` arrondi et réinjecté casserait le déterminisme
/// multi-plateforme, qui est un pilier du projet. Le sens est unique —
/// centièmes `i64` vers `f64` d'affichage — et jamais l'inverse.
///
/// **`Component` uniquement.** Les quatre dérivés au-delà suivent la même
/// raison qu'à TASK-43 : c'est un type public que le dépileur construira.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AnimatedNumber {
    /// `f64` toléré : affichage uniquement.
    pub displayed: f64,
    pub target: f64,
    /// Vitesse de rattrapage, défaut 12.0.
    pub rate: f32,
    /// 0 pour Chips et Total, 1 pour le Mult.
    pub decimals: u8,
}

/// Rend la valeur avec le nombre de décimales demandé.
///
/// Le séparateur de milliers n'est pas spécifié par le corpus et n'est **pas**
/// introduit ici : la localisation appartient à l'Étape 11. Ce qui est
/// normatif, c'est que le nombre de décimales vienne de `decimals`.
fn format_counter(value: f64, decimals: u8) -> String {
    format!("{:.*}", decimals as usize, value)
}

/// `Update`, dans `JuiceSet::Animation`, sans garde d'état — comme le ressort,
/// et pour la même raison : un compteur figé en pleine interpolation par la
/// transition vers la fin de manche afficherait une valeur fausse à l'écran.
///
/// # Le lerp est exponentiel, et ce n'est pas un détail
///
/// `t = 1 - exp(-rate·dt)`. Un lerp linéaire dépendrait du framerate : le même
/// compteur défilerait à des vitesses différentes à 60 et à 144 FPS, et le
/// joueur le plus rapide verrait la séquence de score se dérouler plus vite.
///
/// La forme exponentielle rend l'erreur résiduelle **multiplicative** : chaque
/// frame la multiplie par `exp(-rate·dt)`, et le produit sur une durée totale
/// vaut `exp(-rate·T)` **quel que soit le découpage**. Vérifié : 6 frames de
/// 16,667 ms et 15 frames de 6,667 ms donnent la même valeur jusqu'au dernier
/// chiffre.
pub fn animate_numbers(time: Res<Time>, mut query: Query<(&mut AnimatedNumber, &mut Text)>) {
    let dt = time.delta_secs();
    for (mut number, mut text) in &mut query {
        let t = f64::from(1.0 - (-number.rate * dt).exp());
        number.displayed += (number.target - number.displayed) * t;

        // **Accrochage obligatoire.** Sans lui la convergence est asymptotique :
        // le compteur affiche indéfiniment une valeur qui ne se pose jamais sur
        // sa cible, et le total final reste visuellement à un poil du score
        // commis. Le seuil est calibré sur la résolution d'affichage — un demi
        // dernier chiffre visible, donc 0,5 sans décimale et 0,05 avec une.
        let epsilon = 0.5 / 10f64.powi(i32::from(number.decimals));
        if (number.target - number.displayed).abs() < epsilon {
            number.displayed = number.target;
        }

        text.0 = format_counter(number.displayed, number.decimals);
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

/// Secousse de caméra : un trauma borné qui décroît, et une amplitude
/// quadratique.
///
/// **`Resource` uniquement**, pour la même raison que `JuiceSettings`.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ScreenShake {
    /// Borné à `1.0`.
    pub trauma: f32,
    /// Unités de trauma par seconde.
    pub decay: f32,
    /// Amplitude maximale, en pixels.
    pub max_offset: f32,
}

impl Default for ScreenShake {
    /// **Un `derive(Default)` serait faux** : il donnerait `decay = 0.0` et
    /// `max_offset = 0.0`, c'est-à-dire une secousse qui ne retombe jamais et
    /// ne bouge jamais.
    ///
    /// Ces deux valeurs sont des **réglages**, à confirmer par la direction
    /// artistique à l'Étape 7 — contrairement aux constantes du ressort, qui
    /// sont figées. `decay = 1.5` fait retomber un trauma plein en deux tiers
    /// de seconde.
    fn default() -> Self {
        Self {
            trauma: 0.0,
            decay: 1.5,
            max_offset: 12.0,
        }
    }
}

impl ScreenShake {
    /// Ajoute du trauma, **borné à `1.0`** : plusieurs ajouts dans la même
    /// frame n'empilent pas au-delà.
    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }

    /// Amplitude appliquée, extraite pour être testable **sans dépendre de la
    /// phase du déplacement**.
    ///
    /// Le carré n'est pas cosmétique : à trauma 0,5 l'amplitude vaut le
    /// **quart** de celle à trauma 1,0, pas la moitié. Une relation linéaire
    /// donnerait un tremblement de fond permanent pendant toute la traîne de
    /// décroissance — fatigant à l'œil, illisible sur les paliers suivants, et
    /// sans rien gagner sur les gros impacts.
    pub fn amplitude(&self, shake_intensity: f32) -> f32 {
        self.max_offset * self.trauma * self.trauma * shake_intensity
    }
}

/// Fréquences du déplacement, en hertz. Premières entre elles, pour que le
/// motif ne se referme jamais.
///
/// **Ce ne sont pas des valeurs anodines.** Elles décident de ce qu'on voit :
/// à 2 Hz on obtient un balancement de bateau, à 31 et 37 un tremblement. Le
/// corpus ne les chiffrait pas.
///
/// **Et ce n'est pas du bruit.** Deux sinusoïdes tracent une figure de
/// Lissajous, c'est-à-dire une courbe lisse. À ces fréquences elle se lit comme
/// du tremblement, mais si la direction artistique veut du vrai bruit il faudra
/// une fonction de hachage, pas un ajustement de fréquence. Réglages d'Étape 7.
const SHAKE_FREQ_X: f32 = 31.0;
const SHAKE_FREQ_Y: f32 = 37.0;

/// `Update`, dans `JuiceSet::Animation`, sans garde d'état : si le système
/// s'arrêtait à la transition vers la fin de manche, la caméra resterait
/// décalée de son dernier offset.
///
/// # L'exception caméra
///
/// `animate_punch_scale` est le seul système qui écrit dans le `Transform`
/// d'une entité **de jeu**. **Celui-ci écrit dans celui de la caméra, et c'est
/// la seule exception admise** — l'audit de fin d'étape compte deux écrivains
/// et doit savoir pourquoi.
///
/// Il n'y a pas de conflit d'accès : les deux requêtes empruntent
/// `&mut Transform`, donc l'ordonnanceur les sérialise au lieu de les
/// paralléliser. Ce qui panique, c'est deux requêtes sur `&mut Transform` dans
/// **un même** système.
///
/// # L'écriture est absolue, jamais cumulative
///
/// `translation.x = offset` et non `+=`. C'est le défaut classique de la
/// secousse : un cumul par frame et la caméra dérive lentement hors cadre, sans
/// que rien ne le signale. La base est l'origine en `x` et `y` ; `z` n'est
/// jamais touché, c'est l'ordre de tri 2D. Si une étape ultérieure fait bouger
/// la caméra, cette convention devra être rouverte explicitement.
///
/// # La direction ne puise dans aucun des quatre flux de la partie
///
/// Ceux-ci sont du gameplay déterministe : y puiser pour du visuel
/// désynchroniserait la partie selon la cadence d'affichage. Le déplacement est
/// dérivé du temps écoulé, donc reproductible et sans état supplémentaire.
pub fn apply_screen_shake(
    time: Res<Time>,
    settings: Res<JuiceSettings>,
    mut shake: ResMut<ScreenShake>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    // Jamais sous zéro : un trauma négatif redonnerait une amplitude positive
    // par le carré, et la secousse repartirait toute seule.
    shake.trauma = (shake.trauma - shake.decay * time.delta_secs()).max(0.0);

    let amplitude = shake.amplitude(settings.shake_intensity);
    let t = time.elapsed_secs() * std::f32::consts::TAU;

    for mut transform in &mut camera {
        transform.translation.x = amplitude * (t * SHAKE_FREQ_X).sin();
        transform.translation.y = amplitude * (t * SHAKE_FREQ_Y).sin();
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

    // ---- AnimatedNumber ----

    fn compteur(app: &mut App, target: f64, decimals: u8) -> Entity {
        app.world_mut()
            .spawn((
                Text::new("0"),
                AnimatedNumber {
                    displayed: 0.0,
                    target,
                    rate: 12.0,
                    decimals,
                },
            ))
            .id()
    }

    fn affiche(app: &App, entite: Entity) -> f64 {
        app.world()
            .get::<AnimatedNumber>(entite)
            .expect("compteur")
            .displayed
    }

    fn texte(app: &App, entite: Entity) -> String {
        app.world().get::<Text>(entite).expect("texte").0.clone()
    }

    /// Avance d'un pas donné en microsecondes.
    fn avancer_de(app: &mut App, microsecondes: u64) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_micros(microsecondes));
        app.update();
    }

    #[test]
    fn test_counter_snaps_exactly_to_target() {
        let mut app = app_animation();
        let entite = compteur(&mut app, 1234.0, 0);

        for _ in 0..200 {
            avancer(&mut app);
        }

        // Égalité **exacte** : sans accrochage la convergence est asymptotique,
        // et le total resterait indéfiniment « à un poil » du score commis.
        assert_eq!(affiche(&app, entite), 1234.0);
        assert_eq!(texte(&app, entite), "1234");
    }

    #[test]
    fn test_lerp_is_framerate_independent() {
        // **Durées rigoureusement égales, et ce n'est pas une coquetterie.**
        //
        // Le corpus appariait 6 × 16,7 ms contre 14 × 6,9 ms en annonçant
        // « même durée simulée » : 100,2 ms contre 96,6 ms. Mesuré, l'écart de
        // `displayed` valait **16,37** pour une tolérance de 1e-4 — le test
        // aurait échoué alors que la propriété qu'il vise est exacte.
        //
        // Un appariement à 3 µs près ne suffit pas non plus : le résiduel se
        // déplace à `rate × résiduel` ≈ **4 460 unités par seconde**, donc une
        // tolérance de 1e-4 exige un appariement à **22 ns**. En microsecondes
        // entières, il faut des produits exacts. 5 × 20 ms contre 20 × 5 ms —
        // 50 FPS contre 200 FPS — donne alors un écart de 1e-13.
        //
        // La forme exponentielle rend l'erreur résiduelle **multiplicative** :
        // chaque frame la multiplie par `exp(-rate·dt)`, et le produit sur une
        // durée T vaut `exp(-rate·T)` quel que soit le découpage.
        let mut lent = app_animation();
        let a = compteur(&mut lent, 1234.0, 0);
        for _ in 0..5 {
            avancer_de(&mut lent, 20_000); // 50 FPS
        }

        let mut rapide = app_animation();
        let b = compteur(&mut rapide, 1234.0, 0);
        for _ in 0..20 {
            avancer_de(&mut rapide, 5_000); // 200 FPS
        }

        let ecart = (affiche(&lent, a) - affiche(&rapide, b)).abs();
        assert!(
            ecart < 1e-4,
            "écart de {ecart} : le rattrapage dépend du framerate"
        );
        // Et l'accrochage n'a pas encore mordu : ce qu'on compare est bien
        // l'interpolation, pas deux valeurs collées à la cible.
        assert!(affiche(&lent, a) < 1234.0);
    }

    #[test]
    fn test_mult_displays_one_decimal() {
        // Le Mult est un `i64` en centièmes : 430 s'affiche 4.3. La conversion
        // se fait dans le sens moteur vers affichage, et jamais dans l'autre.
        let mut app = app_animation();
        // Le Mult tel que le moteur le porte : un `i64` en centièmes.
        let mult_centiemes: i64 = 430;
        let mult = compteur(&mut app, mult_centiemes as f64 / 100.0, 1);
        let chips = compteur(&mut app, 1234.0, 0);

        for _ in 0..200 {
            avancer(&mut app);
        }

        assert_eq!(texte(&app, mult), "4.3");
        assert_eq!(texte(&app, chips), "1234", "un entier n'a pas de point");

        // **Le cas qui pince vraiment `decimals`.** Sur 4.3, l'affichage par
        // défaut de Rust rend déjà « 4.3 » : ignorer `decimals` y serait
        // indistinguable, et le banc l'a montré. Sur un Mult à exactement
        // 4,00, `{}` rend « 4 » et `{:.1}` rend « 4.0 » — et c'est la seconde
        // forme qu'il faut, sans quoi la colonne du Mult change de largeur
        // dès que la valeur tombe juste.
        let rond_centiemes: i64 = 400;
        let rond = compteur(&mut app, rond_centiemes as f64 / 100.0, 1);
        for _ in 0..200 {
            avancer(&mut app);
        }
        assert_eq!(texte(&app, rond), "4.0", "un Mult rond garde sa décimale");
    }

    #[test]
    fn test_text_shows_the_current_value_not_the_target() {
        // En pleine interpolation, le texte doit montrer où en est le compteur,
        // pas où il va. Tous les autres tests lisent le texte **après**
        // convergence, où les deux coïncident : le banc a montré qu'afficher
        // la cible y passait inaperçu.
        let mut app = app_animation();
        let entite = compteur(&mut app, 1234.0, 0);

        avancer(&mut app);

        let affiche_maintenant = affiche(&app, entite);
        assert!(
            affiche_maintenant < 1234.0,
            "déjà convergé, test sans objet"
        );
        assert_eq!(
            texte(&app, entite),
            format_counter(affiche_maintenant, 0),
            "le texte montre la cible et non la valeur courante"
        );
        assert_ne!(texte(&app, entite), "1234");
    }

    #[test]
    fn test_retarget_mid_interpolation_does_not_jump() {
        let mut app = app_animation();
        let entite = compteur(&mut app, 1234.0, 0);
        for _ in 0..3 {
            avancer(&mut app);
        }

        let a_mi_course = affiche(&app, entite);
        assert!(a_mi_course > 0.0 && a_mi_course < 1234.0);

        app.world_mut()
            .get_mut::<AnimatedNumber>(entite)
            .expect("compteur")
            .target = 5000.0;

        // Immédiatement après le changement, rien n'a bougé : la cible ne
        // déplace pas la valeur affichée, elle change seulement sa direction.
        assert_eq!(affiche(&app, entite), a_mi_course);

        avancer(&mut app);
        let apres = affiche(&app, entite);
        assert!(
            apres > a_mi_course,
            "le compteur ne repart pas vers la nouvelle cible"
        );
    }

    #[test]
    fn test_rate_controls_convergence_speed() {
        // `rate` est un **taux** : doubler le rate revient à doubler la durée.
        // L'erreur résiduelle relative vaut `exp(-rate·T)`, donc celle du rate
        // double est le **carré** de celle du rate simple. Sans ce test, un
        // `rate` ignoré ou remplacé par une constante passe inaperçu.
        let mut simple = app_animation();
        let a = compteur(&mut simple, 1234.0, 0);
        let mut double = app_animation();
        let b = compteur(&mut double, 1234.0, 0);
        double
            .world_mut()
            .get_mut::<AnimatedNumber>(b)
            .expect("compteur")
            .rate = 24.0;

        for _ in 0..6 {
            avancer_de(&mut simple, 16_667);
            avancer_de(&mut double, 16_667);
        }

        let r1 = (1234.0 - affiche(&simple, a)) / 1234.0;
        let r2 = (1234.0 - affiche(&double, b)) / 1234.0;
        assert!(
            (r2 - r1 * r1).abs() < 1e-3,
            "résiduel simple {r1}, double {r2} : le carré attendu est {}",
            r1 * r1
        );
    }

    #[test]
    fn test_snap_threshold_follows_decimals() {
        // Le seuil est **calibré sur la résolution d'affichage** : un demi
        // dernier chiffre visible, donc 0,5 sans décimale et 0,05 avec une.
        // Deux compteurs à la même distance de leur cible, une seule frame :
        // celui sans décimale s'accroche, l'autre non.
        let mut app = app_animation();
        let entier = compteur(&mut app, 100.0, 0);
        let decimal = compteur(&mut app, 100.0, 1);
        for entite in [entier, decimal] {
            app.world_mut()
                .get_mut::<AnimatedNumber>(entite)
                .expect("compteur")
                .displayed = 99.6;
        }

        avancer(&mut app);

        assert_eq!(
            affiche(&app, entier),
            100.0,
            "sans décimale, un écart de 0,33 est sous le seuil de 0,5"
        );
        assert!(
            affiche(&app, decimal) < 100.0,
            "avec une décimale, le seuil est 0,05 : il ne faut pas s'accrocher"
        );
    }

    #[test]
    fn test_number_and_punch_coexist_on_one_entity() {
        // Le § 2.6 l'affirme : les deux systèmes accèdent à des types disjoints
        // — `Text` d'un côté, `Transform` de l'autre — et cohabitent. Un
        // compteur qui pulse en changeant de valeur est le cas réel.
        let mut app = app_animation();
        let entite = app
            .world_mut()
            .spawn((
                Text::new("0"),
                AnimatedNumber {
                    displayed: 0.0,
                    target: 1234.0,
                    rate: 12.0,
                    decimals: 0,
                },
                Transform::default(),
                PunchScale::impulse(0.35),
            ))
            .id();

        avancer(&mut app);

        assert!(affiche(&app, entite) > 0.0, "le compteur n'a pas avancé");
        assert_ne!(echelle(&app, entite), Vec3::ONE, "le ressort n'a pas joué");
    }

    // ---- ScreenShake ----

    use crate::settings::JuiceSettings;

    /// Application avec une caméra posée à un endroit connu.
    ///
    /// La caméra est **déplacée** exprès : à l'origine, « l'amplitude vaut
    /// zéro » et « le système n'écrit rien » sont indistinguables.
    fn app_camera(x: f32, y: f32, z: f32) -> (App, Entity) {
        let mut app = app_animation();
        let camera = app
            .world_mut()
            .spawn((Camera2d, Transform::from_xyz(x, y, z)))
            .id();
        (app, camera)
    }

    fn position(app: &App, camera: Entity) -> Vec3 {
        app.world()
            .get::<Transform>(camera)
            .expect("caméra")
            .translation
    }

    fn trauma(app: &App) -> f32 {
        app.world().resource::<ScreenShake>().trauma
    }

    #[test]
    fn test_trauma_is_clamped_to_one() {
        let mut secousse = ScreenShake::default();
        for _ in 0..3 {
            secousse.add_trauma(0.6);
        }
        assert_eq!(secousse.trauma, 1.0, "le trauma s'est empilé au-delà de 1");
    }

    #[test]
    fn test_screen_shake_defaults_are_not_zero() {
        // **Un `derive(Default)` serait faux** : il donnerait une secousse qui
        // ne décroît jamais et ne bouge jamais. Ces deux valeurs sont des
        // **réglages d'Étape 7**, contrairement aux constantes du ressort.
        let secousse = ScreenShake::default();
        assert_eq!(secousse.trauma, 0.0);
        assert!(secousse.decay > 0.0, "une secousse qui ne retombe jamais");
        assert!(
            secousse.max_offset > 0.0,
            "une secousse qui ne bouge jamais"
        );
    }

    #[test]
    fn test_amplitude_is_quadratic() {
        // À trauma 0,5 l'amplitude vaut le **quart** de celle à trauma 1,0, et
        // non la moitié : les petites secousses restent discrètes, les grosses
        // sont franches. Une relation linéaire donnerait un tremblement de fond
        // permanent pendant toute la traîne de décroissance.
        //
        // Le test porte sur `amplitude()`, jamais sur la translation : la phase
        // du déplacement n'a pas à entrer dans l'assertion.
        let mut plein = ScreenShake::default();
        plein.add_trauma(1.0);
        let mut moitie = ScreenShake::default();
        moitie.add_trauma(0.5);

        assert_eq!(moitie.amplitude(1.0) * 4.0, plein.amplitude(1.0));
    }

    #[test]
    fn test_camera_actually_moves_with_trauma() {
        // Sans ce test, un système qui n'écrirait **rien** passerait tous les
        // autres : la caméra reposant à l'origine, « ramenée à la base » et
        // « jamais touchée » se ressemblent.
        let (mut app, camera) = app_camera(0.0, 0.0, 999.0);
        app.world_mut()
            .resource_mut::<ScreenShake>()
            .add_trauma(1.0);

        let mut ecart_max = 0.0_f32;
        for _ in 0..10 {
            avancer(&mut app);
            let p = position(&app, camera);
            ecart_max = ecart_max.max(p.x.abs()).max(p.y.abs());
        }

        assert!(
            ecart_max > 3.0,
            "la caméra n'a pas bougé : écart max {ecart_max}"
        );
    }

    #[test]
    fn test_decay_controls_the_settling_time() {
        // `decay` est un **taux**, en unités de trauma par seconde : le doubler
        // divise par deux le temps de retour au calme. Sans ce test, un `decay`
        // ignoré au profit de sa valeur par défaut passe inaperçu — le banc l'a
        // montré.
        fn frames_jusqu_au_calme(decay: f32) -> usize {
            let (mut app, _) = app_camera(0.0, 0.0, 0.0);
            {
                let mut secousse = app.world_mut().resource_mut::<ScreenShake>();
                secousse.decay = decay;
                secousse.add_trauma(1.0);
            }
            for tour in 0..300 {
                if trauma(&app) == 0.0 {
                    return tour;
                }
                avancer(&mut app);
            }
            300
        }

        let lent = frames_jusqu_au_calme(1.5);
        let rapide = frames_jusqu_au_calme(3.0);

        assert!(lent < 300 && rapide < 300, "aucune des deux n'est retombée");
        assert!(
            (lent as i32 - rapide as i32 * 2).abs() <= 2,
            "décroissance non proportionnelle : {lent} frames contre {rapide}"
        );
    }

    #[test]
    fn test_shake_moves_in_two_dimensions() {
        // Deux fréquences **distinctes**, sinon `x` et `y` sont égaux à chaque
        // instant et la caméra ne tremble que sur une diagonale. Le banc a
        // montré qu'aucun test ne le voyait.
        let (mut app, camera) = app_camera(0.0, 0.0, 0.0);
        app.world_mut()
            .resource_mut::<ScreenShake>()
            .add_trauma(1.0);

        let mut ecart_max = 0.0_f32;
        for _ in 0..10 {
            avancer(&mut app);
            let p = position(&app, camera);
            ecart_max = ecart_max.max((p.x - p.y).abs());
        }

        assert!(
            ecart_max > 1.0,
            "x et y bougent ensemble : la secousse est diagonale, écart max {ecart_max}"
        );
    }

    #[test]
    fn test_trauma_decays_to_zero_and_camera_returns() {
        let (mut app, camera) = app_camera(0.0, 0.0, 999.0);
        app.world_mut()
            .resource_mut::<ScreenShake>()
            .add_trauma(1.0);

        for _ in 0..120 {
            avancer(&mut app);
            if trauma(&app) == 0.0 {
                break;
            }
        }

        assert_eq!(trauma(&app), 0.0, "le trauma n'est pas retombé à zéro");
        // Un trauma négatif redonnerait une amplitude positive par le carré :
        // une secousse qui repart toute seule.
        assert!(trauma(&app) >= 0.0);

        let p = position(&app, camera);
        assert_eq!(p.x, 0.0, "la caméra est restée décalée en x");
        assert_eq!(p.y, 0.0, "la caméra est restée décalée en y");
        assert_eq!(p.z, 999.0, "l'ordre de tri 2D a été touché");
    }

    #[test]
    fn test_zero_shake_intensity_freezes_camera() {
        // La caméra part **décalée** : à intensité nulle elle doit être ramenée
        // à sa base, ce qui pince à la fois que l'écriture a lieu, qu'elle est
        // absolue et non cumulative, et que `z` n'est jamais touché.
        let (mut app, camera) = app_camera(5.0, 7.0, 999.0);
        app.world_mut()
            .resource_mut::<JuiceSettings>()
            .shake_intensity = 0.0;
        app.world_mut()
            .resource_mut::<ScreenShake>()
            .add_trauma(1.0);

        for _ in 0..10 {
            avancer(&mut app);
            let p = position(&app, camera);
            assert_eq!(p.x, 0.0, "la caméra bouge malgré une intensité nulle");
            assert_eq!(p.y, 0.0, "la caméra bouge malgré une intensité nulle");
            assert_eq!(p.z, 999.0);
        }
    }
}
