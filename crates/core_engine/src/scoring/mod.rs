//! Pipeline de calcul de score : arithmétique en point fixe et journal ordonné.

mod context;
mod effect;
pub mod levels;
mod pipeline;
mod report;

pub use context::ScoreContext;
pub use effect::{ScoreAction, ScoreEffect, StepSource};
pub use report::{ScoreStep, ScoringReport};
