//! Reliques : RelicId, RelicInstance, RelicState, RelicInventory, CATALOG.

#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;

/// Identité d'une relique. Enum **unit-only** : aucune variante ne porte de
/// donnée, ce qui le garde `Copy` et donc stockable dans un `StepSource` lui
/// aussi `Copy`. Les paramètres d'une relique vivent dans `effects_for`
/// (TASK-21), jamais dans son identité.
///
/// Hors build de test, cet enum est **inhabité** : les douze reliques de
/// production arrivent à l'Étape 5, et les trois variantes ci-dessous ne sont
/// que des fixtures. Elles ne franchissent pas la frontière de crate, donc
/// aucun test de `tests/` ne peut les nommer.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RelicId {
    #[cfg(test)]
    SixFire,
    #[cfg(test)]
    MagicPair,
    #[cfg(test)]
    BrokenGlass,
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

/// Unique source de vérité de l'inventaire. L'ordre des slots **est** l'ordre
/// d'application des effets (ADR-005) : il n'est jamais retrié, par quoi que ce
/// soit. `slots` est dimensionné par l'appelant depuis
/// `RunConfig.relic_capacity` (ADR-007), jamais depuis un littéral.
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::resource::Resource, bevy_reflect::Reflect),
    reflect(Component)
)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RelicInventory {
    pub slots: Vec<Option<RelicInstance>>,
}

impl RelicInventory {
    /// Slots occupés, de gauche à droite, numéro de slot compris. Les `None`
    /// sont sautés ; rien d'autre ne l'est, et rien n'est trié.
    ///
    /// **Invariant :** `slots.len() <= 256`, faute de quoi le numéro de slot ne
    /// tient pas dans un `u8` et deux slots distincts se présenteraient sous le
    /// même numéro. `RunConfig.relic_capacity` étant un `u8`, la configuration
    /// ne peut pas l'enfreindre ; un `Vec` construit à la main ou relu d'une
    /// sauvegarde le peut, d'où l'assertion.
    pub fn iter_slots(&self) -> impl Iterator<Item = (u8, &RelicInstance)> + '_ {
        debug_assert!(
            self.slots.len() <= usize::from(u8::MAX) + 1,
            "inventaire de {} slots : le numéro de slot déborde le u8",
            self.slots.len()
        );
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|relic| (index as u8, relic)))
    }

    /// Nombre de reliques possédées, c'est-à-dire de slots occupés. C'est la
    /// forme qu'attend la boutique : `relics.len() < config.relic_capacity`.
    pub fn len(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    /// Vrai quand aucune relique n'est possédée, même si des slots existent.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Nombre de slots, occupés ou non. À ne pas confondre avec `len()` :
    /// les intervertir inverse la condition d'achat.
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }
}

/// Pool de tirage de la boutique (Étape 6). Vide à cette étape : les trois
/// fixtures n'y figurent pas, et les douze reliques de production arrivent à
/// l'Étape 5.
pub const CATALOG: &[RelicId] = &[];

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(uid: u32, def: RelicId, state: RelicState) -> RelicInstance {
        RelicInstance { uid, def, state }
    }

    #[test]
    fn test_iter_slots_is_ordered_and_skips_empty() {
        let inventory = RelicInventory {
            slots: vec![
                Some(inst(1, RelicId::SixFire, RelicState::None)),
                None,
                Some(inst(2, RelicId::MagicPair, RelicState::Counter(3))),
                None,
                Some(inst(3, RelicId::BrokenGlass, RelicState::Disabled)),
            ],
        };

        let slots: Vec<u8> = inventory.iter_slots().map(|(slot, _)| slot).collect();
        assert_eq!(slots, vec![0, 2, 4]);
        assert_eq!(inventory.len(), 3);
        assert_eq!(inventory.capacity(), 5);
        assert!(!inventory.is_empty());
    }

    #[test]
    fn test_relic_inventory_serde_roundtrip() {
        // Les quatre variantes de `RelicState`, plus un slot vide : c'est ce
        // test qui interdit le retour d'un objet-trait, qui ne se sérialise pas.
        let inventory = RelicInventory {
            slots: vec![
                Some(inst(1, RelicId::SixFire, RelicState::None)),
                Some(inst(2, RelicId::MagicPair, RelicState::Counter(7))),
                None,
                Some(inst(
                    3,
                    RelicId::BrokenGlass,
                    RelicState::Perishable { rounds_left: 2 },
                )),
                Some(inst(4, RelicId::SixFire, RelicState::Disabled)),
            ],
        };

        let json = serde_json::to_string(&inventory).expect("sérialisation");
        let back: RelicInventory = serde_json::from_str(&json).expect("désérialisation");
        assert_eq!(inventory, back);
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

        let inventory = RelicInventory {
            slots: vec![Some(first), None, Some(second)],
        };
        let uids: Vec<u32> = inventory.iter_slots().map(|(_, relic)| relic.uid).collect();
        assert_eq!(uids, vec![1, 2]);
    }
}
