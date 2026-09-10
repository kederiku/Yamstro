//! Unique système d'animation : le seul écrivain de `Transform` du projet.
//!
//! # Pourquoi un seul, et pourquoi maintenant
//!
//! Deux `Query<&mut Transform>` dans un même système — l'une sur les dés,
//! l'autre sur les cases de relique — sont en **conflit d'accès à
//! l'exécution**, même sur des ensembles d'entités disjoints : Bevy raisonne
//! sur les types accédés, pas sur les entités effectivement touchées. Des
//! filtres `Without<..>` mutuellement exclusifs ou un `ParamSet` marchent, et
//! recréent exactement le couplage qu'on supprime : chaque nouveau type
//! animable obligerait à rouvrir tous les autres.
//!
//! La solution est structurelle, et elle se pose ici parce que c'est l'Étape 4
//! qui en dépend : le dépileur de la file de score posera des impulsions sur
//! des dés, des cartes de relique et des compteurs de texte.
//!
//! Aucun autre système de cette crate n'écrit un `Transform`, et deux gardes du
//! volet 1 le tiennent : l'écriture est interdite partout ailleurs, et exigée
//! ici.
//!
//! # Aucune garde d'état
//!
//! Un ressort en cours doit se résoudre après une transition. Le système ne
//! porte donc ni `in_state`, ni appartenance aux quatre ensembles du tour de
//! jeu : il n'a rien à ordonner avec eux.

use bevy::prelude::*;

use crate::components::PunchScale;

/// Amplitude en deçà de laquelle le ressort est considéré éteint.
///
/// Ce n'est pas une valeur de gameplay : elle borne une animation, et
/// l'interdiction des flottants porte sur l'arithmétique de score.
const REST_OFFSET: f32 = 1e-3;

/// Vitesse en deçà de laquelle le ressort est considéré éteint. Les **deux**
/// conditions sont exigées : un ressort qui passe par zéro à pleine vitesse
/// n'est pas au repos.
const REST_VELOCITY: f32 = 1e-3;

/// `Update`, sans garde ni filtre. La requête est déjà restreinte aux entités
/// portant le composant : la filtrer davantage n'accélérerait rien et rouvrirait
/// le couplage.
fn animate_punch_scale(
    mut commands: Commands,
    time: Res<Time>,
    mut animated: Query<(Entity, &mut Transform, &mut PunchScale)>,
) {
    let dt = time.delta_secs();

    for (entity, mut transform, mut punch) in &mut animated {
        // Ressort amorti, intégration semi-implicite : la vitesse d'abord,
        // puis la position, ce qui reste stable aux pas de temps d'une frame.
        let accel = -punch.elasticity * punch.offset - punch.decay * punch.velocity;

        // Les deux valeurs sont extraites avant d'être écrites : `punch.offset
        // += punch.velocity * dt` emprunterait le même `Mut` en lecture et en
        // écriture au même instant.
        let dv = accel * dt;
        punch.velocity += dv;
        let dx = punch.velocity * dt;
        punch.offset += dx;

        if punch.offset.abs() < REST_OFFSET && punch.velocity.abs() < REST_VELOCITY {
            // Retour **exact** à la base, et retrait du composant : le laisser
            // à zéro ferait itérer le système indéfiniment sur des entités
            // inertes.
            transform.scale = punch.base_scale;
            commands.entity(entity).remove::<PunchScale>();
            continue;
        }

        transform.scale = punch.base_scale * (1.0 + punch.offset);
    }
}

/// Branche l'unique système d'animation.
pub(crate) fn register(app: &mut App) {
    app.add_systems(Update, animate_punch_scale);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimePlugin;
    use std::time::Duration;

    use crate::components::{DieView, PunchScale, RelicSlotUI};
    use crate::states::AppState;

    /// Application d'animation, **sans `TimePlugin`**.
    ///
    /// Mesuré : avec lui, `Time::advance_by` est réécrit à chaque frame depuis
    /// l'horloge réelle — les pas observés valaient 0,0007 s puis 0,0002 s, et
    /// le ressort ne convergeait pas. Sans lui, chaque frame vaut exactement le
    /// pas demandé, et « après un nombre borné de frames » redevient une vraie
    /// borne.
    fn app_animation() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins.build().disable::<TimePlugin>(),
            StatesPlugin,
            InputPlugin,
            crate::GameStatePlugin,
        ));
        app.init_resource::<Time>();
        app
    }

    fn avancer(app: &mut App, millisecondes: u64) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(millisecondes));
        app.update();
    }

    /// Impulsion sous-amortie qui se résorbe en une demi-seconde environ.
    fn impulsion() -> PunchScale {
        PunchScale {
            base_scale: Vec3::ONE,
            offset: 0.3,
            velocity: 0.0,
            elasticity: 400.0,
            decay: 20.0,
        }
    }

    fn echelle(app: &App, entite: Entity) -> Vec3 {
        app.world()
            .get::<Transform>(entite)
            .expect("transform")
            .scale
    }

    /// Fait tourner jusqu'à extinction, ou jusqu'à la borne.
    fn jusqu_a_extinction(app: &mut App, entite: Entity, frames: usize) -> usize {
        for tour in 0..frames {
            if app.world().get::<PunchScale>(entite).is_none() {
                return tour;
            }
            avancer(app, 16);
        }
        frames
    }

    #[test]
    fn test_punch_scale_settles_and_is_removed() {
        let mut app = app_animation();
        let entite = app
            .world_mut()
            .spawn((Transform::default(), impulsion()))
            .id();

        avancer(&mut app, 16);
        assert_ne!(
            echelle(&app, entite),
            Vec3::ONE,
            "le ressort n'a pas bougé l'échelle"
        );

        let tours = jusqu_a_extinction(&mut app, entite, 200);
        assert!(tours < 200, "le ressort ne s'est pas éteint en 200 frames");

        assert!(
            app.world().get::<PunchScale>(entite).is_none(),
            "le composant éteint est resté sur l'entité"
        );
        assert_eq!(
            echelle(&app, entite),
            Vec3::ONE,
            "l'échelle n'est pas revenue exactement à la base"
        );
    }

    #[test]
    fn test_extinction_requires_both_offset_and_velocity() {
        // **Les deux conditions, et non l'une des deux.** Le banc a montré
        // qu'aucun test ne les distinguait : le ressort nominal ne passe jamais
        // assez près de zéro à pleine vitesse pour que le cas se présente par
        // hasard. Les deux montages ci-dessous sont dégénérés — ni rappel, ni
        // amortissement — et c'est voulu : ils figent chacun **une** des deux
        // grandeurs sous son seuil pendant que l'autre reste franchement
        // au-dessus.
        let mut app = app_animation();

        // Écart nul, vitesse non nulle : un ressort qui passe par sa position
        // de repos n'est pas au repos.
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

        // Écart non nul, vitesse nulle : un ressort à son extrême non plus.
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

        avancer(&mut app, 16);

        assert!(
            app.world().get::<PunchScale>(lance).is_some(),
            "éteint alors que la vitesse est franche"
        );
        assert!(
            app.world().get::<PunchScale>(ecarte).is_some(),
            "éteint alors que l'écart est franc"
        );
    }

    #[test]
    fn test_punch_scale_animates_heterogeneous_entities() {
        // Un dé et une case de relique, deux types sans rapport, animés par le
        // **même** système dans la même frame. C'est la solution structurelle
        // au conflit d'accès : il n'existe qu'une requête sur `Transform`.
        let mut app = app_animation();
        let de = app
            .world_mut()
            .spawn((Transform::default(), DieView { order: 0 }, impulsion()))
            .id();
        let relique = app
            .world_mut()
            .spawn((Transform::default(), RelicSlotUI(0), impulsion()))
            .id();

        avancer(&mut app, 16);

        assert_ne!(echelle(&app, de), Vec3::ONE, "le dé n'a pas été animé");
        assert_ne!(
            echelle(&app, relique),
            Vec3::ONE,
            "la case de relique n'a pas été animée"
        );
        assert_eq!(echelle(&app, de), echelle(&app, relique));
    }

    #[test]
    fn test_punch_scale_reinsertion_restarts_the_spring() {
        let mut app = app_animation();
        let entite = app
            .world_mut()
            .spawn((Transform::default(), impulsion()))
            .id();
        jusqu_a_extinction(&mut app, entite, 200);
        assert!(app.world().get::<PunchScale>(entite).is_none());

        app.world_mut().entity_mut(entite).insert(impulsion());
        avancer(&mut app, 16);

        assert_ne!(
            echelle(&app, entite),
            Vec3::ONE,
            "ré-insérer n'a pas relancé le ressort"
        );
    }

    #[test]
    fn test_punch_scale_runs_across_state_changes() {
        // Un ressort en cours doit se résoudre après une transition : le
        // système n'est sous aucune garde d'état.
        let mut app = app_animation();
        let entite = app
            .world_mut()
            .spawn((Transform::default(), impulsion()))
            .id();
        avancer(&mut app, 16);

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InRun);
        let tours = jusqu_a_extinction(&mut app, entite, 200);

        assert!(tours < 200, "le ressort s'est arrêté au changement d'état");
        assert_eq!(echelle(&app, entite), Vec3::ONE);
    }
}
