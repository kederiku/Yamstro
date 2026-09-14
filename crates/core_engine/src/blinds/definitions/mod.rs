//! Catalogue des manches : les boss et leur traduction en contrainte.
//!
//! Symétrique de `cups/definitions` et de `relics/definitions` : les **types**
//! vivent dans `blinds/mod.rs`, ce répertoire porte le **catalogue**.

pub mod bosses;

pub use bosses::{BossDefinition, BossId, boss_definition, draw_boss};
