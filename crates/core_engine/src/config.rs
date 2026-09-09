//! Configuration de run : RunConfig, apply_delta, effective_rerolls.

use crate::cups::CupDeck;

/// Valeurs de gameplay d'une run, toutes dérivées du gobelet. Aucune n'est une
/// constante du moteur : rien ici ne présuppose un nombre de dés ni un nombre
/// de relances (ADR-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunConfig {
    pub dice_count: u8,
    pub base_rerolls: u8,
    pub relic_capacity: u8,
    pub consumable_capacity: u8,
    pub max_interest: u32,
    pub hands_per_blind: u8,
}

impl RunConfig {
    /// Seul point de création. Les six affectations sont listées une à une,
    /// pour qu'un champ oublié saute aux yeux plutôt que d'être masqué par une
    /// conversion implicite.
    pub fn from_cup(deck: &CupDeck) -> RunConfig {
        RunConfig {
            dice_count: deck.dice_count,
            base_rerolls: deck.base_rerolls,
            relic_capacity: deck.relic_capacity,
            consumable_capacity: deck.consumable_capacity,
            max_interest: deck.max_interest,
            hands_per_blind: deck.hands_per_blind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cups::CupId;
    use crate::cups::definitions::cup;

    /// Les tests portant des valeurs de gameplay vivent dans
    /// `cups/definitions`, seul endroit du moteur où ces nombres ont droit de
    /// cité. Ne restent ici que ceux qui n'en citent aucun.
    #[test]
    fn test_sides_len_matches_dice_count() {
        for id in [
            CupId::Standard,
            CupId::Abandoned,
            CupId::Polyhedron,
            CupId::Cheater,
            CupId::Fortune,
        ] {
            let deck = cup(id);
            let config = RunConfig::from_cup(&deck);

            assert_eq!(deck.sides.len() as u8, config.dice_count, "{id:?}");
        }
    }

    #[test]
    fn test_run_config_roundtrip_serde() {
        let config = RunConfig::from_cup(&cup(CupId::Standard));

        let encoded = serde_json::to_string(&config).expect("sérialisation");
        let decoded: RunConfig = serde_json::from_str(&encoded).expect("désérialisation");

        assert_eq!(decoded, config);
    }
}
