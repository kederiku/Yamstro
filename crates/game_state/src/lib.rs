//! Machine à états du jeu : phases, composants, ressources et systèmes Bevy.

pub mod components;
pub mod plugin;
pub mod states;

pub use components::{DieView, Hidden, Locked, PunchScale, RelicSlotUI, Scoring};
pub use plugin::{GameSet, GameStatePlugin};
pub use states::{AppState, RunPhase, SettingsOverlay};
