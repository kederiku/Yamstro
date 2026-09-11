//! *L'Alignement Stellaire* (Peu commune) — huit Mult sur une suite.
//!
//! Brut de 80 unités de compte pour une fréquence d'environ un sixième, donc un
//! plafond de 240 : **33 %**.
//!
//! # Cette relique était multiplicative, et ne doit plus le redevenir
//!
//! **La version d'origine valait `MultiplyMult(175)`, soit ×1,75 Mult.** Elle
//! est devenue additive parce que la multiplication est réservée aux raretés
//! Rare et Légendaire, et celle-ci est Peu commune. Le changement est
//! délibéré.
//!
//! Cette note nomme la valeur d'origine **exprès** : c'est le seul moyen qu'un
//! lecteur qui retrouvera « ×1,75 » dans d'anciennes notes de conception
//! rapproche les deux et s'arrête. Une note qui dirait seulement « elle
//! multipliait autrefois » ne serait pas reconnaissable. La garde du volet 1
//! est ancrée en conséquence sur l'**émission** d'une multiplication, pas sur
//! le nom — sans quoi ce paragraphe ne pourrait pas exister.
//!
//! Restaurer la version multiplicative compilerait sans un mot.

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
        YahtzeeHand::SmallStraight | YahtzeeHand::LargeStraight
    ) {
        effects.push(ScoreEffect {
            source: StepSource::Relic {
                uid: ctx.uid,
                def: RelicId::StellarAlignment,
            },
            action: ScoreAction::AddMult(800),
        });
    }
    effects
}
