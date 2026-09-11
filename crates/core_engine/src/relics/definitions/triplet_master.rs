//! *Maître du Brelan* (Commune) — six Mult sur un Brelan ou un Carré.
//!
//! Une relique **conditionnelle** peut dépasser son budget d'un facteur égal à
//! l'inverse de sa fréquence, plafonné à trois. Six Mult valent 60 unités de
//! compte pour une fréquence d'environ un tiers, donc un plafond de 120 : elle
//! en occupe **50 %**.
//!
//! **La condition porte sur la figure retenue**, pas sur les dés. Un Full
//! contient un brelan ; recomposer la figure à partir des faces ferait parler
//! cette relique sur un Full, ce que le joueur n'a pas demandé.

use smallvec::SmallVec;

use crate::hands::YahtzeeHand;
use crate::relics::RelicId;
use crate::scoring::{Hook, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

pub(crate) fn effects(hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]> {
    let mut effects = SmallVec::new();
    if hook != Hook::OnHandScored {
        return effects;
    }

    if matches!(
        ctx.hand.hand,
        YahtzeeHand::ThreeOfAKind | YahtzeeHand::FourOfAKind
    ) {
        effects.push(ScoreEffect {
            source: StepSource::Relic {
                uid: ctx.uid,
                def: RelicId::TripletMaster,
            },
            // Le Mult est en centièmes : six Mult s'écrivent six cents.
            action: ScoreAction::AddMult(600),
        });
    }
    effects
}
