//! Reliques : RelicId, RelicInstance, RelicState, RelicInventory, CATALOG.

pub mod definitions;
pub mod effects;
pub mod inventory;

pub use definitions::rarity_of;
pub use inventory::RelicInventory;

/// Identité d'une relique. Enum **unit-only** : aucune variante ne porte de
/// donnée, ce qui le garde `Copy` et donc stockable dans un `StepSource` lui
/// aussi `Copy`. Les paramètres d'une relique vivent dans `effects_for`
/// (TASK-21), jamais dans son identité.
///
/// Les douze premières sont les reliques **de production**, dans l'ordre du
/// catalogue ; les trois dernières sont des **fixtures** `#[cfg(test)]`, qui ne
/// franchissent pas la frontière de crate — aucun test de `tests/` ne peut les
/// nommer, et elles n'entrent jamais dans `CATALOG`.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RelicId {
    CrackedDie,
    PolishedStone,
    TripletMaster,
    FullHouseArchitect,
    StellarAlignment,
    PyramidOfSixes,
    Pendulum,
    UnstableObsidian,
    DivineYahtzee,
    ClayPiggyBank,
    GhostDie,
    DoubleMirror,
    #[cfg(test)]
    SixFire,
    #[cfg(test)]
    MagicPair,
    #[cfg(test)]
    BrokenGlass,
}

/// Palier de rareté d'une relique.
///
/// **`Legendary` est déclarée sans porteur.** Le palier arrive à l'Étape 9 ;
/// la déclarer maintenant évite qu'une sauvegarde de l'Étape 10 ou un
/// modificateur de blind qui l'énumère aient à rouvrir ce type. Le glossaire
/// annonce un `BlindModifier::DisableRarity(RelicRarity)` qui **n'existe pas
/// encore** dans le code : c'est une référence en avant, pas une dépendance.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RelicRarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

/// État mutable d'une relique en cours de run. `None` est une variante de cet
/// enum, et non `Option::None` : une relique sans état est un cas normal, pas
/// une absence de relique.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RelicState {
    None,
    Counter(u32),
    Perishable { rounds_left: u8 },
    Disabled,
}

/// Une relique possédée. `uid` distingue deux copies d'une même définition ;
/// c'est lui, et non `def`, qui identifie l'exemplaire.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RelicInstance {
    pub uid: u32,
    pub def: RelicId,
    pub state: RelicState,
}

/// Catalogue des reliques **tirables**. Les trois fixtures `#[cfg(test)]` n'y
/// figurent jamais : elles ne deviennent pas tirables (C16), et la crate ne
/// compile même pas si l'une d'elles y entre, la bibliothèque devant aussi se
/// construire sans `cfg(test)` pour la cible d'intégration.
///
/// **La composition n'est pas la loi de tirage.** Quatre Communes, quatre Peu
/// communes et quatre Rares décrivent les archétypes couverts ; les taux de la
/// boutique sont un livrable de l'Étape 6, qui lira `rarity_of` sur ces entrées.
pub const CATALOG: &[RelicId] = &[
    RelicId::CrackedDie,
    RelicId::PolishedStone,
    RelicId::TripletMaster,
    RelicId::FullHouseArchitect,
    RelicId::StellarAlignment,
    RelicId::PyramidOfSixes,
    RelicId::Pendulum,
    RelicId::UnstableObsidian,
    RelicId::DivineYahtzee,
    RelicId::ClayPiggyBank,
    RelicId::GhostDie,
    RelicId::DoubleMirror,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(uid: u32, def: RelicId, state: RelicState) -> RelicInstance {
        RelicInstance { uid, def, state }
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "déborde le u8")]
    fn test_iter_slots_asserts_beyond_the_u8_range() {
        // Au-delà de 256 slots, deux slots distincts se présenteraient sous le
        // même numéro. L'assertion est le seul garde-fou de cet invariant, et
        // sans ce test rien ne vérifie qu'elle est encore là.
        let mut inventory = RelicInventory::new(0);
        inventory.slots = vec![None; 257];

        let _ = inventory.iter_slots().count();
    }

    #[test]
    fn test_iter_slots_is_ordered_and_skips_empty() {
        let mut inventory = RelicInventory::new(0);
        inventory.slots = vec![
            Some(inst(1, RelicId::SixFire, RelicState::None)),
            None,
            Some(inst(2, RelicId::MagicPair, RelicState::Counter(3))),
            None,
            Some(inst(3, RelicId::BrokenGlass, RelicState::Disabled)),
        ];

        let slots: Vec<u8> = inventory.iter_slots().map(|(slot, _)| slot).collect();
        assert_eq!(slots, vec![0, 2, 4]);
        assert_eq!(inventory.len(), 3);
        assert_eq!(inventory.capacity(), 5);
        assert!(!inventory.is_empty());
    }

    #[test]
    fn test_catalog_has_no_duplicates() {
        // Vide à cette étape, douze entrées à l'Étape 5 : la comparaison deux à
        // deux reste valide dans les deux cas.
        for (position, left) in CATALOG.iter().enumerate() {
            for right in &CATALOG[position + 1..] {
                assert_ne!(left, right);
            }
        }
    }

    #[test]
    fn test_uids_are_unique_in_inventory() {
        // Deux copies d'une même définition, mêmes état et même figure, ne
        // diffèrent que par leur `uid` : elles doivent rester distinguables,
        // sinon l'Étape 5 ne pourrait plus retirer l'une sans l'autre.
        let first = inst(1, RelicId::SixFire, RelicState::Counter(4));
        let second = inst(2, RelicId::SixFire, RelicState::Counter(4));
        assert_ne!(first, second);

        let json = serde_json::to_string(&(first, second)).expect("sérialisation");
        let (back_first, back_second): (RelicInstance, RelicInstance) =
            serde_json::from_str(&json).expect("désérialisation");
        assert_ne!(back_first, back_second);
        assert_eq!(back_first.def, back_second.def);

        let mut inventory = RelicInventory::new(0);
        inventory.slots = vec![Some(first), None, Some(second)];
        let uids: Vec<u32> = inventory.iter_slots().map(|(_, relic)| relic.uid).collect();
        assert_eq!(uids, vec![1, 2]);
    }
}
