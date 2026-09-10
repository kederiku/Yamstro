//! Systèmes de la machine à états, un module par moment du tour de jeu.

pub mod animation;
pub mod evaluation;
#[cfg(test)]
pub(crate) mod fixtures;
pub mod input;
pub mod round_end;
pub mod setup;
pub mod submission;
