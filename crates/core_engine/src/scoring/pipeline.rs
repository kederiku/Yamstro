//! Passe A : production pure des effets, sans aucune mutation.

use crate::blind::BlindContext;
use crate::hands::{HandLevels, YahtzeeHand};
use crate::scoring::levels::resolved_base;
use crate::scoring::{ScoreAction, ScoreEffect, StepSource};

/// Premier segment de la passe A : la base de la figure, en deux effets.
///
/// L'ordre `AddChips` puis `AddMult` donne le même score final que l'inverse
/// mais un **journal différent** ; il est fixé par l'exemple résolu, pas par
/// l'arithmétique. C'est aussi ce qui donne au premier pas du journal un score
/// nul, le Mult n'étant pas encore posé.
///
/// Le couple ainsi calculé alimentera les champs homonymes du contexte de
/// déclenchement pour toute la suite de la résolution.
// La fonction attend son appelant : TASK-23 écrit la suite de la passe A dans
// ce même fichier et la consommera. Seul son test l'appelle aujourd'hui, ce que
// `-D warnings` refuse. **TASK-23 doit retirer cet attribut.**
#[allow(dead_code)]
pub(crate) fn base_effects(
    hand: YahtzeeHand,
    hand_levels: &HandLevels,
    blind: &BlindContext,
) -> Vec<ScoreEffect> {
    let (base_chips, base_mult) = resolved_base(hand, hand_levels, blind);
    let source = StepSource::HandBase { hand };

    vec![
        ScoreEffect {
            source,
            action: ScoreAction::AddChips(base_chips),
        },
        ScoreEffect {
            source,
            action: ScoreAction::AddMult(base_mult),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blind::BlindContext;
    use crate::hands::{HandLevels, YahtzeeHand};
    use crate::scoring::{ScoreAction, StepSource};
    use smallvec::smallvec;

    #[test]
    fn test_base_emits_chips_then_mult() {
        let levels = HandLevels::default();
        let blind = BlindContext {
            modifiers: smallvec![],
        };

        let effects = base_effects(YahtzeeHand::FullHouse, &levels, &blind);

        // L'ordre Chips puis Mult donne le même score final que l'inverse, mais
        // un journal différent : il est fixé par l'exemple normatif.
        assert_eq!(effects.len(), 2);
        assert_eq!(
            effects[0].source,
            StepSource::HandBase {
                hand: YahtzeeHand::FullHouse
            }
        );
        assert_eq!(effects[0].action, ScoreAction::AddChips(30));
        assert_eq!(
            effects[1].source,
            StepSource::HandBase {
                hand: YahtzeeHand::FullHouse
            }
        );
        assert_eq!(effects[1].action, ScoreAction::AddMult(400));
    }
}
