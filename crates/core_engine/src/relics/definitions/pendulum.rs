//! *Le Balancier* (Rare) — ×1,5 Mult quand la somme des dés comptabilisés est paire.
//!
//! # Trois divergences écartées, et elles reviennent ensemble
//!
//! Le cahier des charges d'origine appelait cette relique *La Balance*, lui
//! donnait ×2, et faisait porter sa condition sur un « total de points ». Les
//! trois lectures ont été tranchées contre lui : le nom retenu est *Le
//! Balancier*, la valeur ×1,5, et la condition porte sur la **somme des faces
//! des dés comptabilisés**. Ces trois divergences viennent du même document et
//! se réintroduisent ensemble ; trois gardes distinctes les attrapent
//! séparément.
//!
//! # Pourquoi les dés comptabilisés et non le plateau
//!
//! Un dé écarté n'a aucune identité de score. La différence est mesurable sur la
//! main de référence : `[4,4,4,6,1]` donne 12 pour les dés comptabilisés —
//! **pair** — et 19 pour le plateau — **impair**. Une lecture du plateau ne
//! déclencherait donc pas, rendrait un Mult de 800 au lieu de 1200, et un score
//! de 176 au lieu de 264.
//!
//! Les dés sont retrouvés par **recherche sur l'identité**, jamais par
//! indexation : le pool en retire et en ajoute en cours de manche, et un
//! identifiant introuvable est ignoré sans interrompre le calcul.

use smallvec::SmallVec;

use crate::relics::RelicId;
use crate::scoring::{Hook, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

pub(crate) fn effects(hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]> {
    let mut effects = SmallVec::new();
    if hook != Hook::OnHandScored {
        return effects;
    }

    let somme = ctx
        .hand
        .scoring_dice
        .iter()
        .filter_map(|id| ctx.dice.iter().find(|de| de.id == *id))
        .fold(0u32, |total, de| {
            total.saturating_add(u32::from(de.current_value))
        });

    if somme % 2 == 0 {
        effects.push(ScoreEffect {
            source: StepSource::Relic {
                uid: ctx.uid,
                def: RelicId::Pendulum,
            },
            // ×1,5 s'écrit en pourcentage entier : aucun flottant n'entre dans
            // le calcul du score.
            action: ScoreAction::MultiplyMult(150),
        });
    }
    effects
}
