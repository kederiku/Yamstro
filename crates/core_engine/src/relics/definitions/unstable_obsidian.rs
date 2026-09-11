//! *Obsidienne Instable* (Rare) — ×2 Mult, sans condition.
//!
//! **Elle n'émet ici que sa multiplication.** Son malus de relance passe par une
//! fonction distincte, dans ce même fichier, que TASK-61 écrira : les deux ne
//! partagent que le fichier, et le modificateur de lancer n'est pas un effet de
//! score.

use smallvec::SmallVec;

use crate::relics::RelicId;
use crate::scoring::{Hook, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

pub(crate) fn effects(hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]> {
    let mut effects = SmallVec::new();
    if hook != Hook::OnHandScored {
        return effects;
    }

    effects.push(ScoreEffect {
        source: StepSource::Relic {
            uid: ctx.uid,
            def: RelicId::UnstableObsidian,
        },
        action: ScoreAction::MultiplyMult(200),
    });
    effects
}
