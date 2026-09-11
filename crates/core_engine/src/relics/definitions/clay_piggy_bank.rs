//! *Tirelire en Terre* (Commune) — un or par relance non utilisée, plafond cinq.
//!
//! # Deux déclencheurs, deux rôles
//!
//! Elle accumule sur `OnHandScored`, une main après l'autre, et encaisse sur
//! `OnRoundEnd`, une fois par blind. Le compteur vit dans **son propre**
//! `RelicState` : deux exemplaires accumulent séparément et rendent cinq
//! chacun. Le porter ailleurs — dans la session, dans un compteur global — les
//! ferait partager une seule bourse.
//!
//! # Ce que le plafond veut dire
//!
//! `min(compteur, 5)` s'applique **au cumul de la blind**, pas main par main.
//! Encaisser à chaque main rendrait le plafond inopérant : sur quatre mains à
//! trois relances, la règle donne cinq et l'encaissement par main donnerait
//! douze. C'est ce qui fixe le site d'appel, et non l'inverse.
//!
//! # Trois points de forme
//!
//! `add_relic` rend `RelicState::None` : le premier avancement doit le lire
//! comme zéro, jamais paniquer. La remise à zéro rend `Counter(0)` et **non**
//! `None`, pour que la variante reste stable d'une blind à l'autre et que
//! l'aller-retour serde reste lisible. Et l'élargissement du `u8` des relances
//! vers le `u32` du compteur s'écrit `u32::from`, jamais une conversion
//! tronquante.

use crate::relics::RelicState;
use crate::scoring::{Hook, TriggerCtx};

const GOLD_CAP: u32 = 5;

pub(crate) fn gold(ctx: &TriggerCtx) -> u32 {
    match ctx.state {
        RelicState::Counter(n) => n.min(GOLD_CAP),
        // Énumérées, jamais un attrape-tout : une cinquième variante de
        // `RelicState` doit faire échouer la compilation ici, pas rendre zéro
        // en silence. Le bloc normatif du ticket emploie un joker ; la garde de
        // TASK-53 l'interdit, et son motif vaut pour cet enum-ci comme pour
        // `RelicId`.
        RelicState::None | RelicState::Perishable { .. } | RelicState::Disabled => 0,
    }
}

/// **L'argument `state` fait foi**, jamais `ctx.state`. Les deux désignent la
/// même chose par contrat — l'appelant bâtit toujours le contexte avec l'état
/// de l'instance courante —, mais un seul des deux est la valeur que l'appelant
/// s'apprête à réécrire.
pub(crate) fn advance(hook: Hook, ctx: &TriggerCtx, state: RelicState) -> RelicState {
    match hook {
        Hook::OnHandScored => {
            let cumul = match state {
                RelicState::Counter(n) => n,
                RelicState::None | RelicState::Perishable { .. } | RelicState::Disabled => 0,
            };
            RelicState::Counter(cumul.saturating_add(u32::from(ctx.rerolls_left)))
        }
        Hook::OnRoundEnd => RelicState::Counter(0),
        Hook::OnRoll | Hook::OnScoringDie => state,
    }
}
