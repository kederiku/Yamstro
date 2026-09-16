//! Mise en scène du score : animation, secousse, dépilement de la file.
//!
//! # Le sens des dépendances est unique
//!
//! ```text
//! core_engine  ←  game_state  ←  ui_and_juice
//! ```
//!
//! Cette crate connaît les deux autres ; **jamais l'inverse**. Une seule ligne
//! `ui_and_juice = { path = … }` dans le manifeste de `game_state` ferme un
//! cycle et le workspace cesse de compiler. Le réflexe naturel viendra le jour
//! où `ScoringStepQueue` semblera mal placée : elle reste dans `game_state`.
//!
//! # Aucune dépendance de tweening, aucune dépendance audio
//!
//! Le ressort et le lerp sont **maison**, une dizaine de lignes chacun. La v1
//! déclarait une bibliothèque de tweening et ne l'employait jamais. Le son
//! appartient à l'Étape 8, par le backend de l'ADR-006 : cette étape émet un
//! événement et rien de plus.
//!
//! # Trois sets chaînés, un quatrième délibérément libre
//!
//! Le dépilement est une chaîne — lire l'entrée, faire avancer la file,
//! commettre quand elle est vide — et cet ordre est posé **ici**, une fois pour
//! toutes, plutôt que réparti sur les tickets qui peupleront les sets.
//!
//! **Les systèmes d'animation, eux, tournent en permanence.** Un système coupé
//! net à une transition fige `Transform.scale` à sa valeur courante : entité
//! restée gonflée, caméra décalée hors de sa position de base. Au moment où le
//! commit bascule vers la fin de manche, plusieurs ressorts oscillent encore.
//! `JuiceSet::Animation` ne porte donc ni garde d'état ni chaînage — et il ne
//! faut pas lui en ajouter « par symétrie ».
//!
//! `run_in`, `run_after` et `run_before` sont supprimés en 0.19 ; `.chain()`,
//! `.before()` et `.after()` restent les combinateurs ordinaires.

pub mod animation;
pub mod events;
pub mod graphics;
pub mod queue;
pub mod settings;

use bevy::prelude::*;
use game_state::RunPhase;

/// Les quatre ensembles de la mise en scène.
///
/// Les trois premiers portent la chaîne du dépilement ; le quatrième est libre,
/// pour la raison écrite dans le `//!`.
///
/// Le corpus nomme les trois systèmes mais aucune énumération porteuse : celle-ci
/// est introduite ici, sur le modèle de `GameSet`. L'alternative — un `.chain()`
/// écrit au moment de l'enregistrement — répartirait la chaîne sur deux tickets
/// et deux fichiers.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JuiceSet {
    ReadInput,
    TickQueue,
    Commit,
    Animation,
}

/// Plugin de la mise en scène.
///
/// Il ne fait qu'une chose : poser l'ordre et la garde des trois sets de
/// dépilement. **Il n'enregistre aucun système et n'initialise aucun état** ;
/// les états appartiennent à `GameStatePlugin`, et chaque ticket aval branche
/// ses systèmes dans l'ensemble qui lui revient.
#[derive(Debug, Clone, Copy, Default)]
pub struct JuicePlugin;

impl Plugin for JuicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::animation::ScreenShake>()
            .init_resource::<crate::settings::JuiceSettings>();

        // Enregistrement **unique** du tampon. Un second `add_message` ne
        // doublerait pas la file : il ne ferait rien (0.19 teste la présence de
        // `Messages<M>` avant d'enregistrer) et masquerait l'oubli du premier.
        app.add_message::<crate::events::ScoreStepPlayed>();

        app.configure_sets(
            Update,
            (JuiceSet::ReadInput, JuiceSet::TickQueue, JuiceSet::Commit)
                .chain()
                .run_if(in_state(RunPhase::Scoring)),
        );

        // `JuiceSet::Animation` n'est volontairement ni chaîné ni gardé : il
        // n'est pas configuré du tout.
        app.add_systems(
            Update,
            (
                crate::animation::animate_punch_scale,
                crate::animation::animate_numbers,
                crate::animation::apply_screen_shake,
            )
                .in_set(JuiceSet::Animation),
        );

        // Le flash lit les paliers joués : il doit tourner **après** le
        // dépilement, sinon il les découvre à la frame suivante. Il reste hors
        // garde d'état comme les autres animations : une opacité coupée net à
        // la transition laisserait l'écran blanc.
        app.add_systems(
            Update,
            crate::animation::animate_flash
                .in_set(JuiceSet::Animation)
                .after(JuiceSet::TickQueue),
        );

        // **Un seul support de flash**, instancié une fois et nulle part
        // ailleurs : deux nœuds, ce serait deux plafonds indépendants, donc six
        // changements de luminance par seconde.
        app.add_systems(Startup, crate::animation::spawn_flash_overlay);

        // Les deux premiers maillons du dépilement. Le chaînage et la garde
        // d'état viennent du `configure_sets` ci-dessus : les répéter ici ne
        // ferait que doubler ce qui existe déjà.
        app.add_systems(
            Update,
            (
                crate::queue::read_fast_forward_input.in_set(JuiceSet::ReadInput),
                crate::queue::tick_scoring_queue.in_set(JuiceSet::TickQueue),
                crate::queue::commit_score_when_drained.in_set(JuiceSet::Commit),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;
    use bevy::state::app::StatesPlugin;
    use game_state::{AppState, RunPhase};

    /// Ordre d'exécution observé, un ensemble par entrée.
    #[derive(Resource, Default)]
    struct Journal(Vec<JuiceSet>);

    fn app_juice() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin, JuicePlugin));
        app
    }

    /// Pose les deux états et la file à la main.
    ///
    /// `JuicePlugin` n'initialise ni l'un ni l'autre — c'est le travail de
    /// `GameStatePlugin` — et le monter ici ferait entrer toute la machine de
    /// jeu dans un test qui ne vérifie que l'ordonnancement de ce plugin-ci.
    ///
    /// **Sans états, les trois sets gardés ne tournent jamais** : la condition
    /// rend `false` sans paniquer, et un test d'ordre passerait sur une liste
    /// vide. **Avec** les états, en revanche, le dépilement de TASK-48 tourne,
    /// et il réclame la file : sans elle le système panique, sans même pouvoir
    /// nommer la ressource manquante — les noms de type valent tous
    /// « Enable the debug feature to see the name » dans notre graphe.
    /// `InputPlugin` est requis pour la même raison, depuis TASK-48.
    fn poser_les_etats(app: &mut App) {
        app.init_state::<AppState>();
        app.add_sub_state::<RunPhase>();
        app.insert_resource(game_state::ScoringStepQueue::default());
        app.insert_resource(core_engine::blinds::BlindContext {
            blind: core_engine::blinds::BlindDefinition::default(),
            target_score: 0,
            current_score: 0,
            hands_remaining: 0,
            used_hands: core_engine::hands::HandGrid::default(),
        });
    }

    fn entrer_dans_le_comptage(app: &mut App) {
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InRun);
        app.update();
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::Scoring);
        app.update();
    }

    fn journal(app: &App) -> Vec<JuiceSet> {
        app.world().resource::<Journal>().0.clone()
    }

    #[test]
    fn test_juice_plugin_boots_headless() {
        // Aucun état initialisé, et c'est le cas nominal d'un plugin monté
        // seul : la condition des trois sets ne doit pas paniquer.
        let mut app = app_juice();

        app.update();
        app.update();
        app.update();
    }

    #[test]
    fn test_juice_sets_are_ordered() {
        let mut app = app_juice();
        poser_les_etats(&mut app);
        app.init_resource::<Journal>();

        // Enregistrés dans l'ordre **inverse** de l'ordre attendu : sans le
        // chaînage, rien ne contraindrait l'ordonnanceur et le journal ne
        // sortirait pas trié.
        app.add_systems(
            Update,
            (
                (|mut j: ResMut<Journal>| j.0.push(JuiceSet::Commit)).in_set(JuiceSet::Commit),
                (|mut j: ResMut<Journal>| j.0.push(JuiceSet::TickQueue))
                    .in_set(JuiceSet::TickQueue),
                (|mut j: ResMut<Journal>| j.0.push(JuiceSet::ReadInput))
                    .in_set(JuiceSet::ReadInput),
            ),
        );

        app.update();
        assert!(
            journal(&app).is_empty(),
            "les sets ont tourné hors de la phase de comptage"
        );

        entrer_dans_le_comptage(&mut app);

        assert_eq!(
            journal(&app),
            vec![JuiceSet::ReadInput, JuiceSet::TickQueue, JuiceSet::Commit]
        );
    }

    #[test]
    fn test_animation_set_is_never_gated() {
        // **Règle normative, et elle se voit à l'écran.** Un système d'animation
        // coupé net à une transition fige l'échelle à sa valeur courante :
        // entité restée gonflée, caméra décalée. Le set d'animation ne porte
        // donc ni garde d'état ni chaînage, et tourne même hors de toute run —
        // ici, aucun état n'est initialisé du tout.
        let mut app = app_juice();
        app.init_resource::<Journal>();
        app.add_systems(
            Update,
            (|mut j: ResMut<Journal>| j.0.push(JuiceSet::Animation)).in_set(JuiceSet::Animation),
        );

        app.update();

        assert_eq!(journal(&app), vec![JuiceSet::Animation]);
    }
}
