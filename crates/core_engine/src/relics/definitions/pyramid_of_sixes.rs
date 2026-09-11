//! *La Pyramide de Six* (Peu commune) — quinze Chips par six comptabilisé.
//!
//! **Le faible rendement nominal est voulu.** Sur main uniforme l'espérance est
//! de 0,83 six, soit 12,5 unités de compte sur un budget Peu commune de 80 —
//! 16 %. Dès trois six comptabilisés elle rend 45 unités, soit 56 % : c'est une
//! relique de construction autour des six, pas une relique généraliste.
//!
//! Elle teste la face **six**, jamais « la face maximale » : un dé à huit faces
//! montrant huit ne la déclenche pas.
//!
//! Sa valeur diffère volontairement de celle de la fixture homonyme du moteur,
//! et cet écart est ce qui rend une confusion entre les deux détectable.

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

    if value == 6 {
        effects.push(ScoreEffect {
            source: StepSource::Relic {
                uid: ctx.uid,
                def: RelicId::PyramidOfSixes,
            },
            action: ScoreAction::AddChips(15),
        });
    }
    effects
}
