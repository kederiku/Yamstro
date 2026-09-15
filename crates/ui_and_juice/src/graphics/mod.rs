//! Effets visuels de l'Étape 7 : shaders WGSL, matériaux, palettes.
//!
//! **Un module de `ui_and_juice`, pas une crate de plus** (raccord B du
//! backlog). Le sens des dépendances reste
//! `core_engine ← game_state ← ui_and_juice ← shop_system` ; une crate de plus
//! obligerait la boutique et les états à arbitrer un nouveau sens, et il y
//! aurait deux ressources de réglages visuels à sérialiser à l'Étape 10.
//!
//! À la fin de TASK-82, ce dossier ne contient que `mod.rs` et `plugin.rs`.
//! `background.rs` (TASK-84), `theme.rs` (TASK-86), `crt.rs` (TASK-88) et
//! `holo.rs` (TASK-90) arrivent chacun avec leur ticket : un `pub mod` sans
//! fichier ne compile pas, et bloquerait les quatre.

pub mod plugin;

pub use plugin::VisualEffectsPlugin;
