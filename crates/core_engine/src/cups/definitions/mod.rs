//! Définitions des gobelets : seul endroit du moteur où les valeurs de
//! gameplay sont écrites en dur (ADR-007).

use super::{CupDeck, CupId};

/// Fabrique le gobelet demandé. Le `match` est exhaustif : ajouter une variante
/// à `CupId` sans la définir ici casse la compilation.
pub fn cup(id: CupId) -> CupDeck {
    let standard = CupDeck {
        id,
        starting_gold: 4,
        sides: vec![6, 6, 6, 6, 6],
        dice_count: 5,
        base_rerolls: 2,
        relic_capacity: 5,
        consumable_capacity: 2,
        max_interest: 5,
        hands_per_blind: 4,
    };

    match id {
        CupId::Standard => standard,
        CupId::Abandoned => CupDeck {
            base_rerolls: 0,
            ..standard
        },
        CupId::Polyhedron => CupDeck {
            sides: vec![6, 6, 6, 6, 8],
            ..standard
        },
        CupId::Cheater => CupDeck {
            dice_count: 6,
            base_rerolls: 1,
            sides: vec![6, 6, 6, 6, 6, 6],
            ..standard
        },
        CupId::Fortune => CupDeck {
            relic_capacity: 6,
            starting_gold: 15,
            ..standard
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RunConfig;

    #[test]
    fn test_standard_cup_derives_five_dice_two_rerolls() {
        let config = RunConfig::from_cup(&cup(CupId::Standard));

        assert_eq!(config.dice_count, 5);
        assert_eq!(config.base_rerolls, 2);
        assert_eq!(config.relic_capacity, 5);
        assert_eq!(config.consumable_capacity, 2);
        assert_eq!(config.max_interest, 5);
        assert_eq!(config.hands_per_blind, 4);
    }

    #[test]
    fn test_cheater_cup_has_six_dice() {
        let config = RunConfig::from_cup(&cup(CupId::Cheater));

        assert_eq!(config.dice_count, 6);
        assert_eq!(config.base_rerolls, 1);
    }

    #[test]
    fn test_abandoned_cup_has_zero_rerolls() {
        let config = RunConfig::from_cup(&cup(CupId::Abandoned));

        assert_eq!(config.base_rerolls, 0);
    }

    #[test]
    fn test_fortune_cup_has_six_relic_slots() {
        let deck = cup(CupId::Fortune);
        let config = RunConfig::from_cup(&deck);

        assert_eq!(config.relic_capacity, 6);
        assert_eq!(deck.starting_gold, 15);
    }

    #[test]
    fn test_polyhedron_carries_a_d8() {
        let deck = cup(CupId::Polyhedron);
        let config = RunConfig::from_cup(&deck);

        assert_eq!(deck.sides, [6, 6, 6, 6, 8]);
        assert_eq!(config.dice_count, 5);
    }

    #[test]
    fn test_cup_deck_roundtrip_serde() {
        let deck = cup(CupId::Polyhedron);

        let encoded = serde_json::to_string(&deck).expect("sérialisation");
        let decoded: CupDeck = serde_json::from_str(&encoded).expect("désérialisation");

        assert_eq!(decoded, deck);
        assert!(encoded.contains(r#""id":"Polyhedron""#));
    }
}
