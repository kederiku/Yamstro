//! Base de la figure : niveaux puis modificateurs de blind.

use crate::blinds::{BlindContext, BlindModifier};
use crate::hands::YahtzeeHand;

/// `HandLevels` reste chez les figures, c'est un concept de figure et non de
/// scoring. Ce ré-export n'est qu'un raccord : il fait résoudre le chemin
/// `core_engine::scoring::levels::HandLevels` sans dupliquer la structure.
pub use crate::hands::HandLevels;

/// Base de la figure une fois les niveaux appliqués, puis les modificateurs de
/// blind qui touchent la base. **Aucun autre endroit du pipeline n'y touche.**
///
/// L'ordre est strict : niveau d'abord, modificateur ensuite. L'inverse change
/// le résultat, et `test_level_applies_before_blind_modifier` existe pour ça.
/// La progression de niveau n'est pas recopiée ici : elle est **appelée**, et
/// son plafond est tenu par le type des figures.
///
/// Entre les deux modificateurs, la convention est `HalveBaseScores` puis
/// `AllMultToOne`, dans l'ordre de déclaration des variantes, ce qui fait
/// d'`AllMultToOne` une opération terminale et idempotente. Elle est écrite en
/// deux conditions successives et non par une itération sur la collection :
/// l'ordre d'une collection n'est pas un ordre d'application.
pub fn resolved_base(
    hand: YahtzeeHand,
    hand_levels: &HandLevels,
    blind: &BlindContext,
) -> (u64, i64) {
    let (mut chips, mut mult) = hand_levels.base_for(hand);

    if blind.has_modifier(BlindModifier::HalveBaseScores) {
        // Arrondi au plus proche, en entier, comme le reste du moteur. Une
        // division nue tronquerait et ferait tomber les As à 2 au lieu de 3.
        // L'écriture diffère entre les deux, et ce n'est pas un oubli :
        // `div_ceil` est stable sur les entiers non signés, où clippy l'exige,
        // et encore instable sur les signés au canal 1.95. Les Chips passent
        // donc par elle, le Mult garde la forme normative.
        chips = chips.div_ceil(2).max(1);
        mult = (mult + 1) / 2;
        mult = mult.max(100);
    }

    if blind.has_modifier(BlindModifier::AllMultToOne) {
        mult = 100;
    }

    (chips, mult)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blinds::{BlindContext, BlindModifier};
    use crate::hands::YahtzeeHand;

    fn sans_modificateur() -> BlindContext {
        BlindContext::de_test(None)
    }

    fn avec(modifiers: &[BlindModifier]) -> BlindContext {
        // Un blind ne porte qu'un modificateur ; les tests qui en passaient
        // deux vérifiaient un empilement que la définition ne permet plus.
        BlindContext::de_test(modifiers.first().cloned())
    }

    #[test]
    fn test_full_house_base_is_30x4() {
        let levels = HandLevels::default();
        assert_eq!(
            resolved_base(YahtzeeHand::FullHouse, &levels, &sans_modificateur()),
            (30, 400)
        );
    }

    #[test]
    fn test_hand_level_changes_base() {
        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::ThreeOfAKind);
        levels.upgrade(YahtzeeHand::ThreeOfAKind);
        assert_eq!(levels.level(YahtzeeHand::ThreeOfAKind), 3);
        assert_eq!(
            resolved_base(YahtzeeHand::ThreeOfAKind, &levels, &sans_modificateur()),
            (40, 400)
        );

        for _ in 0..20 {
            levels.upgrade(YahtzeeHand::ThreeOfAKind);
        }
        assert_eq!(
            resolved_base(YahtzeeHand::ThreeOfAKind, &levels, &sans_modificateur()),
            (145, 1100)
        );
    }

    #[test]
    fn test_all_mult_to_one_leaves_chips() {
        let levels = HandLevels::default();
        assert_eq!(
            resolved_base(
                YahtzeeHand::FullHouse,
                &levels,
                &avec(&[BlindModifier::AllMultToOne])
            ),
            (30, 100)
        );
    }

    #[test]
    fn test_halve_base_scores_rounds_to_nearest() {
        let levels = HandLevels::default();
        let blind = avec(&[BlindModifier::HalveBaseScores]);
        assert_eq!(
            resolved_base(YahtzeeHand::FullHouse, &levels, &blind),
            (15, 200)
        );
        // (5 + 1) / 2 = 3, et (100 + 1) / 2 = 50 relevé au plancher 100.
        assert_eq!(resolved_base(YahtzeeHand::Aces, &levels, &blind), (3, 100));
    }

    // ---- TASK-73 : *Le Silex*, depuis le catalogue ----

    /// Manche portant la contrainte du boss, telle que le catalogue la rend.
    /// **C'est l'angle neuf** : les tests ci-dessus nomment `HalveBaseScores`
    /// à la main et ne verraient pas *Le Silex* changer de contrainte dans
    /// `boss_definition`.
    fn manche_du_silex() -> BlindContext {
        use crate::blinds::definitions::{BossId, boss_definition};
        let mut rng = crate::rng::RunRng::from_seed(0);
        BlindContext::de_test(Some(
            boss_definition(BossId::Flint, &mut rng.boss, 5).modifier,
        ))
    }

    /// Full niveau 3. Deux montées depuis le niveau 1.
    fn full_niveau_3() -> HandLevels {
        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::FullHouse);
        levels.upgrade(YahtzeeHand::FullHouse);
        levels
    }

    #[test]
    fn test_silex_halves_after_level_with_floor() {
        // Full niveau 3 : (60, 600) avant le boss, (30, 300) après.
        assert_eq!(
            resolved_base(
                YahtzeeHand::FullHouse,
                &full_niveau_3(),
                &sans_modificateur()
            ),
            (60, 600),
            "la base de départ a changé"
        );
        assert_eq!(
            resolved_base(YahtzeeHand::FullHouse, &full_niveau_3(), &manche_du_silex()),
            (30, 300)
        );

        // As niveau 1 : (5, 100). L'arrondi au plus proche rend 3 et non 2, et
        // le mult tombé à 50 est relevé au plancher. Jamais 50.
        let (chips, mult) = resolved_base(
            YahtzeeHand::Aces,
            &HandLevels::default(),
            &manche_du_silex(),
        );
        assert_eq!((chips, mult), (3, 100));
        assert_ne!(mult, 50, "le plancher du glossaire § 4 n'a pas mordu");
        assert_ne!(chips, 2, "la troncature a remplacé l'arrondi");
    }

    #[test]
    fn test_silex_is_applied_after_hand_levels() {
        let obtenu = resolved_base(YahtzeeHand::FullHouse, &full_niveau_3(), &manche_du_silex());

        assert_eq!(obtenu, (30, 300));
        // L'ordre inverse — diviser d'abord, monter en niveau ensuite —
        // donnerait (15, 200) puis deux paliers, soit (45, 400). C'est
        // l'ambiguïté que la v1 laissait ouverte, et elle s'exclut par une
        // assertion, pas par un commentaire.
        assert_ne!(obtenu, (45, 400), "le niveau a été appliqué après le boss");
    }

    #[test]
    fn test_silex_touches_nothing_else() {
        use crate::evaluator::HandMatch;
        use crate::relics::RelicInventory;
        use crate::scoring::{ScoringPipeline, StepSource};

        let des = [3_u8, 3, 3, 5, 5]
            .iter()
            .enumerate()
            .map(|(rang, valeur)| {
                let mut die = crate::dice::Die::new(crate::dice::DieId(rang as u32), 6);
                die.current_value = *valeur;
                die
            })
            .collect::<Vec<_>>();
        let main = HandMatch {
            hand: YahtzeeHand::FullHouse,
            scoring_dice: des.iter().map(|die| die.id).collect(),
            discarded_dice: Vec::new(),
            potential_score: 0,
        };
        let stock = RelicInventory::new(5);
        let niveaux = full_niveau_3();

        let paliers = |manche: &BlindContext| {
            ScoringPipeline::resolve(&main, &des, &niveaux, &stock, manche).steps
        };
        let sans = paliers(&sans_modificateur());
        let avec_boss = paliers(&manche_du_silex());

        // **Comparer les actes, pas les cumuls.** Un `ScoreStep` porte aussi
        // `chips_after`, `mult_after` et `score_after`, qui s'accumulent depuis
        // la base : diviser celle-ci les décale forcément tous. Ce qui doit
        // rester identique est la **suite des actes** — mêmes sources, mêmes
        // actions, dans le même ordre —, ce qui dit que le boss divise la base
        // une fois et ne redivise rien en aval.
        let actes = |liste: &[crate::scoring::ScoreStep], base: bool| {
            liste
                .iter()
                .filter(|palier| matches!(palier.source, StepSource::HandBase { .. }) == base)
                .map(|palier| (palier.source, palier.action))
                .collect::<Vec<_>>()
        };

        // **La base est pinnée à sa valeur, pas seulement « différente ».**
        // Un `assert_ne!` seul laisse passer une **seconde** division en aval :
        // deux fois divisé diffère tout autant d'une fois divisé. C'est le
        // premier piège du ticket, et il survivait au banc sans ces deux
        // lignes. Full niveau 3 : (60, 600) devient (30, 300), une fois.
        use crate::scoring::ScoreAction;
        let source = StepSource::HandBase {
            hand: YahtzeeHand::FullHouse,
        };
        assert_eq!(
            actes(&sans, true),
            vec![
                (source, ScoreAction::AddChips(60)),
                (source, ScoreAction::AddMult(600))
            ]
        );
        assert_eq!(
            actes(&avec_boss, true),
            vec![
                (source, ScoreAction::AddChips(30)),
                (source, ScoreAction::AddMult(300))
            ],
            "la base a été divisée ailleurs qu'à l'étape 1"
        );
        assert_eq!(
            actes(&sans, false),
            actes(&avec_boss, false),
            "le boss a débordé de l'étape 1"
        );
        assert!(
            !actes(&sans, false).is_empty(),
            "aucun palier hors base à comparer"
        );

        // Et les cumuls, eux, bougent bel et bien : sans cette assertion le
        // test passerait sur un boss inerte dont la base ne change pas.
        assert_ne!(
            sans.last().map(|palier| palier.score_after),
            avec_boss.last().map(|palier| palier.score_after),
            "le score final est le même avec et sans Le Silex"
        );

        // **Ce que cette égalité tient, et ce qu'elle ne tient pas.** Elle tient
        // parce qu'aucune relique de l'Étape 5 ne lit la base : les seules
        // occurrences de `base_chips` dans `relics/` sont une fixture de test.
        // Le `TriggerCtx` la porte pourtant bel et bien, et une relique de
        // l'Étape 9 qui la lirait serait légitimement affectée par le boss.
        // Ce n'est donc pas un invariant général du pipeline.
    }

    #[test]
    fn test_single_modifier_per_blind() {
        // **L'empilement a été retiré.** Une définition de blind ne porte plus
        // qu'un modificateur, `Option<BlindModifier>`, ce qui rend sans objet
        // l'ordre d'application que ce test verrouillait auparavant. Reste
        // vérifiable : chacun des deux, seul, produit le résultat documenté.
        let levels = HandLevels::default();

        assert_eq!(
            resolved_base(
                YahtzeeHand::FullHouse,
                &levels,
                &avec(&[BlindModifier::HalveBaseScores])
            ),
            (15, 200)
        );
        assert_eq!(
            resolved_base(
                YahtzeeHand::FullHouse,
                &levels,
                &avec(&[BlindModifier::AllMultToOne])
            ),
            (30, 100)
        );
    }

    #[test]
    fn test_level_applies_before_blind_modifier() {
        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::FullHouse);
        levels.upgrade(YahtzeeHand::FullHouse);
        // Niveau 3 : (60, 600). Halvé : (30, 300). L'ordre inverse donnerait
        // (45, 400), ce que ce test exclut.
        assert_eq!(
            resolved_base(
                YahtzeeHand::FullHouse,
                &levels,
                &avec(&[BlindModifier::HalveBaseScores])
            ),
            (30, 300)
        );
    }
}
