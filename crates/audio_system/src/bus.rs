//! Les trois volumes utilisateur, source unique de vérité.
//!
//! **Deux notions homonymes, à ne pas fondre.** `Bus`, dans la façade, nomme un *routage* : la
//! voie sur laquelle un son part. [`AudioBusVolumes`] porte des *valeurs* : trois curseurs que
//! l'interface de l'Étape 11 fera bouger, en y écrivant directement, sans en tenir aucune copie.
//!
//! # Où s'appliquent les volumes : sur les bus, et nulle part ailleurs
//!
//! Un changement de volume s'applique sur le gain du bus, jamais en relançant un son : le bus
//! multiplie tout ce qui le traverse, y compris un son commencé avant le mouvement du curseur.
//! **Ce que l'on remet à la façade ne porte donc aucun volume utilisateur** : [`local_gain`]
//! pour un son, [`layer_gain`] pour une couche. Y remettre [`sfx_volume`] ou [`music_gain`]
//! appliquerait les curseurs deux fois, une fois dans la valeur et une fois sur le bus, et le
//! son sortirait au carré des réglages sans qu'aucun test sur le backend nul ne le voie.
//!
//! [`sfx_volume`] et [`music_gain`] sont les deux formules de l'étape : **ce que l'auditeur
//! entend**, vérifié sur le son rendu par `tests/real_backend.rs`.
//!
//! # L'unité
//!
//! Amplitude linéaire, comme toute la façade : `0.5` vaut −6 dB. La courbe perceptive d'un
//! curseur relève de l'interface, de la position affichée, jamais de la valeur tenue ici.

use bevy::prelude::*;
use log::warn;

use crate::backend::{AudioBackendHandle, Bus};

/// Les trois volumes du mixage utilisateur, dans `0.0..=1.0`.
///
/// `Resource` seule. `Default` s'écrit à la main : dérivé, il rendrait trois zéros, et le jeu
/// démarrerait muet sans une seule erreur.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct AudioBusVolumes {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
}

impl Default for AudioBusVolumes {
    fn default() -> Self {
        Self {
            master: 1.0,
            music: 1.0,
            sfx: 1.0,
        }
    }
}

/// Borne à `0.0..=1.0`. **Une valeur non finie se lit comme le défaut audible** : `clamp` laisse
/// passer un `NaN`, et un `NaN` poussé au backend donne des échantillons `NaN`.
fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Ce que l'auditeur entend d'un son du bus SFX : `sfx * master * local_volume`, borné.
/// **Jamais remis à la façade**, voir la doc de tête.
pub fn sfx_volume(v: &AudioBusVolumes, local_volume: f32) -> f32 {
    unit(v.sfx) * unit(v.master) * local_gain(local_volume)
}

/// Ce que l'auditeur entend d'une couche : `music * master * stem_weight * duck`, borné.
/// **Jamais remis à la façade**, voir la doc de tête.
pub fn music_gain(v: &AudioBusVolumes, stem_weight: f32, duck: f32) -> f32 {
    unit(v.music) * unit(v.master) * layer_gain(stem_weight, duck)
}

/// Ce que l'on remet à `play` : la part locale du volume, et elle seule.
pub fn local_gain(local_volume: f32) -> f32 {
    unit(local_volume)
}

/// Ce que l'on remet à `set_layer_gain` : le poids de la couche et l'enveloppe de ducking, et
/// eux seuls. Un poids sorti d'une interpolation peut dépasser sa cible d'un epsilon : borné.
pub fn layer_gain(stem_weight: f32, duck: f32) -> f32 {
    unit(unit(stem_weight) * unit(duck))
}

/// Pousse les trois gains, et seulement quand la ressource a changé : la valeur ne bouge que
/// sur action de l'utilisateur. La ressource est « changée » à l'image de son insertion, donc
/// les valeurs d'ouverture partent une fois au démarrage. Le routage réverbéré rejoint le bus
/// SFX : il n'a pas de curseur et n'est jamais poussé.
fn push_bus_volumes(volumes: Res<AudioBusVolumes>, mut backend: ResMut<AudioBackendHandle>) {
    if !volumes.is_changed() {
        return;
    }
    for (bus, value, name) in [
        (Bus::Master, volumes.master, "master"),
        (Bus::Music, volumes.music, "music"),
        (Bus::Sfx, volumes.sfx, "sfx"),
    ] {
        if !value.is_finite() {
            warn!("volume `{name}` non fini ({value}) : lu comme le défaut, 1.0");
        }
        backend.0.set_bus_gain(bus, unit(value));
    }
}

/// Insère la ressource et enregistre la poussée. Appelée par `GameAudioPlugin::build`.
pub fn bus_plugin(app: &mut App) {
    app.init_resource::<AudioBusVolumes>()
        .add_systems(Update, push_bus_volumes);
}
