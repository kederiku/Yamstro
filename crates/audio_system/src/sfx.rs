//! La banque de sons unique et son générateur local : TASK-102 et TASK-103.
//!
//! # Une ressource, et une seule
//!
//! [`SoundEffectBank`] est **la** ressource de son du projet. La v1 en avait trois pour un rôle,
//! réparties sur deux documents, donc trois systèmes audio en concurrence : c'est ce défaut que
//! la banque unique corrige. Un clip de plus dans la banque n'est pas une seconde ressource ;
//! une seconde ressource, un « pool » local dans un autre fichier, si. Celui qui en ressent le
//! besoin s'arrête et le signale.
//!
//! `hand_base_chord` est de ceux-là : sans lui la figure de base emprunterait l'accord des
//! reliques, et deux sources sur un même échantillon suppriment ce que le timbre porte. C'est
//! le grain qui dit au joueur ce qui vient de se déclencher, pas la hauteur.
//!
//! # Chargée une fois, par la façade
//!
//! Dix-sept chargements au démarrage, dans l'ordre des champs, par un seul site : les index des
//! clips sont ceux du catalogue du backend, et deux sites de chargement les feraient diverger
//! d'une exécution à l'autre. **Un fichier absent n'arrête rien** : la façade rend toujours un
//! clip valide, et le backend réel le remplace par un bip, en le disant. Ce fichier n'a donc ni
//! branche d'échec, ni résultat à déballer.
//!
//! # Un générateur local, jamais celui de la run
//!
//! Le générateur de la run est sérialisé dans la sauvegarde. Un seul tirage audio pris sur l'un
//! de ses flux le décalerait d'un cran, et le nombre de sons joués dépend du volume, du nombre
//! d'images, du moment de la sauvegarde : **même graine, dés différents**, sans une ligne
//! d'erreur. Un cinquième flux réservé à l'audio serait sérialisé lui aussi, et ne réparerait
//! rien. La banque tient donc son propre générateur, privé, initialisé à l'entropie système,
//! et n'entre dans aucune sauvegarde : l'audio ne fait pas partie de l'état de jeu.

use bevy::prelude::*;
use rand::{RngExt, SeedableRng, rngs::SmallRng};

use crate::backend::{AudioBackendHandle, AudioClip};

/// La banque. `Resource` seule : en 0.19 elle est un sous-trait de `Component`, et un type ne
/// dérive pas les deux. Insérée par `load_sound_effect_bank`, et par personne d'autre.
#[derive(Resource)]
pub struct SoundEffectBank {
    pub dice_rolls: [AudioClip; 6],
    pub chip_tick: AudioClip,
    pub hand_base_chord: AudioClip,
    pub relic_chord: AudioClip,
    pub mult_hit: AudioClip,
    pub seal_tick: AudioClip,
    pub die_lock: AudioClip,
    pub hand_consumed: AudioClip,
    pub ui_hover: AudioClip,
    pub ui_click: AudioClip,
    pub coin: AudioClip,
    pub victory_fanfare: AudioClip,
    /// Générateur local, non sérialisé, initialisé à l'entropie système. Privé : un tirage fait
    /// depuis un autre fichier échapperait au test qui garde le générateur de la run.
    rng: SmallRng,
}

impl SoundEffectBank {
    /// Une des six variations du lancer, et sa hauteur : plus ou moins 5 %, bornes comprises.
    pub fn dice_roll(&mut self) -> (AudioClip, f32) {
        let idx = self.rng.random_range(0..6);
        let pitch = 1.0 + self.rng.random_range(-0.05..=0.05);
        (self.dice_rolls[idx], pitch)
    }
}

/// Précharge la banque par la façade, puis l'insère. Les chemins sont relatifs à la racine des
/// assets ; les fichiers sont ceux de TASK-107.
fn load_sound_effect_bank(
    assets: Res<AssetServer>,
    mut backend: ResMut<AudioBackendHandle>,
    mut commands: Commands,
) {
    let backend = &mut backend.0;
    let mut load = |path: &str| backend.load_clip(&assets, path);
    commands.insert_resource(SoundEffectBank {
        dice_rolls: [
            "audio/dice_roll_01.ogg",
            "audio/dice_roll_02.ogg",
            "audio/dice_roll_03.ogg",
            "audio/dice_roll_04.ogg",
            "audio/dice_roll_05.ogg",
            "audio/dice_roll_06.ogg",
        ]
        .map(&mut load),
        chip_tick: load("audio/chip_tick.ogg"),
        hand_base_chord: load("audio/hand_base_chord.ogg"),
        relic_chord: load("audio/relic_chord.ogg"),
        mult_hit: load("audio/mult_hit.ogg"),
        seal_tick: load("audio/seal_tick.ogg"),
        die_lock: load("audio/die_lock.ogg"),
        hand_consumed: load("audio/hand_consumed.ogg"),
        ui_hover: load("audio/ui_hover.ogg"),
        ui_click: load("audio/ui_click.ogg"),
        coin: load("audio/coin.ogg"),
        victory_fanfare: load("audio/victory_fanfare.ogg"),
        rng: SmallRng::from_rng(&mut rand::rng()),
    });
}

/// Enregistre les systèmes de ce fichier. Appelée par `GameAudioPlugin::build`.
pub fn sfx_plugin(app: &mut App) {
    app.add_systems(Startup, load_sound_effect_bank);
}
