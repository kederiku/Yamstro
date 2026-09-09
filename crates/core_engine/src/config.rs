//! Configuration de run : RunConfig, apply_delta, effective_rerolls.

use crate::cups::CupDeck;

/// Valeurs de gameplay d'une run, toutes dérivées du gobelet. Aucune n'est une
/// constante du moteur : rien ici ne présuppose un nombre de dés ni un nombre
/// de relances (ADR-007).
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
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

/// Applique un delta signé en saturant des deux côtés. Le `-` nu est proscrit.
pub fn apply_delta(base: u8, delta: i8) -> u8 {
    // `unsigned_abs` et non la négation : `-i8::MIN` déborde et panique.
    if delta.is_negative() {
        base.saturating_sub(delta.unsigned_abs())
    } else {
        base.saturating_add(delta.unsigned_abs())
    }
}

/// `blind_cap` correspond à `BlindModifier::MaxRerolls` (boss L'Étau).
///
/// Chaîne d'application, dans cet ordre exact et non négociable : base du
/// gobelet, puis stake, puis plafond de manche, puis relique. Le plafond est un
/// minimum, jamais une affectation, et le maillon relique lui étant postérieur,
/// un delta de relique positif dépasse légitimement le plafond.
pub fn effective_rerolls(
    config: &RunConfig,
    stake_delta: i8,
    blind_cap: Option<u8>,
    relic_delta: i8,
) -> u8 {
    let after_stake = apply_delta(config.base_rerolls, stake_delta);
    let after_blind = blind_cap.map_or(after_stake, |cap| after_stake.min(cap));

    apply_delta(after_blind, relic_delta)
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

    #[test]
    fn test_rerolls_underflow_saturates() {
        // Gobelet Abandonné (aucune relance) sous Stake 4, qui retire une
        // relance, et sous le boss L'Étau, qui plafonne à une. Sans saturation
        // le compteur u8 repasserait à 255 en release et paniquerait en debug.
        let config = RunConfig::from_cup(&cup(CupId::Abandoned));

        assert_eq!(effective_rerolls(&config, -1, Some(1), 0), 0);
    }

    #[test]
    fn test_rerolls_nominal() {
        let config = RunConfig::from_cup(&cup(CupId::Standard));

        assert_eq!(effective_rerolls(&config, 0, None, 0), 2);
    }

    #[test]
    fn test_blind_cap_alone_lowers() {
        let config = RunConfig::from_cup(&cup(CupId::Standard));

        assert_eq!(effective_rerolls(&config, 0, Some(1), 0), 1);
    }

    #[test]
    fn test_blind_cap_never_raises() {
        // Le plafond est un min, jamais une affectation : il ne relève rien.
        let config = RunConfig::from_cup(&cup(CupId::Abandoned));

        assert_eq!(effective_rerolls(&config, 0, Some(3), 0), 0);
    }

    #[test]
    fn test_positive_relic_delta() {
        let config = RunConfig::from_cup(&cup(CupId::Standard));

        assert_eq!(effective_rerolls(&config, 0, None, 2), 4);
    }

    #[test]
    fn test_relic_delta_may_exceed_blind_cap() {
        // Conséquence assumée de l'ordre : le maillon relique est postérieur au
        // plafond, donc un delta positif le dépasse légitimement.
        let config = RunConfig::from_cup(&cup(CupId::Standard));

        assert_eq!(effective_rerolls(&config, 0, Some(1), 2), 3);
    }

    #[test]
    fn test_chain_order_matters() {
        // Plafonner avant d'appliquer le stake donnerait 4 : ce test verrouille
        // l'ordre normatif base, stake, plafond, relique.
        let config = RunConfig::from_cup(&cup(CupId::Standard));

        assert_eq!(effective_rerolls(&config, 2, Some(3), 0), 3);
    }

    #[test]
    fn test_apply_delta_saturates_both_ends() {
        assert_eq!(apply_delta(0, -1), 0);
        assert_eq!(apply_delta(u8::MAX, 1), u8::MAX);
        // i8::MIN passe par unsigned_abs : sa négation déborderait.
        assert_eq!(apply_delta(0, i8::MIN), 0);
    }
}
