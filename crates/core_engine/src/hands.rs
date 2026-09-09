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

        // BTreeSet et non HashSet : § 5 du glossaire proscrit les conteneurs à
        // ordre d'itération non déterministe dans core_engine.
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
}
