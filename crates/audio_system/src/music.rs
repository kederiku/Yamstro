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

use core_engine::blinds::{BlindContext, BlindType};
use game_state::states::{AppState, RunPhase};

pub const CLIMAX_THRESHOLD_PERCENT: u128 = 75; // seule occurrence du seuil dans le dépôt

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
