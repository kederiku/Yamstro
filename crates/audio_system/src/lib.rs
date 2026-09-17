//! `audio_system` — vertical layering, banque de SFX, hauteur, bus.
//!
//! Seule crate du projet qui dépende d'un backend audio. Elle lit l'état du jeu et l'événement
//! de palier que la mise en scène publie ; rien ne remonte d'ici vers elle. Le backend est celui
//! que retient l'addendum à l'ADR-006 (`docs/adr/ADR-006-addendum.md`), et **seul `backend.rs` le
//! nomme** : ce fichier-ci, comme les quatre autres modules, ne connaît que la façade.
//!
//! Les cinq modules sont posés une fois pour toutes ; chaque ticket de l'Étape 8 remplit le sien.

pub mod backend;
pub mod bus;
pub mod music;
pub mod pitch;
pub mod sfx;

use bevy::prelude::*;
use game_state::states::{AppState, RunPhase};

pub use backend::{
    AudioBackend, AudioBackendHandle, AudioClip, BackendKind, Bus, LayerHandle, NullBackend,
    PlayedSound,
};
pub use bus::AudioBusVolumes;
pub use music::AdaptiveMusicManager;

/// Monte l'audio du jeu.
///
/// Le plugin porte un **discriminant**, jamais le backend : `build` ne reçoit qu'un `&self` et
/// ne pourrait pas déplacer une instance hors de lui. C'est `build` qui construit le backend et
/// insère [`AudioBackendHandle`], que tous les systèmes de la crate empruntent en `ResMut`.
///
/// **Les trois constructeurs exigent un `AssetPlugin` déjà monté**, le backend nul compris : la
/// façade reçoit un `AssetServer`. En 0.19 un système dont une ressource manque n'est pas écarté
/// en silence, il panique, et son message ne nomme ni le système ni le paramètre : `build` le
/// vérifie donc d'entrée, sous un message lisible.
///
/// **Ils exigent aussi la machine à états du jeu, déjà montée** : la musique lit l'état de
/// l'application à chaque image, menu compris. La machine appartient à `game_state` ; ce plugin
/// la vérifie, il ne l'initialise pas.
///
/// Il ne monte pas `JuicePlugin` et n'y ajoute rien : l'inventaire de la mise en scène reste
/// celui que TASK-46 garde.
pub struct GameAudioPlugin {
    kind: BackendKind,
}

impl GameAudioPlugin {
    /// Le backend réel, sur la sortie audio de la machine ; muet s'il n'y en a pas.
    pub fn new() -> Self {
        Self {
            kind: BackendKind::Real,
        }
    }

    /// Le backend nul journalisant : aucun périphérique, aucun son, tout est consigné.
    pub fn headless() -> Self {
        Self {
            kind: BackendKind::Null,
        }
    }

    /// Le backend réel sans plateforme de sortie : l'appelant fait avancer le contexte audio
    /// lui-même. Voie des tests sur le son réel.
    pub fn offline() -> Self {
        Self {
            kind: BackendKind::Offline,
        }
    }
}

impl Default for GameAudioPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        assert!(
            app.world().contains_resource::<AssetServer>(),
            "`GameAudioPlugin` exige un `AssetPlugin` déjà monté : la façade audio reçoit un `AssetServer`"
        );
        assert!(
            app.world().contains_resource::<State<AppState>>()
                && app
                    .world()
                    .contains_resource::<Messages<StateTransitionEvent<RunPhase>>>(),
            "`GameAudioPlugin` exige la machine à états du jeu déjà montée : `GameStatePlugin`, ou \
             `init_state::<AppState>()` puis `add_sub_state::<RunPhase>()`"
        );
        backend::install(app, self.kind);
        bus::bus_plugin(app);
        music::music_plugin(app);
    }
}
