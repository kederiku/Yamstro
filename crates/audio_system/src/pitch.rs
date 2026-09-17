//! Le demi-ton tempéré, seul modèle de hauteur du projet : TASK-104 et TASK-105.
//!
//! # Une seule formule dans tout le dépôt
//!
//! `base_pitch * 2^(n / 12)` : douze demi-tons doublent la fréquence, vingt-quatre la quadruplent,
//! et la montée s'entend comme une gamme, pas comme un glissando. Le modèle linéaire de la v1
//! n'existe nulle part, et la mise en scène ne parle pas de hauteur du tout : cette étape est la
//! seule du projet à le faire. La formule n'est écrite qu'ici ; aucun appelant ne la recalcule,
//! sans quoi la copie ne suivrait pas le jour où la hauteur de base cesserait de valoir un.
//!
//! # Deux octaves, pas davantage
//!
//! Le plafond est **dans `advance()`**, pas chez l'appelant : aucun site ne peut le contourner.
//! Sans lui, une main de trente paliers finirait en sifflet, au-delà de deux octaves et demie.
//!
//! # Remise à zéro à l'entrée du décompte
//!
//! À l'**entrée**, jamais à la fin : une séquence s'achève de trois façons au moins (dépilement
//! complet, accélération jusqu'à la file vide, transition anticipée), et un reset de fin en
//! manquerait une. Le décompte suivant partirait au demi-ton où le précédent s'est arrêté.
//!
//! L'entrée dans un état se rejoue sur une transition d'un état vers lui-même. C'est sans
//! conséquence : le reset est une affectation, il est idempotent. **Aucune garde de réentrance**
//! n'est donc écrite : elle serait un second témoin d'un fait que le compteur porte déjà.

use bevy::prelude::*;
use game_state::states::RunPhase;

/// Le compteur de demi-tons du décompte en cours. `Resource` seule : en 0.19 elle est un
/// sous-trait de `Component`, et un type ne dérive pas les deux. Trois champs, rien de plus : ni
/// compteur de paliers, ni mémoire de la dernière source, ni volume.
///
/// `Default` s'écrit à la main : dérivé, il rendrait une hauteur de base nulle, donc une hauteur
/// nulle à tous les paliers, et un plafond à zéro, atteint dès le premier palier, sans une erreur.
#[derive(Resource)]
pub struct PitchScaleTracker {
    pub base_pitch: f32, // 1.0
    pub current_semitone: i32,
    pub max_semitone: i32, // 24
}

impl Default for PitchScaleTracker {
    fn default() -> Self {
        Self {
            base_pitch: 1.0,
            current_semitone: 0,
            max_semitone: 24,
        }
    }
}

impl PitchScaleTracker {
    pub fn pitch(&self) -> f32 {
        self.base_pitch * 2.0_f32.powf(self.current_semitone as f32 / 12.0)
    }

    pub fn advance(&mut self) {
        self.current_semitone = (self.current_semitone + 1).min(self.max_semitone); // 24
    }

    pub fn reset(&mut self) {
        self.current_semitone = 0;
    }
}

fn reset_pitch_on_scoring_entry(mut pitch: ResMut<PitchScaleTracker>) {
    pitch.reset();
}

/// Insère la ressource et enregistre le reset. Appelée par `GameAudioPlugin::build`.
pub fn pitch_plugin(app: &mut App) {
    app.init_resource::<PitchScaleTracker>()
        .add_systems(OnEnter(RunPhase::Scoring), reset_pitch_on_scoring_entry);
}
