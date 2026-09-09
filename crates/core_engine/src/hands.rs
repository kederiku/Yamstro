//! Figures du Yams : YahtzeeHand, HandLevels.

/// Les treize figures du Yams.
///
/// **L'ordre de déclaration est normatif.** Il sert de clé de départage au tri
/// de l'évaluateur, et `hand as usize` donne la position `0..=12` qui indexera
/// le tableau de niveaux de `HandLevels`.
#[repr(u8)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum YahtzeeHand {
    Aces,
    Twos,
    Threes,
    Fours,
    Fives,
    Sixes,
    ThreeOfAKind,
    FourOfAKind,
    FullHouse,
    SmallStraight,
    LargeStraight,
    Yahtzee,
    Chance,
}

impl YahtzeeHand {
    /// Les treize figures, dans l'ordre de déclaration. Seul itérateur autorisé
    /// sur les figures : aucun code du corpus ne reconstruit cette liste.
    pub const ALL: [YahtzeeHand; 13] = [
        Self::Aces,
        Self::Twos,
        Self::Threes,
        Self::Fours,
        Self::Fives,
        Self::Sixes,
        Self::ThreeOfAKind,
        Self::FourOfAKind,
        Self::FullHouse,
        Self::SmallStraight,
        Self::LargeStraight,
        Self::Yahtzee,
        Self::Chance,
    ];

    /// Base normative de la figure au niveau 1 : `(Chips, Mult en centièmes)`.
    /// 400 vaut ×4,00. Table du § 4.1 du glossaire.
    pub const fn base_score(&self) -> (u64, i64) {
        match self {
            Self::Aces => (5, 100),
            Self::Twos => (10, 100),
            Self::Threes => (15, 200),
            Self::Fours => (20, 200),
            Self::Fives => (25, 300),
            Self::Sixes => (30, 300),
            Self::ThreeOfAKind => (10, 200),
            Self::FourOfAKind => (20, 300),
            Self::FullHouse => (30, 400),
            Self::SmallStraight => (25, 300),
            Self::LargeStraight => (40, 400),
            Self::Yahtzee => (50, 600),
            Self::Chance => (5, 100),
        }
    }
}

/// Niveau maximal atteignable par une figure.
pub const MAX_HAND_LEVEL: u8 = 10;

/// Chips conférés par chaque niveau supplémentaire, quelle que soit la figure.
const LEVEL_CHIPS_STEP: u64 = 15;

/// Mult conféré par chaque niveau supplémentaire, en centièmes.
const LEVEL_MULT_STEP: i64 = 100;

/// Niveau courant de chacune des treize figures, dans `1..=MAX_HAND_LEVEL`.
///
/// Le stockage est un tableau indexé par `hand as usize`, jamais une table de
/// hachage, dont l'ordre d'itération n'est pas déterministe.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HandLevels {
    levels: [u8; 13],
}

impl Default for HandLevels {
    /// Les treize figures démarrent au niveau 1, jamais 0. `Default` ne peut
    /// pas être dérivé : le dérivé rendrait `[0; 13]`.
    fn default() -> Self {
        Self { levels: [1; 13] }
    }
}

impl HandLevels {
    /// Niveau courant de la figure.
    pub fn level(&self, hand: YahtzeeHand) -> u8 {
        self.levels[hand as usize]
    }

    /// Monte la figure d'un niveau, en saturant à `MAX_HAND_LEVEL`. Le
    /// Parchemin de grille est achetable en boucle : le plafond doit tenir.
    pub fn upgrade(&mut self, hand: YahtzeeHand) {
        let level = &mut self.levels[hand as usize];
        *level = level.saturating_add(1).min(MAX_HAND_LEVEL);
    }

    /// Base de la figure au niveau courant : `(Chips, Mult en centièmes)`.
    /// Au niveau 1, rend exactement `hand.base_score()`.
    pub fn base_for(&self, hand: YahtzeeHand) -> (u64, i64) {
        let (base_chips, base_mult) = hand.base_score();
        let extra = self.level(hand).saturating_sub(1);

        let chips = base_chips.saturating_add(u64::from(extra) * LEVEL_CHIPS_STEP);
        let mult = base_mult.saturating_add(i64::from(extra) * LEVEL_MULT_STEP);

        (chips, mult)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Table normative du § 4.1 du glossaire, recopiée en dur : ce test compare
    /// des valeurs, il ne vérifie pas la cohérence de `base_score` avec
    /// elle-même. Mult en centièmes.
    const EXPECTED: [(YahtzeeHand, u64, i64); 13] = [
        (YahtzeeHand::Aces, 5, 100),
        (YahtzeeHand::Twos, 10, 100),
        (YahtzeeHand::Threes, 15, 200),
        (YahtzeeHand::Fours, 20, 200),
        (YahtzeeHand::Fives, 25, 300),
        (YahtzeeHand::Sixes, 30, 300),
        (YahtzeeHand::ThreeOfAKind, 10, 200),
        (YahtzeeHand::FourOfAKind, 20, 300),
        (YahtzeeHand::FullHouse, 30, 400),
        (YahtzeeHand::SmallStraight, 25, 300),
        (YahtzeeHand::LargeStraight, 40, 400),
        (YahtzeeHand::Yahtzee, 50, 600),
        (YahtzeeHand::Chance, 5, 100),
    ];

    #[test]
    fn test_full_house_base_is_30_chips_4_mult() {
        assert_eq!(YahtzeeHand::FullHouse.base_score(), (30, 400));
    }

    #[test]
    fn test_base_score_table_is_normative() {
        for (index, (hand, chips, mult)) in EXPECTED.into_iter().enumerate() {
            assert_eq!(YahtzeeHand::ALL[index], hand, "ALL[{index}]");
            assert_eq!(hand.base_score(), (chips, mult), "{hand:?}");
        }
    }

    #[test]
    fn test_all_contains_thirteen_unique_hands() {
        assert_eq!(YahtzeeHand::ALL.len(), 13);

        // BTreeSet plutôt qu'un ensemble à ordre d'itération non déterministe :
        // le § 5 du glossaire proscrit ces derniers dans core_engine.
        let distinct: BTreeSet<YahtzeeHand> = YahtzeeHand::ALL.into_iter().collect();
        assert_eq!(distinct.len(), 13, "doublon dans ALL");
    }

    #[test]
    fn test_all_is_in_declaration_order() {
        for (index, hand) in YahtzeeHand::ALL.iter().enumerate() {
            assert_eq!(*hand as usize, index);
        }
    }

    #[test]
    fn test_yahtzee_hand_roundtrip_serde() {
        // Les noms sérialisés conditionnent la compatibilité des sauvegardes
        // (Étape 10) : renommer une variante casserait les parties en cours.
        const NAMES: [&str; 13] = [
            "Aces",
            "Twos",
            "Threes",
            "Fours",
            "Fives",
            "Sixes",
            "ThreeOfAKind",
            "FourOfAKind",
            "FullHouse",
            "SmallStraight",
            "LargeStraight",
            "Yahtzee",
            "Chance",
        ];

        for (hand, name) in YahtzeeHand::ALL.into_iter().zip(NAMES) {
            let encoded = serde_json::to_string(&hand).expect("sérialisation");
            assert_eq!(encoded, format!("\"{name}\""));

            let decoded: YahtzeeHand = serde_json::from_str(&encoded).expect("désérialisation");
            assert_eq!(decoded, hand);
        }
    }

    #[test]
    fn test_hand_level_up_adds_15_chips_and_100_mult() {
        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::FullHouse);

        assert_eq!(levels.level(YahtzeeHand::FullHouse), 2);
        assert_eq!(levels.base_for(YahtzeeHand::FullHouse), (45, 500));
    }

    #[test]
    fn test_upgrade_saturates_at_max_level() {
        let mut levels = HandLevels::default();
        for _ in 0..20 {
            levels.upgrade(YahtzeeHand::Yahtzee);
        }

        assert_eq!(levels.level(YahtzeeHand::Yahtzee), MAX_HAND_LEVEL);
        // Yams vaut (50, 600) au niveau 1 ; neuf niveaux de plus valent
        // +135 Chips et +900 centièmes.
        assert_eq!(levels.base_for(YahtzeeHand::Yahtzee), (185, 1500));
    }

    #[test]
    fn test_initial_level_is_one_for_all_hands() {
        let levels = HandLevels::default();

        for hand in YahtzeeHand::ALL {
            assert_eq!(levels.level(hand), 1, "{hand:?}");
        }
    }

    #[test]
    fn test_base_for_level_one_matches_base_score() {
        let levels = HandLevels::default();

        for hand in YahtzeeHand::ALL {
            assert_eq!(levels.base_for(hand), hand.base_score(), "{hand:?}");
        }
    }

    #[test]
    fn test_upgrade_does_not_touch_other_hands() {
        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::Yahtzee);

        assert_eq!(levels.level(YahtzeeHand::Yahtzee), 2);
        for hand in YahtzeeHand::ALL {
            if hand == YahtzeeHand::Yahtzee {
                continue;
            }
            assert_eq!(levels.level(hand), 1, "{hand:?}");
        }
    }

    #[test]
    fn test_hand_levels_roundtrip_serde() {
        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::FullHouse);
        for _ in 0..3 {
            levels.upgrade(YahtzeeHand::Chance);
        }

        // FullHouse est en position 8, Chance en position 12 : la forme
        // sérialisée part dans les sauvegardes de l'Étape 10.
        let encoded = serde_json::to_string(&levels).expect("sérialisation");
        assert_eq!(encoded, r#"{"levels":[1,1,1,1,1,1,1,1,2,1,1,1,4]}"#);

        let decoded: HandLevels = serde_json::from_str(&encoded).expect("désérialisation");
        assert_eq!(decoded, levels);
    }
}
