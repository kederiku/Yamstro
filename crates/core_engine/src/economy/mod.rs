//! Économie de run.
//!
//! **L'Étape 6 en est propriétaire** : prix de boutique, intérêts et revente
//! viendront ici. L'Étape 5 n'y verse qu'une fonction, celle dont l'or des
//! reliques a besoin, et elle est posée à sa place définitive plutôt que
//! rangée provisoirement dans le module des reliques.

pub mod payout;

pub use payout::{INTEREST_TRANCHE, Payout, calculate_payout};

use crate::relics::RelicInventory;
use crate::relics::effects::gold_for;
use crate::scoring::TriggerCtx;

/// Somme des `gold_for` sur les slots occupés et non `Disabled`, de gauche à
/// droite.
///
/// Le contexte de chaque slot se rebâtit comme en phase A du pipeline :
/// l'identifiant, le rang et **l'état propre de l'instance** écrasent ceux du
/// contexte de base. Lire l'état du contexte de base ferait rendre à toutes les
/// reliques la valeur de la première.
///
/// Somme en saturation. Aucune capacité d'inventaire actuelle ne peut faire
/// déborder un `u32`, mais le choix est écrit plutôt que laissé au hasard de
/// cette capacité.
#[must_use]
pub fn round_end_gold(inventory: &RelicInventory, base: &TriggerCtx<'_>) -> u32 {
    inventory
        .iter_slots()
        .filter(|(_, inst)| inst.participe())
        .fold(0u32, |total, (slot, inst)| {
            let ctx = TriggerCtx {
                uid: inst.uid,
                slot,
                state: inst.state,
                ..*base
            };
            total.saturating_add(gold_for(inst.def, &ctx))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blinds::{BlindContext, BlindDefinition};
    use crate::dice::{Die, DieId};
    use crate::evaluator::HandMatch;
    use crate::hands::{HandGrid, HandLevels, YahtzeeHand};
    use crate::relics::{RelicId, RelicState};
    use crate::scoring::TriggerCtx;

    /// Le filtre sur `Disabled` ne se teste **qu'avec une relique dont l'or ne
    /// dépend pas de son état** : toute relique de production lit son propre
    /// compteur, donc éteinte elle rend zéro par son bras à elle, et le mutant
    /// qui retire le filtre survit. `BrokenGlass` rend trois sans condition.
    #[test]
    fn test_disabled_slot_is_skipped_even_when_its_gold_ignores_state() {
        let hand = HandMatch {
            hand: YahtzeeHand::Chance,
            scoring_dice: vec![DieId(0)],
            discarded_dice: Vec::new(),
            potential_score: 0,
        };
        let dice = vec![Die::new(DieId(0), 6)];
        let levels = HandLevels::default();
        let blind = BlindContext {
            blind: BlindDefinition::default(),
            target_score: 300,
            current_score: 0,
            hands_remaining: 4,
            used_hands: HandGrid::default(),
        };
        let base = TriggerCtx {
            hand: &hand,
            dice: &dice,
            hand_levels: &levels,
            blind: &blind,
            uid: 0,
            slot: 0,
            state: RelicState::None,
            die: None,
            base_chips: 0,
            base_mult: 0,
            left_effects: &[],
            roll_index: u8::MAX,
            rerolls_left: 0,
        };

        let mut inventory = RelicInventory::new(3);
        inventory.add_relic(RelicId::BrokenGlass).expect("slot 0");
        inventory.add_relic(RelicId::BrokenGlass).expect("slot 1");
        assert_eq!(round_end_gold(&inventory, &base), 6);

        inventory.slots[1].as_mut().expect("slot 1").state = RelicState::Disabled;
        assert_eq!(
            round_end_gold(&inventory, &base),
            3,
            "le slot éteint a versé malgré tout"
        );
    }
}
