//! Évaluateur de main : HandEvaluator, HandMatch.

use core::cmp::Ordering;

use crate::dice::{Die, DieId, MAX_DIE_SIDES};
use crate::hands::{HandLevels, YahtzeeHand};
use crate::scoring::ScoreContext;

/// Taille du tableau de fréquences. L'index est la valeur de face, l'index 0
/// reste inutilisé : le comptage passe par un tableau et jamais par une table
/// de hachage, dont l'ordre d'itération détruirait le départage.
const FACE_SLOTS: usize = MAX_DIE_SIDES as usize + 1;

/// Longueur d'une Petite Suite, en valeurs consécutives. Seuil **absolu** : il
/// ne se dérive jamais du nombre de dés en main ni de la configuration.
const SMALL_STRAIGHT_LEN: u8 = 4;

/// Longueur d'une Grande Suite. Même remarque : une main de quatre dés n'a pas
/// de Grande Suite, et ce n'est pas une erreur.
const LARGE_STRAIGHT_LEN: u8 = 5;

/// Les six figures numériques et la face qu'elles retiennent. Elles restent
/// définies sur 1 à 6 : une face au-delà n'y participe jamais, alors qu'elle
/// participe pleinement aux figures de forme et aux suites.
const NUMERIC_HANDS: [(YahtzeeHand, u8); 6] = [
    (YahtzeeHand::Aces, 1),
    (YahtzeeHand::Twos, 2),
    (YahtzeeHand::Threes, 3),
    (YahtzeeHand::Fours, 4),
    (YahtzeeHand::Fives, 5),
    (YahtzeeHand::Sixes, 6),
];

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

        for (hand, length) in [
            (YahtzeeHand::SmallStraight, SMALL_STRAIGHT_LEN),
            (YahtzeeHand::LargeStraight, LARGE_STRAIGHT_LEN),
        ] {
            if let Some(window) = highest_straight(&counts, length) {
                let chosen: Vec<DieId> = window
                    .iter()
                    .filter_map(|face| lowest_ids_of_face(dice, *face, 1).first().copied())
                    .collect();
                matches.push(assemble(hand, dice, &chosen));
            }
        }

        // Une figure numérique retient tous les dés de sa face, là où une
        // figure de forme n'en retient que le seuil.
        for (hand, face) in NUMERIC_HANDS {
            let chosen = all_ids_of_face(dice, face);
            if !chosen.is_empty() {
                matches.push(assemble(hand, dice, &chosen));
            }
        }

        let all: Vec<DieId> = dice.iter().map(|die| die.id).collect();
        matches.push(assemble(YahtzeeHand::Chance, dice, &all));

        // Le tri ne sélectionne rien : la grille est consommable et le choix de
        // la figure appartient au joueur. Il fige l'ordre d'affichage, et rien
        // d'autre. `sort_by` est stable, et la clé de départage est totale.
        matches.sort_by(compare_matches);

        matches
    }
}

/// Recalcule les aperçus selon les niveaux de figures, puis retrie.
///
/// Ne relance aucune détection : les figures émises et les dés retenus restent
/// ceux qu'`evaluate` a décidés, dans le même ordre. Seuls le chiffrage et
/// l'ordre d'affichage changent.
///
/// Sans cette fonction, la surbrillance de la meilleure figure encore
/// disponible classe faux dès qu'un parchemin de grille a monté une figure :
/// `evaluate` chiffre tout au niveau 1.
///
/// Le score est recalculé **depuis la base**, jamais accumulé sur la valeur
/// courante : appeler la fonction deux fois de suite ne doit rien changer.
pub fn rescore_with_levels(matches: &mut [HandMatch], dice: &[Die], levels: &HandLevels) {
    for found in matches.iter_mut() {
        found.potential_score =
            score_from_base(levels.base_for(found.hand), dice, &found.scoring_dice);
    }

    matches.sort_by(compare_matches);
}

/// Ordre d'affichage : aperçu décroissant, départagé par l'ordre de
/// déclaration de `YahtzeeHand`.
///
/// Partagé par `evaluate` et `rescore_with_levels` : deux implémentations
/// finiraient par diverger. Le tri appelant est stable, et cette clé est
/// totale, donc le résultat est identique à chaque exécution.
///
/// Ce comparateur ne sélectionne rien : la grille est consommable et la figure
/// est choisie par le joueur.
fn compare_matches(left: &HandMatch, right: &HandMatch) -> Ordering {
    right
        .potential_score
        .cmp(&left.potential_score)
        .then((left.hand as usize).cmp(&(right.hand as usize)))
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

/// La fenêtre de `length` valeurs consécutives la plus haute, s'il en existe
/// une. Les valeurs distinctes se lisent dans le tableau de fréquences par
/// ordre croissant : aucun conteneur de hachage n'intervient, et le balayage
/// ascendant fait que la dernière fenêtre trouvée est la plus haute.
fn highest_straight(counts: &[u8; FACE_SLOTS], length: u8) -> Option<Vec<u8>> {
    let mut best = None;
    let mut run: Vec<u8> = Vec::new();

    for face in 1..=MAX_DIE_SIDES {
        if counts
            .get(usize::from(face))
            .is_some_and(|count| *count > 0)
        {
            run.push(face);
        } else {
            run.clear();
        }

        if run.len() >= usize::from(length) {
            best = Some(run[run.len().saturating_sub(usize::from(length))..].to_vec());
        }
    }

    best
}

/// Tous les dés affichant cette face, dans l'ordre de la main.
fn all_ids_of_face(dice: &[Die], face: u8) -> Vec<DieId> {
    dice.iter()
        .filter(|die| die.current_value == face)
        .map(|die| die.id)
        .collect()
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

    let potential_score = score_from_base(hand.base_score(), dice, &scoring_dice);

    HandMatch {
        hand,
        scoring_dice,
        discarded_dice,
        potential_score,
    }
}

/// Aperçu de score d'une figure, à partir de la base qu'on lui donne.
///
/// `evaluate` passe `hand.base_score()`, donc le niveau 1 ;
/// `rescore_with_levels` passe `levels.base_for(hand)`. Une seule formule, un
/// seul endroit où elle peut se tromper.
///
/// Ce n'est pas un score. Sceaux, modificateurs de dé et reliques n'entrent pas
/// dans ce calcul : ils relèvent du pipeline de l'Étape 2, seul juge du score
/// réel. Les ajouter ici dupliquerait le pipeline et ferait diverger les deux
/// nombres, celui qu'on montre au joueur et celui qu'il encaisse.
///
/// L'aperçu sert au tri et à la surbrillance, jamais à choisir une figure.
fn score_from_base(base: (u64, i64), dice: &[Die], scoring_dice: &[DieId]) -> u64 {
    let (base_chips, base_mult) = base;

    let chips = scoring_dice.iter().fold(base_chips, |total, id| {
        // Les dés se retrouvent par identifiant, jamais par position : la main
        // perd et gagne des dés en cours de manche. Un identifiant absent vaut
        // zéro Chip plutôt qu'un panic.
        let face = dice
            .iter()
            .find(|die| die.id == *id)
            .map_or(0, |die| u64::from(die.current_value));

        total.saturating_add(face)
    });

    ScoreContext {
        chips,
        mult: base_mult,
    }
    .final_score()
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

    #[test]
    fn test_small_straight_unordered_with_duplicate() {
        let dice = hand(&[3, 1, 4, 2, 4]);
        let matches = HandEvaluator::evaluate(&dice);

        let small = find(&matches, YahtzeeHand::SmallStraight).expect("Petite Suite attendue");
        assert_eq!(small.scoring_dice.len(), 4);
        // Le second dé de valeur 4 porte le plus grand identifiant : c'est lui
        // qui est écarté, le doublon ne servant pas à la détection.
        assert_eq!(small.discarded_dice, [DieId(4)]);
    }

    #[test]
    fn test_large_straight_with_d8() {
        let mut dice = hand(&[4, 5, 6, 7, 8]);
        if let Some(last) = dice.last_mut() {
            last.sides = 8;
        }
        let matches = HandEvaluator::evaluate(&dice);

        let large = find(&matches, YahtzeeHand::LargeStraight).expect("Grande Suite attendue");
        assert_eq!(large.scoring_dice.len(), 5);
        assert!(large.discarded_dice.is_empty());
    }

    #[test]
    fn test_faces_above_six_excluded_from_numeric_hands() {
        let dice = hand(&[7, 7, 7, 2, 3]);
        let matches = HandEvaluator::evaluate(&dice);

        let three = find(&matches, YahtzeeHand::ThreeOfAKind).expect("Brelan attendu");
        assert_eq!(faces_of(&dice, &three.scoring_dice), [7, 7, 7]);

        let numeric = [
            YahtzeeHand::Aces,
            YahtzeeHand::Twos,
            YahtzeeHand::Threes,
            YahtzeeHand::Fours,
            YahtzeeHand::Fives,
            YahtzeeHand::Sixes,
        ];
        for wanted in numeric {
            if let Some(found) = find(&matches, wanted) {
                assert!(
                    faces_of(&dice, &found.scoring_dice).iter().all(|f| *f <= 6),
                    "{wanted:?} retient une face au-delà de six"
                );
            }
        }

        assert_eq!(
            faces_of(
                &dice,
                &find(&matches, YahtzeeHand::Twos)
                    .expect("Deux attendus")
                    .scoring_dice
            ),
            [2]
        );
        assert_eq!(
            faces_of(
                &dice,
                &find(&matches, YahtzeeHand::Threes)
                    .expect("Trois attendus")
                    .scoring_dice
            ),
            [3]
        );
    }

    #[test]
    fn test_small_and_large_straight_both_emitted() {
        // Les deux cases de la grille sont distinctes et se consomment
        // séparément : détecter la Grande n'écarte pas la Petite.
        let matches = HandEvaluator::evaluate(&hand(&[1, 2, 3, 4, 5]));

        assert!(find(&matches, YahtzeeHand::SmallStraight).is_some());
        assert!(find(&matches, YahtzeeHand::LargeStraight).is_some());
    }

    #[test]
    fn test_large_straight_two_to_six() {
        let matches = HandEvaluator::evaluate(&hand(&[2, 3, 4, 5, 6]));

        let large = find(&matches, YahtzeeHand::LargeStraight).expect("Grande Suite attendue");
        assert_eq!(large.scoring_dice.len(), 5);
    }

    #[test]
    fn test_gap_is_not_a_straight() {
        let matches = HandEvaluator::evaluate(&hand(&[1, 2, 4, 5, 6]));

        assert!(find(&matches, YahtzeeHand::SmallStraight).is_none());
        assert!(find(&matches, YahtzeeHand::LargeStraight).is_none());
    }

    #[test]
    fn test_four_dice_never_yield_large_straight() {
        // Les seuils sont absolus : une main de quatre dés n'a pas de Grande
        // Suite, et ce n'est pas une erreur.
        let matches = HandEvaluator::evaluate(&hand(&[3, 4, 5, 6]));

        assert!(find(&matches, YahtzeeHand::SmallStraight).is_some());
        assert!(find(&matches, YahtzeeHand::LargeStraight).is_none());
    }

    #[test]
    fn test_numeric_hand_keeps_all_matching_dice() {
        let dice = hand(&[3, 3, 3, 4, 5]);
        let matches = HandEvaluator::evaluate(&dice);

        let threes = find(&matches, YahtzeeHand::Threes).expect("Trois attendus");
        assert_eq!(threes.scoring_dice, [DieId(0), DieId(1), DieId(2)]);
        assert_eq!(threes.discarded_dice, [DieId(3), DieId(4)]);
    }

    #[test]
    fn test_numeric_hand_absent_when_no_die_matches() {
        let matches = HandEvaluator::evaluate(&hand(&[2, 3, 4, 5, 6]));

        assert!(find(&matches, YahtzeeHand::Aces).is_none());
    }

    #[test]
    fn test_highest_straight_wins_the_tie() {
        // Trois Petites Suites conviennent et deux Grandes : la fenêtre dont la
        // valeur haute est la plus grande l'emporte.
        let dice = hand(&[1, 2, 3, 4, 5, 6]);
        let matches = HandEvaluator::evaluate(&dice);

        let small = find(&matches, YahtzeeHand::SmallStraight).expect("Petite Suite attendue");
        assert_eq!(faces_of(&dice, &small.scoring_dice), [3, 4, 5, 6]);

        let large = find(&matches, YahtzeeHand::LargeStraight).expect("Grande Suite attendue");
        assert_eq!(faces_of(&dice, &large.scoring_dice), [2, 3, 4, 5, 6]);
    }

    /// Les couples figure et aperçu, dans l'ordre du `Vec`.
    fn scored(matches: &[HandMatch]) -> Vec<(YahtzeeHand, u64)> {
        matches
            .iter()
            .map(|found| (found.hand, found.potential_score))
            .collect()
    }

    #[test]
    fn test_full_house_potential_score_is_184() {
        // Base (30, 400), somme des faces 16, donc 46 Chips à ×4,00.
        let matches = HandEvaluator::evaluate(&hand(&[2, 2, 2, 5, 5]));

        let full = find(&matches, YahtzeeHand::FullHouse).expect("Full attendu");
        assert_eq!(full.potential_score, 184);
    }

    #[test]
    fn test_sort_is_reproducible() {
        let dice = hand(&[2, 2, 2, 5, 5, 5]);
        let reference = scored(&HandEvaluator::evaluate(&dice));

        for _ in 0..100 {
            assert_eq!(scored(&HandEvaluator::evaluate(&dice)), reference);
        }
    }

    #[test]
    fn test_tie_break_keeps_highest_face() {
        let dice = hand(&[2, 2, 2, 5, 5, 5]);
        let matches = HandEvaluator::evaluate(&dice);

        let three = find(&matches, YahtzeeHand::ThreeOfAKind).expect("Brelan attendu");
        assert_eq!(faces_of(&dice, &three.scoring_dice), [5, 5, 5]);
        assert_eq!(three.discarded_dice, [DieId(0), DieId(1), DieId(2)]);
        assert_eq!(three.potential_score, 50);

        let full = find(&matches, YahtzeeHand::FullHouse).expect("Full attendu");
        assert_eq!(full.potential_score, 196);
    }

    #[test]
    fn test_full_house_restitutes_in_hand_order() {
        // La sélection prend le triple de 5 et la paire de 2, mais la
        // restitution suit l'ordre de la main : les deux 2 sortent en premier.
        let dice = hand(&[2, 2, 2, 5, 5, 5]);
        let matches = HandEvaluator::evaluate(&dice);

        let full = find(&matches, YahtzeeHand::FullHouse).expect("Full attendu");
        assert_eq!(faces_of(&dice, &full.scoring_dice), [2, 2, 5, 5, 5]);
    }

    #[test]
    fn test_equal_scores_follow_declaration_order() {
        let matches = HandEvaluator::evaluate(&hand(&[3, 3, 3, 4, 5]));
        let pairs = scored(&matches);

        let threes = pairs
            .iter()
            .position(|(hand, _)| *hand == YahtzeeHand::Threes)
            .expect("Trois attendus");
        let fours = pairs
            .iter()
            .position(|(hand, _)| *hand == YahtzeeHand::Fours)
            .expect("Quatre attendus");

        assert_eq!(pairs[threes].1, 48);
        assert_eq!(pairs[fours].1, 48);
        assert!(threes < fours, "à égalité, l'ordre de déclaration tranche");
    }

    #[test]
    fn test_reference_scores_for_a_three_of_a_kind_hand() {
        // Verrouille d'un coup le calcul, le tri décroissant, le départage par
        // ordre de déclaration et le fait qu'aucune autre figure ne soit émise.
        let matches = HandEvaluator::evaluate(&hand(&[3, 3, 3, 4, 5]));

        assert_eq!(
            scored(&matches),
            [
                (YahtzeeHand::Fives, 90),
                (YahtzeeHand::Threes, 48),
                (YahtzeeHand::Fours, 48),
                (YahtzeeHand::ThreeOfAKind, 38),
                (YahtzeeHand::Chance, 23),
            ]
        );
    }

    #[test]
    fn test_result_is_sorted_descending() {
        let matches = HandEvaluator::evaluate(&hand(&[2, 2, 2, 5, 5, 5]));

        for pair in matches.windows(2) {
            assert!(
                pair[0].potential_score >= pair[1].potential_score,
                "{:?} devrait précéder {:?}",
                pair[0].hand,
                pair[1].hand
            );
        }
    }

    #[test]
    fn test_scoring_dice_partition_is_preserved() {
        let dice = hand(&[3, 3, 3, 4, 5]);

        for found in HandEvaluator::evaluate(&dice) {
            assert_eq!(
                found.scoring_dice.len() + found.discarded_dice.len(),
                dice.len(),
                "{:?}",
                found.hand
            );
            for id in &found.scoring_dice {
                assert!(!found.discarded_dice.contains(id), "{:?}", found.hand);
            }
        }
    }

    /// La plus petite main produisant à la fois un Full et une Grande Suite.
    fn reference_hand() -> Vec<Die> {
        hand(&[2, 2, 2, 3, 3, 4, 5, 6])
    }

    fn score_of(matches: &[HandMatch], wanted: YahtzeeHand) -> u64 {
        find(matches, wanted)
            .map(|found| found.potential_score)
            .unwrap_or_else(|| panic!("{wanted:?} attendue"))
    }

    fn rank_of(matches: &[HandMatch], wanted: YahtzeeHand) -> usize {
        matches
            .iter()
            .position(|found| found.hand == wanted)
            .unwrap_or_else(|| panic!("{wanted:?} attendue"))
    }

    #[test]
    fn test_rescore_at_level_one_is_idempotent() {
        let dice = reference_hand();
        let mut matches = HandEvaluator::evaluate(&dice);
        let before = matches.clone();

        rescore_with_levels(&mut matches, &dice, &HandLevels::default());

        assert_eq!(matches, before);
    }

    #[test]
    fn test_rescore_lifts_full_house_above_large_straight() {
        let dice = reference_hand();
        let mut matches = HandEvaluator::evaluate(&dice);

        assert_eq!(score_of(&matches, YahtzeeHand::LargeStraight), 240);
        assert_eq!(score_of(&matches, YahtzeeHand::FullHouse), 168);
        assert!(
            rank_of(&matches, YahtzeeHand::LargeStraight)
                < rank_of(&matches, YahtzeeHand::FullHouse)
        );

        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::FullHouse);
        levels.upgrade(YahtzeeHand::FullHouse);
        assert_eq!(levels.level(YahtzeeHand::FullHouse), 3);

        rescore_with_levels(&mut matches, &dice, &levels);

        // Base (60, 600) au niveau 3, plus 12 de faces, soit 72 Chips à ×6,00.
        assert_eq!(score_of(&matches, YahtzeeHand::FullHouse), 432);
        // Une figure non montée ne bouge pas.
        assert_eq!(score_of(&matches, YahtzeeHand::LargeStraight), 240);
        assert!(
            rank_of(&matches, YahtzeeHand::FullHouse)
                < rank_of(&matches, YahtzeeHand::LargeStraight)
        );
    }

    #[test]
    fn test_rescore_does_not_touch_dice_partition() {
        let dice = reference_hand();
        let mut matches = HandEvaluator::evaluate(&dice);
        let before: Vec<(YahtzeeHand, Vec<DieId>, Vec<DieId>)> = matches
            .iter()
            .map(|found| {
                (
                    found.hand,
                    found.scoring_dice.clone(),
                    found.discarded_dice.clone(),
                )
            })
            .collect();

        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::FullHouse);
        levels.upgrade(YahtzeeHand::Sixes);
        rescore_with_levels(&mut matches, &dice, &levels);

        assert_eq!(matches.len(), before.len());
        for (wanted, scoring, discarded) in before {
            let found = find(&matches, wanted).expect("figure conservée");
            assert_eq!(found.scoring_dice, scoring, "{wanted:?}");
            assert_eq!(found.discarded_dice, discarded, "{wanted:?}");
        }
    }

    #[test]
    fn test_rescore_reorders_stably() {
        // Trois montés au niveau 2 valent 108, comme Six resté au niveau 1.
        // Avant la montée, Six précédait Trois ; après, l'ordre de déclaration
        // doit l'emporter sur l'ordre entrant.
        let dice = reference_hand();
        let mut matches = HandEvaluator::evaluate(&dice);
        assert!(rank_of(&matches, YahtzeeHand::Sixes) < rank_of(&matches, YahtzeeHand::Threes));

        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::Threes);

        rescore_with_levels(&mut matches, &dice, &levels);

        assert_eq!(score_of(&matches, YahtzeeHand::Threes), 108);
        assert_eq!(score_of(&matches, YahtzeeHand::Sixes), 108);
        assert!(rank_of(&matches, YahtzeeHand::Threes) < rank_of(&matches, YahtzeeHand::Sixes));
    }

    #[test]
    fn test_rescore_is_idempotent_on_second_call() {
        // Le recalcul part de la base, jamais du score courant : une
        // accumulation incrémentale doublerait le bonus au second appel.
        let dice = reference_hand();
        let mut matches = HandEvaluator::evaluate(&dice);
        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::FullHouse);
        levels.upgrade(YahtzeeHand::FullHouse);

        rescore_with_levels(&mut matches, &dice, &levels);
        let after_first = matches.clone();

        rescore_with_levels(&mut matches, &dice, &levels);

        assert_eq!(matches, after_first);
    }
}
