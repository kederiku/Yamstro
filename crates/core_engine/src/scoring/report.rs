//! Journal de score : ScoreStep et ScoringReport.

use crate::scoring::effect::{ScoreAction, StepSource};

/// Un pas du journal : l'effet appliqué et l'état qui en résulte.
///
/// Un pas par effet produit par la passe A, exactement, dans le même ordre :
/// ni fusion de deux effets d'une même source, ni pas synthétique de clôture.
/// Les trois champs `*_after` décrivent l'état du contexte **après**
/// application de `action`, `score_after` valant le score final à cet instant.
///
/// Le journal est rejouable : repartir d'un contexte neuf et appliquer dans
/// l'ordre les seuls `action` reproduit `ScoringReport::final_score`. Aucune
/// mutation du contexte ne peut donc avoir lieu hors d'une action enregistrée,
/// et un ajustement discret de la base rendrait le journal faux.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScoreStep {
    pub source: StepSource,
    pub action: ScoreAction,
    pub chips_after: u64,
    pub mult_after: i64,
    pub score_after: u64,
}

/// Sortie du pipeline : le score calculé et le journal qui le justifie.
///
/// `PartialEq` n'est pas décoratif : c'est lui qui permet de comparer deux
/// rapports **entiers**, journal compris, plutôt que deux nombres. Une
/// divergence d'ordre dans `steps` se voit alors, là où une comparaison de
/// scores la laisserait passer.
///
/// Le rapport ne porte aucun champ d'attribution : ni identité de blind, ni
/// marqueur d'application, ni horodatage. La résolution calcule et ne commet
/// rien (ADR-010) ; un tel champ inviterait au double comptage que cet ADR
/// supprime. `final_score` est la valeur autoritaire, redondante par
/// construction avec le dernier pas.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScoringReport {
    pub final_score: u64,
    pub chips: u64,
    pub mult: i64,
    pub steps: Vec<ScoreStep>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::DieId;
    use crate::hands::YahtzeeHand;
    use crate::relics::RelicId;
    use crate::scoring::{ScoreAction, ScoreContext, StepSource};

    /// Les neuf pas du § 2.4 : Full 5-5-5-2-2, FullHouse niveau 1, inventaire
    /// `[Paire Magique, Verre Brisé]` aux slots 0 et 1. Fixture partagée avec
    /// TASK-25 et TASK-26 : elle doit rester identique dans les trois.
    fn fixture() -> ScoringReport {
        let hand = StepSource::HandBase {
            hand: YahtzeeHand::FullHouse,
        };
        let step = |source, action, chips_after, mult_after, score_after| ScoreStep {
            source,
            action,
            chips_after,
            mult_after,
            score_after,
        };
        let die = |id, value| StepSource::Die {
            die_id: DieId(id),
            value,
        };

        ScoringReport {
            final_score: 588,
            chips: 49,
            mult: 1200,
            steps: vec![
                step(hand, ScoreAction::AddChips(30), 30, 0, 0),
                step(hand, ScoreAction::AddMult(400), 30, 400, 120),
                step(die(0, 5), ScoreAction::AddChips(5), 35, 400, 140),
                step(die(1, 5), ScoreAction::AddChips(5), 40, 400, 160),
                step(die(2, 5), ScoreAction::AddChips(5), 45, 400, 180),
                step(die(3, 2), ScoreAction::AddChips(2), 47, 400, 188),
                step(die(4, 2), ScoreAction::AddChips(2), 49, 400, 196),
                step(
                    StepSource::Relic {
                        uid: 0,
                        def: RelicId::MagicPair,
                    },
                    ScoreAction::AddMult(400),
                    49,
                    800,
                    392,
                ),
                step(
                    StepSource::Relic {
                        uid: 1,
                        def: RelicId::BrokenGlass,
                    },
                    ScoreAction::MultiplyMult(150),
                    49,
                    1200,
                    588,
                ),
            ],
        }
    }

    #[test]
    fn test_report_equality_on_clone() {
        let report = fixture();
        assert_eq!(report, report.clone());

        // Un seul `score_after` faux suffit à rompre l'égalité : c'est ce qui
        // rend comparables deux rapports entiers à TASK-26.
        let mut altere = report.clone();
        altere.steps[4].score_after += 1;
        assert_ne!(report, altere);

        // L'ordre est l'information (ADR-005) : permuter deux pas change le
        // rapport, même à contenu identique.
        let mut permute = report.clone();
        permute.steps.swap(3, 4);
        assert_ne!(report, permute);
    }

    #[test]
    fn test_steps_are_replayable() {
        let report = fixture();
        let mut ctx = ScoreContext::default();

        // Le journal n'est pas un commentaire du calcul, il est le calcul :
        // chaque pas est confronté à l'état réel, et non le seul état final.
        for (position, step) in report.steps.iter().enumerate() {
            match step.action {
                ScoreAction::AddChips(amount) => ctx.add_chips(amount),
                ScoreAction::AddMult(hundredths) => ctx.add_mult(hundredths),
                ScoreAction::MultiplyMult(factor_pct) => ctx.multiply_mult(factor_pct),
            }
            assert_eq!(
                (ctx.chips, ctx.mult, ctx.final_score()),
                (step.chips_after, step.mult_after, step.score_after),
                "pas n°{}",
                position + 1
            );
        }

        assert_eq!(ctx.chips, report.chips);
        assert_eq!(ctx.mult, report.mult);
        assert_eq!(ctx.final_score(), report.final_score);
        assert_eq!(report.final_score, 588);
    }

    #[test]
    fn test_first_step_score_after_is_zero() {
        // Les Chips de base sont versées avant le Mult de base : un Mult nul
        // rend un score nul, et le plancher de design ne s'applique pas encore.
        let premier = fixture().steps[0];
        assert_eq!(premier.chips_after, 30);
        assert_eq!(premier.mult_after, 0);
        assert_eq!(premier.score_after, 0);
    }
}
