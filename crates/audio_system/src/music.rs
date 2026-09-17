//! Les quatre couches musicales et leurs gains, fonction pure de l'état : TASK-99 à TASK-101,
//! TASK-106.
//!
//! Les index sont ceux de tout l'étage audio : **0 Base, 1 Mélodie, 2 Tension, 3 Climax**.
//!
//! # Une fonction pure, et rien d'autre
//!
//! Mêmes entrées, mêmes sorties : aucun état interne, aucune hystérésis, aucune mémoire de
//! l'image précédente. Les gains cibles se recalculent à chaque image et ne dépendent jamais de
//! l'historique ; c'est ce qui rend le mixage testable sans jouer un son et impossible à
//! désynchroniser de l'état. Le lissage est le rôle de l'interpolation, en aval.
//!
//! L'audio ne calcule jamais un score et n'anticipe jamais un total en cours : il lit le score
//! commis, qui n'est écrit qu'une fois la file de dépilement vide (ADR-010). Le Climax monte donc
//! au commit de la main, pas pendant le décompte.
//!
//! # Quand une blind compte : [`blind_in_play`]
//!
//! [`target_gains`] pose Tension et Climax **dès qu'on lui passe un contexte**, quelle que soit
//! la phase. Or le contexte de blind n'est jamais retiré du monde : inséré à l'entrée de la
//! sélection de blind, il survit à la boutique, à la fin de run et au menu suivant. Passé tel
//! quel, il ferait sonner le Climax à chaque visite de boutique, la blind venant d'y être battue.
//! **L'appelant filtre donc par [`blind_in_play`]**, et jamais autrement :
//!
//! ```
//! # use audio_system::music::{blind_in_play, target_gains};
//! # use game_state::states::{AppState, RunPhase};
//! # let (app, phase, blind) = (AppState::InRun, Some(RunPhase::Shop), None);
//! let gains = target_gains(app, phase, blind_in_play(app, phase, blind));
//! # assert_eq!(gains, [0.5, 0.5, 0.0, 0.0]);
//! ```
//!
//! # Les quatre voies : créées une fois, rattachées à rien
//!
//! [`AdaptiveMusicManager`] est une ressource, et les quatre voies naissent au démarrage, hors de
//! toute portée d'état. **Aucune entité musicale ne porte de marqueur de despawn lié à un état**
//! (`DespawnOnEnter`, `DespawnOnExit`, ou un marqueur maison équivalent), ni ici ni dans ce que
//! la façade crée pour une voie : la voie s'arrêterait au premier changement
//! de phase, au milieu d'une blind, sans une erreur ni une ligne de log. Le marqueur le plus
//! tentant est celui des entités de run, rattachées à la sortie de la run : la musique, elle,
//! joue aussi au menu, et ne se rattache à rien. Une voie détruite ne renaît pas : les couches
//! ne partent qu'une fois.
//!
//! L'audio est un pur lecteur d'état : cette crate n'écrit aucune transition.
//!
//! Le retour de la fin de manche au lancer n'est jamais une transition d'un état vers lui-même,
//! contrairement à ce qu'on lit parfois ; le piège des transitions réflexives est réel, mais il
//! n'est qu'un cas du précédent. `tests/real_backend.rs` fait le tour complet des états sur le
//! son rendu, sortie de run et transition réflexive comprises.
//!
//! # [`music_plugin`], le point d'enregistrement de ce fichier
//!
//! `GameAudioPlugin::build` l'appelle, et les systèmes de ce fichier s'y branchent : ceux de
//! TASK-101 et de TASK-106 s'ajoutent ici, jamais dans `lib.rs`. Un système enregistré nulle
//! part ne tourne jamais, et rien ne le dit.

use std::time::Duration;

use bevy::prelude::*;
use core_engine::blinds::{BlindContext, BlindType};
use game_state::states::{AppState, RunPhase};

use crate::backend::{AudioBackendHandle, LayerHandle};

pub const CLIMAX_THRESHOLD_PERCENT: u128 = 75; // seule occurrence du seuil dans le dépôt

/// Les quatre stems, **dans l'ordre des index** : Base, Mélodie, Tension, Climax. Chemins
/// relatifs à la racine des assets ; les fichiers sont ceux de TASK-107. Une permutation ne casse
/// aucune compilation : le mix lèverait la Tension à la place de la Mélodie.
pub const STEM_PATHS: [&str; 4] = [
    "audio/stem_base.ogg",
    "audio/stem_melody.ogg",
    "audio/stem_tension.ogg",
    "audio/stem_climax.ogg",
];

/// Vitesse du fondu, par seconde : une couche est à 95 % de sa cible au bout de 1,5 s, la seule
/// grandeur que l'étape mesure. Aucun test ne dépend de sa valeur.
pub const DEFAULT_FADE_PER_SECOND: f32 = 2.0;

/// Durée du ducking de fanfare : 1,2 s. En millisecondes entières, donc exacte.
pub const DUCK_DURATION: Duration = Duration::from_millis(1_200);

/// L'état musical. `Resource` seule : en 0.19 elle est un sous-trait de `Component`, et un type
/// ne dérive pas les deux.
///
/// **Ni `Default`, ni `init_resource`** : les quatre [`LayerHandle`] viennent du backend, et un
/// minuteur par défaut aurait une durée nulle, donc un ducking qui ne dure rien. Insérée par
/// `setup_music_layers`, et par personne d'autre.
///
/// `target_gains` est le champ, [`target_gains`] la fonction libre : le champ mémorise ce que la
/// fonction a rendu à l'image courante. `layers[i]`, `current_gains[i]` et `target_gains[i]`
/// désignent toujours la même couche, celle de `STEM_PATHS[i]`.
#[derive(Resource)]
pub struct AdaptiveMusicManager {
    pub layers: [LayerHandle; 4],
    pub current_gains: [f32; 4],
    pub target_gains: [f32; 4],
    pub fade_per_second: f32,
    pub duck: f32,
    pub duck_timer: Timer,
}

/// Le minuteur du ducking, **né terminé** : sans cela les 1,2 premières secondes de chaque
/// lancement sortiraient atténuées, et la garde de réentrance de la fanfare (TASK-106) avalerait
/// la première de la partie.
///
/// Terminé **par un `tick`** : l'état « terminé » n'est recalculé que là, écrire l'horloge ne
/// suffit pas. Le second `tick`, de durée nulle, efface le « vient de se terminer » du premier :
/// au lancement, aucun ducking ne vient de finir.
fn finished_duck_timer() -> Timer {
    let mut timer = Timer::new(DUCK_DURATION, TimerMode::Once);
    timer.tick(DUCK_DURATION);
    timer.tick(Duration::ZERO);
    timer
}

/// Charge **et** démarre les quatre voies, une fois pour la durée du processus, puis insère la
/// ressource. C'est ce système, et lui seul, qui lie un fichier à une voie : sans ces quatre
/// appels les gains se pousseraient sur du silence, et le jeu serait muet sous des tests verts.
///
/// Les gains naissent à zéro : la bande-son monte en fondu depuis le silence (TASK-101).
fn setup_music_layers(
    assets: Res<AssetServer>,
    mut backend: ResMut<AudioBackendHandle>,
    mut commands: Commands,
) {
    let backend = &mut backend.0;
    let layers =
        std::array::from_fn(|index| backend.load_layer(&assets, STEM_PATHS[index], index as u8));
    commands.insert_resource(AdaptiveMusicManager {
        layers,
        current_gains: [0.0; 4],
        target_gains: [0.0; 4],
        fade_per_second: DEFAULT_FADE_PER_SECOND,
        duck: 1.0,
        duck_timer: finished_duck_timer(),
    });
}

/// Enregistre les systèmes de ce fichier. Appelée par `GameAudioPlugin::build`.
pub fn music_plugin(app: &mut App) {
    app.add_systems(Startup, setup_music_layers);
}

/// Gains cibles des 4 couches. Fonction pure : mêmes entrées, mêmes sorties.
pub fn target_gains(
    app: AppState,
    phase: Option<RunPhase>,
    blind: Option<&BlindContext>,
) -> [f32; 4] {
    let mut g = match (app, phase) {
        (AppState::MainMenu, _) | (AppState::CupSelect, _) => [0.6, 0.4, 0.0, 0.0],
        (AppState::InRun, Some(RunPhase::Shop)) => [0.5, 0.5, 0.0, 0.0],
        (AppState::InRun, Some(_)) => [1.0, 1.0, 0.0, 0.0],
        _ => [0.0, 0.0, 0.0, 0.0],
    };
    if let Some(b) = blind {
        // Tension : Mise Boss, ou dernière main de la blind.
        if b.blind.kind == BlindType::Boss || b.hands_remaining <= 1 {
            g[2] = 1.0;
        }
        // Climax : le seuil du score cible. Arithmétique entière, aucun flottant.
        if (b.current_score as u128) * 100 >= (b.target_score as u128) * CLIMAX_THRESHOLD_PERCENT {
            g[3] = 1.0;
        }
    }
    g
}

/// Le contexte de blind, **seulement quand une blind est en jeu** : en run, de la sélection de
/// blind à la fin de manche. `None` en boutique et hors d'une run, où le contexte présent dans
/// le monde est celui d'une manche finie.
///
/// La sélection de blind compte : le contexte y est déjà celui de la blind à venir, et un Boss
/// annoncé lève la Tension. La fin de manche aussi : sur une blind battue, le Climax y tient.
pub fn blind_in_play(
    app: AppState,
    phase: Option<RunPhase>,
    blind: Option<&BlindContext>,
) -> Option<&BlindContext> {
    match (app, phase) {
        (
            AppState::InRun,
            Some(RunPhase::BlindSelect | RunPhase::Roll | RunPhase::Scoring | RunPhase::RoundEnd),
        ) => blind,
        _ => None,
    }
}
