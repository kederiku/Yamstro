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
//!
//! # Les sons pilotés par l'observation de l'état
//!
//! La case consommée, le lancer de dés et le verrouillage d'un dé s'entendent par **lecture de
//! l'état**, sur un seul patron : comparer l'état courant à une copie locale de l'image
//! précédente, jouer sur la différence, réécrire la copie **après** la boucle. Aucun événement
//! n'est créé ni émis, aucune crate amont n'est touchée : l'audio reste un pur lecteur. Une copie
//! locale appartient à l'instance du système, elle survit à toutes les transitions d'état.
//!
//! - **La case consommée** : un bit nouvellement posé dans la grille de la blind. Le contexte de
//!   blind est remplacé à chaque blind par un contexte neuf, à la grille vide : aucun bit n'y est
//!   *nouvellement* posé, donc aucun son parasite, et la copie se remet en phase d'elle-même en
//!   une image. Un loquet à hauteur fixe : transposé, il cesserait d'être reconnaissable.
//! - **Le lancer** : l'entrée dans la phase de lancer, où les dés roulent, ou une baisse du
//!   compteur de relances à phase constante. Le compteur seul ne suffirait pas : sur un gobelet à
//!   zéro relance il ne bouge jamais, et le lancer initial serait muet. Le clip et sa hauteur
//!   viennent de [`SoundEffectBank::dice_roll`], qui a ici son seul appelant.
//! - **Le verrouillage** : un dé dont le champ `locked` passe à vrai. Le déverrouillage est muet,
//!   et c'est le champ qui est observé, pas le marqueur posé avec lui : deux observateurs pour un
//!   fait divergent au premier site qui bouge.
//!
//! La phase de run, le contexte de blind et le contexte de main sont optionnels : pris nus, ils
//! feraient paniquer le jeu au menu principal. Ce qui part à la façade est la part locale du
//! volume, jamais les curseurs, qui s'appliquent sur les bus.
//!
//! **Trois clips restent sans appelant à la fin de l'étape**, et ce n'est pas un oubli : le
//! survol et la validation d'interface attendent l'Étape 11, et la pièce de la boutique a pour
//! observable l'or de la session de run, que cette crate s'interdit de nommer ; son déclenchement
//! viendra du côté de la boutique.
//!
//! **Ouvert pour l'Étape 10.** À la restauration d'une sauvegarde, la grille et les dés
//! réapparaîtront avec plusieurs bits déjà posés, et tout sonnerait d'un coup. La détection de
//! changement ne distingue pas ce cas (une ressource remplacée est « changée », jamais
//! « ajoutée », comme après un marquage légitime) : c'est à la restauration de le dire.

use bevy::prelude::*;
use core_engine::{
    blinds::BlindContext,
    dice::{Die, DieId},
    hands::{HandGrid, YahtzeeHand},
};
use game_state::{resources::HandContext, states::RunPhase};
use rand::{RngExt, SeedableRng, rngs::SmallRng};

use crate::backend::{AudioBackendHandle, AudioClip, Bus};
use crate::bus::local_gain;

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

/// La case consommée : un loquet par bit nouvellement posé. Bloc du document de l'étape.
fn play_hand_consumed(
    blind: Option<Res<BlindContext>>,
    mut seen: Local<HandGrid>, // HandGrid : bitset des 13 figures (Étape 1)
    bank: Res<SoundEffectBank>,
    mut backend: ResMut<AudioBackendHandle>,
) {
    let Some(blind) = blind else {
        return;
    };
    let latch = bank.hand_consumed;
    for hand in YahtzeeHand::ALL {
        if blind.used_hands.contains(hand) && !seen.contains(hand) {
            backend.0.play(latch, Bus::Sfx, local_gain(1.0), 1.0);
        }
    }
    *seen = blind.used_hands;
}

/// Ce que `play_dice_roll` a vu à l'image précédente. Hors d'une run, les deux valent `None` :
/// c'est l'état initial, et le retour en jeu repart d'une entrée dans la phase de lancer.
#[derive(Default)]
struct RollWitness {
    phase: Option<RunPhase>,
    rerolls_left: Option<u8>,
}

/// Le lancer : un son par entrée dans la phase de lancer, un par relance. Jamais deux pour une
/// même image, les deux raisons partageant un seul `if`, et aucun pour une relance refusée, qui
/// ne décrémente rien.
fn play_dice_roll(
    phase: Option<Res<State<RunPhase>>>,
    hand: Option<Res<HandContext>>,
    mut seen: Local<RollWitness>,
    mut bank: ResMut<SoundEffectBank>,
    mut backend: ResMut<AudioBackendHandle>,
) {
    let now = RollWitness {
        phase: phase.map(|p| *p.get()),
        rerolls_left: hand.map(|h| h.rerolls_left),
    };
    let rolling = now.phase == Some(RunPhase::Roll);
    let entered = rolling && seen.phase != Some(RunPhase::Roll);
    let rerolled = rolling
        && matches!((seen.rerolls_left, now.rerolls_left), (Some(before), Some(after)) if after < before);
    if entered || rerolled {
        let (clip, pitch) = bank.dice_roll();
        backend.0.play(clip, Bus::Sfx, local_gain(1.0), pitch);
    }
    *seen = now;
}

/// Le verrouillage : un loquet par dé nouvellement verrouillé. Le déverrouillage est muet.
fn play_die_lock(
    dice: Query<&Die>,
    mut seen: Local<Vec<DieId>>,
    bank: Res<SoundEffectBank>,
    mut backend: ResMut<AudioBackendHandle>,
) {
    let locked: Vec<DieId> = dice
        .iter()
        .filter(|die| die.locked)
        .map(|die| die.id)
        .collect();
    let latch = bank.die_lock;
    for id in &locked {
        if !seen.contains(id) {
            backend.0.play(latch, Bus::Sfx, local_gain(1.0), 1.0);
        }
    }
    *seen = locked;
}

/// Enregistre les systèmes de ce fichier. Appelée par `GameAudioPlugin::build`.
pub fn sfx_plugin(app: &mut App) {
    app.add_systems(Startup, load_sound_effect_bank);
    app.add_systems(Update, (play_hand_consumed, play_dice_roll, play_die_lock));
}
