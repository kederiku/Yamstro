//! Évaluateur de main : HandEvaluator, HandMatch.

use crate::dice::{Die, DieId, MAX_DIE_SIDES};
use crate::hands::YahtzeeHand;

/// Taille du tableau de fréquences. L'index est la valeur de face, l'index 0
/// reste inutilisé : le comptage passe par un tableau et jamais par une table
/// de hachage, dont l'ordre d'itération détruirait le départage.
const FACE_SLOTS: usize = MAX_DIE_SIDES as usize + 1;

/// Une figure trouvée dans la main, et les dés qui y participent.
///
/// `scoring_dice` et `discarded_dice` partitionnent exactement la main : leur
/// union est la main, leur intersection est vide. C'est ce qui alimente
/// l'animation séquentielle de l'Étape 4.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HandMatch {
    pub hand: YahtzeeHand,
    pub scoring_dice: Vec<DieId>,
    pub discarded_dice: Vec<DieId>,
    pub potential_score: u64,
}

/// Évaluateur sans état.
#[derive(Debug, Clone, Copy)]
pub struct HandEvaluator;

impl HandEvaluator {
    /// Rend **toutes** les figures valides de la main, jamais la meilleure : la
    /// grille est consommable et le choix de la figure appartient au joueur.
    ///
    /// Ce ticket ne couvre que les figures de répétition et Chance. Les suites
    /// et les figures numériques viennent à TASK-11.
    pub fn evaluate(dice: &[Die]) -> Vec<HandMatch> {
        let counts = face_counts(dice);
        let mut matches = Vec::new();

        for (hand, threshold) in [
            (YahtzeeHand::ThreeOfAKind, 3_u8),
            (YahtzeeHand::FourOfAKind, 4),
            (YahtzeeHand::Yahtzee, 5),
        ] {
            if let Some(face) = highest_face_with(&counts, threshold, None) {
                let chosen = lowest_ids_of_face(dice, face, threshold);
                matches.push(assemble(hand, dice, &chosen));
            }
        }

        // Le triple prend la face la plus haute, la paire la plus haute des
        // faces restantes. Les dés surnuméraires du triple ne peuvent pas
        // fournir la paire : les deux faces d'un Full diffèrent.
        if let Some(triple) = highest_face_with(&counts, 3, None)
            && let Some(pair) = highest_face_with(&counts, 2, Some(triple))
        {
            let mut chosen = lowest_ids_of_face(dice, triple, 3);
            chosen.extend(lowest_ids_of_face(dice, pair, 2));
            matches.push(assemble(YahtzeeHand::FullHouse, dice, &chosen));
        }

        matches.push(HandMatch {
            hand: YahtzeeHand::Chance,
            scoring_dice: dice.iter().map(|die| die.id).collect(),
            discarded_dice: Vec::new(),
            // TASK-12: potential_score
            potential_score: 0,
        });

        matches
    }
}

/// Fréquence de chaque face. Une valeur nulle ou au-delà de `MAX_DIE_SIDES` est
/// ignorée, sans panic ni indexation hors bornes.
fn face_counts(dice: &[Die]) -> [u8; FACE_SLOTS] {
    let mut counts = [0_u8; FACE_SLOTS];

    for die in dice {
        if die.current_value == 0 {
            continue;
        }
        if let Some(slot) = counts.get_mut(usize::from(die.current_value)) {
            *slot = slot.saturating_add(1);
        }
    }

    counts
}

/// La face la plus haute dont le compte atteint `threshold`, une face déjà
/// retenue pouvant être exclue.
fn highest_face_with(counts: &[u8; FACE_SLOTS], threshold: u8, excluded: Option<u8>) -> Option<u8> {
    (1..=MAX_DIE_SIDES).rev().find(|face| {
        Some(*face) != excluded
            && counts
                .get(usize::from(*face))
                .is_some_and(|count| *count >= threshold)
    })
}

/// Les `count` dés de cette face portant les plus petits identifiants.
fn lowest_ids_of_face(dice: &[Die], face: u8, count: u8) -> Vec<DieId> {
    let mut ids: Vec<DieId> = dice
        .iter()
        .filter(|die| die.current_value == face)
        .map(|die| die.id)
        .collect();

    ids.sort_unstable();
    ids.truncate(usize::from(count));
    ids
}

/// Assemble un `HandMatch` en rangeant les deux listes dans l'ordre de la main,
/// et non dans celui des identifiants : c'est l'ordre d'affichage que l'Étape 4
/// balaiera.
fn assemble(hand: YahtzeeHand, dice: &[Die], chosen: &[DieId]) -> HandMatch {
    let mut scoring_dice = Vec::new();
    let mut discarded_dice = Vec::new();

    for die in dice {
        if chosen.contains(&die.id) {
            scoring_dice.push(die.id);
        } else {
            discarded_dice.push(die.id);
        }
    }

    HandMatch {
        hand,
        scoring_dice,
        discarded_dice,
        // TASK-12: potential_score
        potential_score: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::MAX_DIE_SIDES;

    /// Une main dont les identifiants suivent l'ordre d'affichage.
    fn hand(values: &[u8]) -> Vec<Die> {
        (0_u32..)
            .zip(values)
            .map(|(id, value)| {
                let mut die = Die::new(DieId(id), MAX_DIE_SIDES);
                die.current_value = *value;
                die
            })
            .collect()
    }

    /// Une main dont les identifiants sont choisis, pour dissocier l'ordre
    /// d'affichage de l'ordre des identifiants.
    fn hand_with_ids(pairs: &[(u32, u8)]) -> Vec<Die> {
        pairs
            .iter()
            .map(|(id, value)| {
                let mut die = Die::new(DieId(*id), MAX_DIE_SIDES);
                die.current_value = *value;
                die
            })
            .collect()
    }

    fn find(matches: &[HandMatch], wanted: YahtzeeHand) -> Option<&HandMatch> {
        matches.iter().find(|found| found.hand == wanted)
    }

    fn faces_of(dice: &[Die], ids: &[DieId]) -> Vec<u8> {
        ids.iter()
            .filter_map(|id| dice.iter().find(|die| die.id == *id))
            .map(|die| die.current_value)
            .collect()
    }

    #[test]
    fn test_yahtzee_detected() {
        let dice = hand(&[4, 4, 4, 4, 4]);
        let matches = HandEvaluator::evaluate(&dice);

        let found = find(&matches, YahtzeeHand::Yahtzee).expect("Yams attendu");
        assert_eq!(found.scoring_dice.len(), 5);
        assert!(found.discarded_dice.is_empty());
    }

    #[test]
    fn test_full_house_valid_and_rejected() {
        let valid = HandEvaluator::evaluate(&hand(&[2, 2, 2, 5, 5]));
        assert!(find(&valid, YahtzeeHand::FullHouse).is_some());

        let rejected = HandEvaluator::evaluate(&hand(&[2, 2, 3, 5, 5]));
        assert!(find(&rejected, YahtzeeHand::FullHouse).is_none());
    }

    #[test]
    fn test_yahtzee_also_yields_three_and_four_of_a_kind() {
        let matches = HandEvaluator::evaluate(&hand(&[4, 4, 4, 4, 4]));

        let three = find(&matches, YahtzeeHand::ThreeOfAKind).expect("Brelan attendu");
        assert_eq!(three.scoring_dice.len(), 3);

        let four = find(&matches, YahtzeeHand::FourOfAKind).expect("Carré attendu");
        assert_eq!(four.scoring_dice.len(), 4);

        // Cinq dés identiques ne font pas un Full : les deux faces diffèrent.
        assert!(find(&matches, YahtzeeHand::FullHouse).is_none());
    }

    #[test]
    fn test_chance_is_always_present() {
        let matches = HandEvaluator::evaluate(&hand(&[1, 3, 5, 2, 6]));

        let chance = find(&matches, YahtzeeHand::Chance).expect("Chance attendue");
        assert_eq!(chance.scoring_dice.len(), 5);
        assert!(chance.discarded_dice.is_empty());
    }

    #[test]
    fn test_scoring_and_discarded_partition_the_hand() {
        let dice = hand(&[2, 2, 2, 5, 5]);
        let all: Vec<DieId> = dice.iter().map(|die| die.id).collect();

        for found in HandEvaluator::evaluate(&dice) {
            assert_eq!(
                found.scoring_dice.len() + found.discarded_dice.len(),
                dice.len(),
                "{:?}",
                found.hand
            );

            let mut union = found.scoring_dice.clone();
            union.extend(found.discarded_dice.iter().copied());
            union.sort_unstable();
            assert_eq!(union, all, "{:?}", found.hand);

            for id in &found.scoring_dice {
                assert!(!found.discarded_dice.contains(id), "{:?}", found.hand);
            }
        }
    }

    #[test]
    fn test_four_dice_never_yield_yahtzee() {
        let matches = HandEvaluator::evaluate(&hand(&[3, 3, 3, 3]));

        assert!(find(&matches, YahtzeeHand::FourOfAKind).is_some());
        assert!(find(&matches, YahtzeeHand::Yahtzee).is_none());
    }

    #[test]
    fn test_faces_above_six_form_sets() {
        let dice = hand(&[7, 7, 7, 2, 3]);
        let matches = HandEvaluator::evaluate(&dice);

        let three = find(&matches, YahtzeeHand::ThreeOfAKind).expect("Brelan attendu");
        assert_eq!(three.scoring_dice.len(), 3);
        assert_eq!(faces_of(&dice, &three.scoring_dice), [7, 7, 7]);
    }

    #[test]
    fn test_highest_face_wins_the_tie() {
        let dice = hand(&[2, 2, 2, 5, 5, 5]);
        let matches = HandEvaluator::evaluate(&dice);

        let three = find(&matches, YahtzeeHand::ThreeOfAKind).expect("Brelan attendu");
        assert_eq!(faces_of(&dice, &three.scoring_dice), [5, 5, 5]);
    }

    #[test]
    fn test_potential_score_is_zero_until_task_12() {
        for found in HandEvaluator::evaluate(&hand(&[2, 2, 2, 5, 5])) {
            assert_eq!(found.potential_score, 0, "{:?}", found.hand);
        }
    }

    #[test]
    fn test_full_house_pair_takes_highest_remaining_face() {
        // Deux paires possibles hors du triple : la plus haute l'emporte.
        let dice = hand(&[6, 6, 6, 3, 3, 2, 2]);
        let matches = HandEvaluator::evaluate(&dice);
        let full = find(&matches, YahtzeeHand::FullHouse).expect("Full attendu");
        assert_eq!(faces_of(&dice, &full.scoring_dice), [6, 6, 6, 3, 3]);

        // Les deux 5 surnuméraires ne peuvent pas fournir la paire : les deux
        // faces d'un Full diffèrent.
        let dice = hand(&[5, 5, 5, 5, 5, 2, 2]);
        let matches = HandEvaluator::evaluate(&dice);
        let full = find(&matches, YahtzeeHand::FullHouse).expect("Full attendu");
        assert_eq!(faces_of(&dice, &full.scoring_dice), [5, 5, 5, 2, 2]);
    }

    #[test]
    fn test_lists_follow_hand_order() {
        // Identifiants décroissants : l'ordre d'affichage et l'ordre des
        // identifiants divergent. Le Brelan retient les trois plus petits
        // identifiants de la face 3, soit 10, 20 et 30, mais les range dans
        // l'ordre de la main.
        let dice = hand_with_ids(&[(40, 3), (30, 3), (20, 3), (10, 3), (0, 1)]);
        let matches = HandEvaluator::evaluate(&dice);

        let three = find(&matches, YahtzeeHand::ThreeOfAKind).expect("Brelan attendu");
        assert_eq!(
            three.scoring_dice,
            [DieId(30), DieId(20), DieId(10)],
            "l'ordre de la main, pas celui des identifiants"
        );
        assert_eq!(three.discarded_dice, [DieId(40), DieId(0)]);
    }

    #[test]
    fn test_out_of_range_face_is_kept_but_not_counted() {
        let dice = hand(&[0, 7, 7, 7, 2]);
        let matches = HandEvaluator::evaluate(&dice);

        // Chance prend tous les dés, celui hors bornes compris.
        let chance = find(&matches, YahtzeeHand::Chance).expect("Chance attendue");
        assert_eq!(chance.scoring_dice.len(), 5);
        assert!(chance.discarded_dice.is_empty());

        // Le dé hors bornes ne compte pour aucune figure de forme.
        let three = find(&matches, YahtzeeHand::ThreeOfAKind).expect("Brelan attendu");
        assert_eq!(faces_of(&dice, &three.scoring_dice), [7, 7, 7]);
        assert_eq!(three.discarded_dice, [DieId(0), DieId(4)]);
    }

    #[test]
    fn test_hand_match_roundtrip_serde() {
        let matches = HandEvaluator::evaluate(&hand(&[2, 2, 2, 5, 5]));

        let encoded = serde_json::to_string(&matches).expect("sérialisation");
        let decoded: Vec<HandMatch> = serde_json::from_str(&encoded).expect("désérialisation");

        assert_eq!(decoded, matches);
    }
}
