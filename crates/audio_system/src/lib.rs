//! `audio_system` — vertical layering, banque de SFX, hauteur, bus.
//!
//! Seule crate du projet qui dépende d'un backend audio. Elle lit l'état du jeu
//! et l'événement de palier que la mise en scène publie ; rien ne remonte d'ici
//! vers elle. Le backend est `bevy_seedling` sur Firewheel, branche A de
//! l'addendum à l'ADR-006 (`docs/adr/ADR-006-addendum.md`).
//!
//! Les cinq modules sont posés une fois pour toutes ; chaque ticket de l'Étape 8
//! remplit le sien.

pub mod backend;
pub mod bus;
pub mod music;
pub mod pitch;
pub mod sfx;

use bevy::prelude::*;

/// Monte l'audio du jeu.
///
/// **Vide jusqu'à TASK-97** : aucune ressource, aucun système, aucun set. Il
/// compile, il démarre sans périphérique, et il ne fait rien. Il ne monte pas
/// `JuicePlugin` et n'y ajoute rien : l'inventaire de la mise en scène reste
/// celui que TASK-46 garde.
pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, _app: &mut App) {}
}
