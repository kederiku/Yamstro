//! *La Pierre Polie* (Commune) — dix Chips par dé pair comptabilisé.
//!
//! Espérance de 2,5 dés pairs sur la main de référence, soit 25 Chips ≡ 2,5
//! unités de compte sur un budget Commune de 40 : **63 %**, symétrique du *Dé
//! Fêlé*.
//!
//! Les Chips ne sont pas en centièmes : dix Chips s'écrivent dix.

use smallvec::SmallVec;

use crate::relics::RelicId;
use crate::scoring::{Hook, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

pub(crate) fn effects(hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]> {
    let mut effects = SmallVec::new();
    if hook != Hook::OnScoringDie {
        return effects;
    }
    let Some((_, value)) = ctx.die else {
        return effects;
    };

    if value % 2 == 0 {
        effects.push(ScoreEffect {
            source: StepSource::Relic {
                uid: ctx.uid,
                def: RelicId::PolishedStone,
            },
            action: ScoreAction::AddChips(10),
        });
    }
    effects
}
