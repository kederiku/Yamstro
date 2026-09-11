//! *Yams Divin* (Rare) — ×3 Mult sur un Yams.
//!
//! **Elle multiplie le Mult, jamais le score total.** La formule
//! `Score = Chips × Mult` est invariante : aucune action ne remplace la formule,
//! et il n'existe pas d'opération de multiplication du total. Le libellé de
//! conception « ×3 le score total » se réécrit en une multiplication du Mult
//! **avant** d'entrer dans le moteur.

use smallvec::SmallVec;

use crate::hands::YahtzeeHand;
use crate::relics::RelicId;
use crate::scoring::{Hook, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

pub(crate) fn effects(hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]> {
    let mut effects = SmallVec::new();
    if hook != Hook::OnHandScored || ctx.hand.hand != YahtzeeHand::Yahtzee {
        return effects;
    }

    effects.push(ScoreEffect {
        source: StepSource::Relic {
            uid: ctx.uid,
            def: RelicId::DivineYahtzee,
        },
        action: ScoreAction::MultiplyMult(300),
    });
    effects
}
