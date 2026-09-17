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
//!
//! # Le lecteur des paliers de score
//!
//! `play_score_steps` est le seul point d'entrée de l'audio depuis la mise en scène : elle publie
//! un palier par effet dépilé, et ne joue jamais un son. Le flux est à sens unique, rien ne
//! remonte d'ici vers elle. Pour chaque palier, dans l'ordre : **lire la hauteur, jouer, puis
//! avancer d'un demi-ton**, quelle que soit la source. Avancer d'abord ferait sonner l'accord
//! d'ouverture un demi-ton au-dessus du nominal, à chaque main.
//!
//! - **Le timbre vient de la source** : quatre sources, quatre clips, jamais un échantillon
//!   partagé. C'est le grain qui dit au joueur ce qui vient de se déclencher, pas la hauteur.
//! - **L'intensité vient de l'action**, et d'elle seule : un plancher audible et une part bornée,
//!   **normalisée par unité**. Les trois actions ne parlent pas la même unité (jetons bruts,
//!   centièmes de multiplicateur, pourcentage) : comparées sur une même échelle, quatre points de
//!   multiplicateur sonneraient comme quatre cents jetons.
//! - **Un palier multiplicatif change de clip et de routage** : le coup percussif, sur le
//!   routage réverbéré. La hauteur, elle, suit toujours la source : la figure de base à la
//!   hauteur nominale, un sceau à la hauteur courante teintée, le reste à la hauteur courante.
//!   Un coup multiplicatif continue donc la gamme au lieu de la casser.
//!
//! Les deux `match` sont exhaustifs, sans bras générique : une variante ajoutée plus tard doit
//! casser la compilation ici, pas partir en silence sur un timbre par défaut. L'identité d'une
//! relique est ignorée : un `match` sur elle grossirait avec le catalogue de l'Étape 9.
//!
//! Le lecteur tourne **après** le dépileur de la mise en scène, dans la même image : sans cette
//! arête le tampon garderait les paliers une image de plus, rien ne casserait, et le son
//! partirait une image après l'impulsion visuelle. C'est une **arête d'ordre**, jamais une
//! appartenance à l'ensemble du dépileur, qui ferait hériter le lecteur de sa condition d'état.

use bevy::prelude::*;
use core_engine::{
    dice::DieSeal,
    scoring::{ScoreAction, StepSource},
};
use game_state::states::RunPhase;
use ui_and_juice::{JuiceSet, events::ScoreStepPlayed};

use crate::backend::{AudioBackendHandle, AudioClip, Bus};
use crate::bus::local_gain;
use crate::sfx::SoundEffectBank;

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

/// Le plancher d'intensité d'un palier additif, et celui d'un palier multiplicatif : aucun
/// palier n'est inaudible, et rien ne dépasse l'unité en fin de run.
const ADDITIVE_FLOOR: f32 = 0.6;
const MULTIPLY_FLOOR: f32 = 0.8;
/// Ce qui donne le plein volume, **dans l'unité de chaque action** : cent jetons, dix points de
/// multiplicateur en centièmes, et un facteur de trois, soit deux cents points de pourcentage
/// au-dessus de l'unité.
const FULL_CHIPS: u64 = 100;
const FULL_MULT_HUNDREDTHS: u64 = 1_000;
const FULL_MULTIPLY_PERCENT: u64 = 200;

/// La part d'une grandeur dans son plein volume, bornée à l'unité.
fn share(value: u64, full: u64) -> f32 {
    value.min(full) as f32 / full as f32
}

/// L'intensité d'un palier, dérivée de l'action seule. Une perte sonne comme un gain de même
/// taille ; un facteur sous l'unité sonne au plancher.
pub fn step_intensity(action: ScoreAction) -> f32 {
    let (floor, part) = match action {
        ScoreAction::AddChips(chips) => (ADDITIVE_FLOOR, share(chips, FULL_CHIPS)),
        ScoreAction::AddMult(hundredths) => (
            ADDITIVE_FLOOR,
            share(hundredths.unsigned_abs(), FULL_MULT_HUNDREDTHS),
        ),
        ScoreAction::MultiplyMult(percent) => (
            MULTIPLY_FLOOR,
            share(
                u64::from(percent.saturating_sub(100)),
                FULL_MULTIPLY_PERCENT,
            ),
        ),
    };
    floor + (1.0 - floor) * part
}

/// La teinte d'un sceau : un facteur de hauteur petit, distinct pour chacun, écrit une fois. Un
/// demi-ton vaut près de six pour cent : une teinte n'entre pas en concurrence avec la gamme.
pub fn seal_tint(seal: DieSeal) -> f32 {
    match seal {
        DieSeal::Gold => 1.02,
        DieSeal::Blue => 1.01,
        DieSeal::Purple => 0.99,
        DieSeal::Red => 0.98,
    }
}

/// Le clip d'un palier : l'action d'abord, la source ensuite.
pub fn step_clip(bank: &SoundEffectBank, source: StepSource, action: ScoreAction) -> AudioClip {
    match action {
        ScoreAction::MultiplyMult(_) => bank.mult_hit,
        ScoreAction::AddChips(_) | ScoreAction::AddMult(_) => match source {
            StepSource::HandBase { .. } => bank.hand_base_chord,
            StepSource::Die { .. } => bank.chip_tick,
            StepSource::Relic { .. } => bank.relic_chord,
            StepSource::Seal { .. } => bank.seal_tick,
        },
    }
}

/// Le routage d'un palier : seul le coup multiplicatif résonne.
pub fn step_bus(action: ScoreAction) -> Bus {
    match action {
        ScoreAction::MultiplyMult(_) => Bus::SfxReverb,
        ScoreAction::AddChips(_) | ScoreAction::AddMult(_) => Bus::Sfx,
    }
}

/// La hauteur d'un palier : elle suit la source, quel que soit le clip.
pub fn step_pitch(tracker: &PitchScaleTracker, source: StepSource) -> f32 {
    match source {
        StepSource::HandBase { .. } => tracker.base_pitch,
        StepSource::Die { .. } | StepSource::Relic { .. } => tracker.pitch(),
        StepSource::Seal { seal, .. } => tracker.pitch() * seal_tint(seal),
    }
}

/// Lit les paliers publiés par la mise en scène, et en dérive seul le clip, le routage,
/// l'intensité et la hauteur. Voir la doc de tête.
fn play_score_steps(
    mut events: MessageReader<ScoreStepPlayed>,
    mut pitch: ResMut<PitchScaleTracker>,
    bank: Res<SoundEffectBank>,
    mut backend: ResMut<AudioBackendHandle>,
) {
    for step in events.read() {
        let clip = step_clip(&bank, step.source, step.action);
        let bus = step_bus(step.action);
        let volume = local_gain(step_intensity(step.action));
        let height = step_pitch(&pitch, step.source);
        backend.0.play(clip, bus, volume, height);
        pitch.advance();
    }
}

/// Insère la ressource, enregistre le reset et le lecteur. Appelée par `GameAudioPlugin::build`.
pub fn pitch_plugin(app: &mut App) {
    app.init_resource::<PitchScaleTracker>()
        .add_systems(OnEnter(RunPhase::Scoring), reset_pitch_on_scoring_entry);
    app.add_systems(Update, play_score_steps.after(JuiceSet::TickQueue));
}
