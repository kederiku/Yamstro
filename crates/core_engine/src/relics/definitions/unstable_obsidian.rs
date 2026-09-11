//! *Obsidienne Instable* (Rare) — ×2 Mult, sans condition.
//!
//! **Deux corps disjoints, un seul fichier.** La multiplication sort d'`effects`,
//! le malus de relance de `roll_modifier` : les deux ne partagent aucune
//! variable, et un modificateur de lancer n'est pas un effet de score. C'est
//! aussi pourquoi *Miroir Double*, qui ne miroite que les effets, double le
//! `MultiplyMult` sans jamais toucher au malus.

use smallvec::SmallVec;

use crate::relics::RelicId;
use crate::scoring::{Hook, RollModifier, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

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

/// Une relance en moins, **sans condition** : ni le rang du lancer, ni les
/// faces, ni l'état de la relique n'entrent en compte.
///
/// Le delta est rendu **signé et négatif**. Le convertir en `u8` ici perdrait
/// le signe et obligerait l'appelant à le réinterpréter ; c'est `apply_delta`,
/// au dernier maillon de la chaîne de l'ADR-007, qui sature — *Gobelet
/// Abandonné* plus cette relique rend zéro, jamais 255.
pub(crate) fn roll_modifier() -> RollModifier {
    RollModifier {
        reroll_delta: -1,
        ..RollModifier::default()
    }
}
