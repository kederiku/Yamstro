//! *Architecte du Full* (Peu commune) — quarante Chips puis cinq Mult sur un Full.
//!
//! Brut de 40 Chips et 5 Mult, soit 90 unités de compte, pour une fréquence
//! d'environ un sixième : plafond de 240, donc **38 %**.
//!
//! # L'ordre des deux effets est normatif
//!
//! Les Chips **puis** le Mult, dans cet ordre dans la file. La passe B replie
//! les effets séquentiellement, un palier par effet, et la mise en scène de
//! l'Étape 4 les anime dans cet ordre : l'inverser change le journal, et
//! changera le **score** dès qu'une relique multiplicative pourra se glisser
//! entre les deux.
//!
//! La capacité en ligne de la file couvre exactement ces deux effets : aucune
//! allocation sur le tas.

use smallvec::SmallVec;

use crate::hands::YahtzeeHand;
use crate::relics::RelicId;
use crate::scoring::{Hook, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

pub(crate) fn effects(hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]> {
    let mut effects = SmallVec::new();
    if hook != Hook::OnHandScored || ctx.hand.hand != YahtzeeHand::FullHouse {
        return effects;
    }

    let source = StepSource::Relic {
        uid: ctx.uid,
        def: RelicId::FullHouseArchitect,
    };
    // Les Chips ne sont pas en centièmes, le Mult l'est.
    effects.push(ScoreEffect {
        source,
        action: ScoreAction::AddChips(40),
    });
    effects.push(ScoreEffect {
        source,
        action: ScoreAction::AddMult(500),
    });
    effects
}
