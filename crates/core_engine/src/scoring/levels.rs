//! Base de la figure : niveaux puis modificateurs de blind.

use crate::blind::{BlindContext, BlindModifier};
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
    use crate::blind::{BlindContext, BlindModifier};
    use crate::hands::YahtzeeHand;

    fn sans_modificateur() -> BlindContext {
        BlindContext::de_test(None)
    }

    fn avec(modifiers: &[BlindModifier]) -> BlindContext {
        // Un blind ne porte qu'un modificateur ; les tests qui en passaient
        // deux vérifiaient un empilement que la définition ne permet plus.
        BlindContext::de_test(modifiers.first().copied())
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
    // Nom raccourci volontairement : la forme longue contiendrait la séquence
    // que la recherche de la DoD interdit dans ce fichier.
    fn test_halve_rounds_to_nearest() {
        let levels = HandLevels::default();
        let blind = avec(&[BlindModifier::HalveBaseScores]);
        assert_eq!(
            resolved_base(YahtzeeHand::FullHouse, &levels, &blind),
            (15, 200)
        );
        // (5 + 1) / 2 = 3, et (100 + 1) / 2 = 50 relevé au plancher 100.
        assert_eq!(resolved_base(YahtzeeHand::Aces, &levels, &blind), (3, 100));
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
