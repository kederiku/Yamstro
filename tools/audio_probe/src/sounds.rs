//! Ce que la mesure et l'écoute partagent : les fichiers du banc et la marque d'une couche.

use bevy::prelude::*;

pub const STEMS: [&str; 4] = ["stem_0.wav", "stem_1.wav", "stem_2.wav", "stem_3.wav"];
pub const HIT: &str = "mult_hit.wav";

/// Marque la couche `n` : le fondu s'en sert pour retrouver son `VolumeNode`.
#[derive(Component, Debug, Clone, Copy)]
pub struct Layer(pub usize);
