//! Plugin racine et ordre des ensembles de systèmes.
//!
//! **Ce plugin exige `InputPlugin`.** `toggle_settings_overlay` (TASK-39) lit
//! `Res<ButtonInput<KeyCode>>` **sans aucune garde d'état** — le menu doit
//! rester atteignable partout — si bien que toute application montant ce
//! plugin doit aussi monter celui des entrées. Bevy n'offre aucun moyen
//! d'exiger un plugin ; l'absence se manifeste par une panique dont le message
//! ne nomme ni le système ni la ressource hors feature `debug`. La dépendance
//! est donc énoncée ici, faute de pouvoir l'être dans le type.
//!
//! **Ce plugin ne pose pas de gestionnaire d'erreur.** L'API de Bevy 0.19 est
//! `App::set_error_handler`, dont le corps ouvre sur un `assert!` refusant un
//! second appel sur la même `App` : un plugin de bibliothèque qui s'en
//! emparerait ferait paniquer toute application qui le règle elle-même, ou
//! tout second plugin qui aurait la même idée. Le choix appartient au binaire
//! final, qui l'exprimera par `set_error_handler` ou en insérant la ressource
//! `bevy_ecs::error::FallbackErrorHandler`.

use bevy::prelude::*;

use crate::resources::ScoringStepQueue;
use crate::states::{AppState, RunPhase, SettingsOverlay};

/// Les quatre ensembles de systèmes du tour de jeu, dans leur ordre
/// d'exécution.
///
/// Les quatre noms sont normatifs ; celui de l'énumération qui les porte est
/// introduit ici, le corpus n'en fournissant aucun.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    HandlingInput,
    UpdatingBoard,
    EvaluatingBoard,
    Resolving,
}

/// Entrées gelées par l'overlay des paramètres.
///
/// **Un ensemble à une seule raison d'être.** Il vit dans `GameSet::HandlingInput`
/// et n'y ajoute que la condition de gel ; chaque système y garde sa propre
/// condition d'état.
///
/// **Ce qui doit rester dehors.** `toggle_settings_overlay` (TASK-39) est une
/// entrée, et c'est celle qui **ferme** l'overlay : la placer ici enfermerait
/// le joueur dans un menu qu'aucune touche ne peut plus quitter. C'est
/// précisément pour éviter ce piège que le gel n'est pas posé sur
/// `HandlingInput` tout entier.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputSet {
    FrozenByOverlay,
}

/// Vrai tant que l'overlay des paramètres est fermé.
pub(crate) fn overlay_is_closed(overlay: Res<SettingsOverlay>) -> bool {
    !overlay.open
}

/// Plugin racine de la machine à états.
///
/// Il pose l'ordre des quatre ensembles, une fois pour toutes, puis délègue
/// l'enregistrement des systèmes au module qui les porte. Chaque ticket aval y
/// branche les siens de la même façon : la connaissance de l'ordre et des
/// gardes reste à côté des systèmes, jamais ici.
#[derive(Debug, Clone, Copy, Default)]
pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .add_sub_state::<RunPhase>()
            .init_resource::<SettingsOverlay>()
            // Ressource d'application, dont le défaut vide est un état
            // légitime — contrairement au contexte de main, que `setup_round`
            // doit poser parce qu'aucun `Default` n'y aurait de sens.
            .init_resource::<ScoringStepQueue>();

        app.configure_sets(
            Update,
            (
                GameSet::HandlingInput,
                GameSet::UpdatingBoard,
                GameSet::EvaluatingBoard,
                GameSet::Resolving,
            )
                .chain(),
        );

        app.configure_sets(
            Update,
            InputSet::FrozenByOverlay
                .in_set(GameSet::HandlingInput)
                .run_if(overlay_is_closed),
        );

        crate::systems::setup::register(app);
        crate::systems::input::register(app);
        crate::systems::evaluation::register(app);
        crate::systems::submission::register(app);
        crate::systems::round_end::register(app);
        crate::relics::register(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;
    use bevy::state::app::StatesPlugin;

    /// Ordre d'exécution observé, un ensemble par entrée.
    #[derive(Resource, Default)]
    struct Journal(Vec<GameSet>);

    #[test]
    fn test_plugin_boots_headless() {
        // `MinimalPlugins` n'inclut pas `StatesPlugin` : sans lui, les états
        // des tickets aval ne seraient jamais initialisés et le test passerait
        // pour de mauvaises raisons.
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin, GameStatePlugin));

        app.update();
        app.update();
        app.update();
    }

    #[test]
    fn test_system_sets_are_ordered() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin, GameStatePlugin));
        app.init_resource::<Journal>();

        // Les systèmes sont enregistrés dans l'ordre **inverse** de l'ordre
        // attendu. Sans le chaînage, rien ne contraindrait l'ordonnanceur et
        // le journal ne sortirait pas trié ; le test mord donc sur un
        // `.chain()` retiré.
        app.add_systems(
            Update,
            (
                (|mut journal: ResMut<Journal>| journal.0.push(GameSet::Resolving))
                    .in_set(GameSet::Resolving),
                (|mut journal: ResMut<Journal>| journal.0.push(GameSet::EvaluatingBoard))
                    .in_set(GameSet::EvaluatingBoard),
                (|mut journal: ResMut<Journal>| journal.0.push(GameSet::UpdatingBoard))
                    .in_set(GameSet::UpdatingBoard),
                (|mut journal: ResMut<Journal>| journal.0.push(GameSet::HandlingInput))
                    .in_set(GameSet::HandlingInput),
            ),
        );

        app.update();

        assert_eq!(
            app.world().resource::<Journal>().0,
            vec![
                GameSet::HandlingInput,
                GameSet::UpdatingBoard,
                GameSet::EvaluatingBoard,
                GameSet::Resolving,
            ]
        );
    }
}
