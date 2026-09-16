//! Effets visuels de l'Étape 7 : shaders WGSL, matériaux, palettes.
//!
//! **Un module de `ui_and_juice`, pas une crate de plus** (raccord B du
//! backlog). Le sens des dépendances reste
//! `core_engine ← game_state ← ui_and_juice ← shop_system` ; une crate de plus
//! obligerait la boutique et les états à arbitrer un nouveau sens, et il y
//! aurait deux ressources de réglages visuels à sérialiser à l'Étape 10.
//!
//! TASK-82 a posé `mod.rs` et `plugin.rs`, TASK-84 `background.rs`, TASK-86
//! `theme.rs`, TASK-88 `crt.rs`, TASK-90 `holo.rs` : les quatre fichiers que
//! le document dessine sont là.

pub mod background;
pub mod crt;
pub mod holo;
pub mod plugin;
pub mod theme;

pub use plugin::VisualEffectsPlugin;
