//! Machine à états à deux niveaux, et overlay des paramètres.
//!
//! # Transitions déclarées
//!
//! Au niveau `RunPhase`, la liste est **exhaustive** :
//!
//! ```text
//! BlindSelect → Roll
//! Roll        → Scoring                       (submit_hand)
//! Scoring     → RoundEnd                      (file vide)
//! RoundEnd    → Roll     [set_if_neq]         (main suivante, même blind)
//! RoundEnd    → Shop                          (blind battue)
//! RoundEnd    → AppState::GameOver            (hands_remaining == 0 et cible non atteinte)
//! Shop        → BlindSelect                   (bouton « Continuer »)
//! BlindSelect → AppState::Victory             (Boss Ante 8 battu)
//! ```
//!
//! Au niveau `AppState` : `MainMenu → CupSelect → InRun` ; `MainMenu ↔ Codex` ;
//! `GameOver → MainMenu` ; `Victory → MainMenu`.
//!
//! Ce fichier ne fait que **déclarer** ces transitions ; aucune n'est
//! implémentée ici.
//!
//! # Le piège de la transition vers l'état courant
//!
//! Une transition d'un état vers lui-même déclenche les schedules d'entrée et
//! de sortie, donc les despawn de `DespawnOnExit` et `DespawnOnEnter`. Le cas
//! est réel : `RoundEnd → Roll` pour la main suivante d'une même blind. Le
//! symptôme est silencieux — des entités détruites puis re-spawnées, des dés
//! qui perdent leur marqueur de verrouillage le temps d'une frame.
//!
//! Ce n'est pas inconditionnel, contrairement à ce qu'on lit parfois. Le
//! système de despawn ouvre sur :
//!
//! ```text
//! if transition.entered == transition.exited && !transition.allow_same_state_transitions {
//!     return;
//! }
//! ```
//!
//! Le drapeau vient de la variante de `NextState` employée : `set` pose un
//! `Pending` qui l'autorise, `set_if_neq` un `PendingIfNeq` qui le refuse.
//! D'où la règle : **`set_if_neq` partout où l'état cible peut être l'état
//! courant**, et les entités de run rattachées à `DespawnOnExit(AppState::InRun)`,
//! jamais à une phase.
//!
//! **La forme d'appel n'est pas celle qu'on écrirait naturellement.**
//! `next.set_if_neq(x)` sur un `ResMut<NextState<S>>` ne compile pas :
//!
//! ```text
//! error[E0277]: can't compare `NextState<RunPhase>` with `NextState<RunPhase>`
//! ```
//!
//! La méthode homonyme de `DetectChangesMut`, portée par `ResMut`, capture
//! l'appel avant celle de `NextState` et réclame un `PartialEq` que `NextState`
//! ne dérive pas. Écris donc l'appel qualifié :
//!
//! ```ignore
//! {
//!     let mut next = app.world_mut().resource_mut::<NextState<RunPhase>>();
//!     NextState::set_if_neq(&mut next, RunPhase::Roll);
//! }
//! ```
//!
//! L'emprunt se referme par un bloc et non par un `drop` : `ResMut`
//! n'implémentant pas `Drop`, clippy refuse `drop(next)` sous `-D warnings`.

use bevy::prelude::*;

/// État de l'application. Six variantes, aucune de plus.
#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    CupSelect,
    InRun,
    Codex,
    GameOver,
    Victory,
}

/// Phase d'une run. **N'existe que sous `AppState::InRun`** : hors de là, la
/// ressource `State<RunPhase>` est absente du monde, et un système qui la lit
/// sans condition d'état échoue à la construction de ses paramètres. C'est
/// voulu : être « en phase Roll » depuis le menu principal devient impossible
/// par construction.
#[derive(SubStates, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
#[source(AppState = AppState::InRun)]
pub enum RunPhase {
    #[default]
    BlindSelect,
    Roll,
    Scoring,
    RoundEnd,
    Shop,
}

/// Ouverture du menu des paramètres.
///
/// **Une ressource, jamais un état.** En faire un état obligerait à le
/// dupliquer dans chaque phase et à mémoriser un état de retour pour savoir où
/// revenir en le fermant. Le menu doit rester atteignable depuis n'importe quel
/// `AppState` sans changer d'état, ce qu'un booléen global donne gratuitement.
///
/// `Resource` étant un sous-trait de `Component` en 0.19, ajouter `Component`
/// au dérivé ferait échouer la compilation.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct SettingsOverlay {
    pub open: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    /// Application montée en headless, jamais de fenêtre.
    fn app_nue() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, crate::GameStatePlugin));
        app
    }

    /// Entre dans `InRun` puis dans la phase demandée.
    fn app_en_phase(phase: RunPhase) -> App {
        let mut app = app_nue();
        app.update();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InRun);
        app.update();
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(phase);
        app.update();
        app
    }

    #[test]
    fn test_default_app_state_is_main_menu() {
        let mut app = app_nue();
        app.update();

        assert_eq!(
            *app.world().resource::<State<AppState>>().get(),
            AppState::MainMenu
        );
    }

    #[test]
    fn test_run_phase_absent_outside_in_run() {
        // `RunPhase` n'existe que sous `AppState::InRun` : hors de là, la
        // ressource est absente du monde, et c'est ce qui rend impossible
        // d'être « en phase Roll » depuis le menu principal.
        let mut app = app_nue();
        app.update();

        assert!(app.world().get_resource::<State<RunPhase>>().is_none());
    }

    #[test]
    fn test_run_phase_defaults_to_blind_select_on_enter() {
        let mut app = app_nue();
        app.update();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InRun);
        app.update();

        let phase = app
            .world()
            .get_resource::<State<RunPhase>>()
            .expect("la sous-phase existe sous InRun");
        assert_eq!(*phase.get(), RunPhase::BlindSelect);
    }

    #[test]
    fn test_run_phase_removed_on_exit_in_run() {
        let mut app = app_en_phase(RunPhase::Roll);
        assert!(app.world().get_resource::<State<RunPhase>>().is_some());

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::MainMenu);
        app.update();

        assert!(app.world().get_resource::<State<RunPhase>>().is_none());
    }

    #[test]
    fn test_set_if_neq_on_current_state_is_inert() {
        let mut app = app_en_phase(RunPhase::Roll);
        let entite = app.world_mut().spawn(DespawnOnExit(RunPhase::Roll)).id();

        // Appel **qualifié** : `next.set_if_neq(..)` ne compile pas, la méthode
        // homonyme de `DetectChangesMut` capturant l'appel. Voir le `//!`.
        {
            let mut next = app.world_mut().resource_mut::<NextState<RunPhase>>();
            NextState::set_if_neq(&mut next, RunPhase::Roll);
        }
        app.update();

        assert!(
            app.world().get_entity(entite).is_ok(),
            "set_if_neq vers l'état courant a détruit l'entité"
        );
    }

    #[test]
    fn test_plain_set_on_current_state_despawns() {
        // Contre-épreuve du test précédent : c'est le **contraste** entre les
        // deux qui prouve le comportement, pas l'un des deux isolément.
        let mut app = app_en_phase(RunPhase::Roll);
        let entite = app.world_mut().spawn(DespawnOnExit(RunPhase::Roll)).id();

        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::Roll);
        app.update();

        assert!(
            app.world().get_entity(entite).is_err(),
            "set nu vers l'état courant n'a pas détruit l'entité"
        );
    }

    #[test]
    fn test_settings_overlay_is_a_resource() {
        // Ce test ne peut pas échouer sur la partie « lisible depuis deux
        // états » : une ressource est globale au monde et sa lecture ne dépend
        // d'aucun état. Il documente l'intention. Sa seule dent est la sonde
        // de borne : `Resource` étant un sous-trait de `Component` en 0.19,
        // ajouter `Component` au dérivé ferait échouer la compilation.
        fn exige_resource<T: Resource>() {}
        exige_resource::<SettingsOverlay>();

        let mut app = app_nue();
        app.update();
        assert!(!app.world().resource::<SettingsOverlay>().open);

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InRun);
        app.update();
        assert!(!app.world().resource::<SettingsOverlay>().open);
    }
}
