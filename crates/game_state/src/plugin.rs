//! Plugin racine et ordre des ensembles de systèmes.
//!
//! **Ce plugin ne pose pas de gestionnaire d'erreur.** L'API de Bevy 0.19 est
//! `App::set_error_handler`, dont le corps ouvre sur un `assert!` refusant un
//! second appel sur la même `App` : un plugin de bibliothèque qui s'en
//! emparerait ferait paniquer toute application qui le règle elle-même, ou
//! tout second plugin qui aurait la même idée. Le choix appartient au binaire
//! final, qui l'exprimera par `set_error_handler` ou en insérant la ressource
//! `bevy_ecs::error::FallbackErrorHandler`.

use bevy::prelude::*;

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
            .init_resource::<SettingsOverlay>();

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

        crate::systems::setup::register(app);
        crate::systems::input::register(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        app.add_plugins((MinimalPlugins, StatesPlugin, GameStatePlugin));

        app.update();
        app.update();
        app.update();
    }

    #[test]
    fn test_system_sets_are_ordered() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, GameStatePlugin));
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
