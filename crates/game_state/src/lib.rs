//! Machine à états du jeu : phases, composants, ressources et systèmes Bevy.

pub mod components;
pub mod plugin;
pub mod resources;
pub mod states;
pub mod systems;

pub use components::{DieView, Hidden, Locked, PunchScale, RelicSlotUI, Scoring};
pub use plugin::{GameSet, GameStatePlugin};
pub use resources::{HandContext, RunSession, ScoringStepQueue};
pub use states::{AppState, RunPhase, SettingsOverlay};
