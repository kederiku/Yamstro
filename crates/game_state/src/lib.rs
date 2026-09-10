//! Machine à états du jeu : phases, composants, ressources et systèmes Bevy.

pub mod plugin;
pub mod states;

pub use plugin::{GameSet, GameStatePlugin};
pub use states::{AppState, RunPhase, SettingsOverlay};
