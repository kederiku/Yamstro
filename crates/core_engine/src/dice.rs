//! Modèle de dé : DieId, Die, DieSeal, DieModifier.

#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;

use rand::{Rng, RngExt};

/// Nombre de faces maximal accepté par le moteur. Borne technique et non
/// valeur de gameplay : le nombre de faces réel vient du gobelet.
pub const MAX_DIE_SIDES: u8 = 20;

/// Identifiant d'un dé, unique et jamais réutilisé au sein d'une même run.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct DieId(pub u32);

/// Sceau gravé sur un dé. Les effets sont spécifiés à l'Étape 9.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DieSeal {
    Gold,
    Red,
    Blue,
    Purple,
}

/// Modificateur porté par un dé. Le catalogue s'étend à l'Étape 9.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DieModifier {
    BonusChips(u64),
    /// Mult exprimé en centièmes : 150 vaut +1,50 Mult.
    BonusMult(i64),
}

/// Un dé unitaire : sa valeur courante, son nombre de faces, son verrouillage,
/// son sceau et ses modificateurs.
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::component::Component, bevy_reflect::Reflect),
    reflect(Component)
)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Die {
    pub id: DieId,
    pub current_value: u8,
    pub sides: u8,
    pub locked: bool,
    pub seal: Option<DieSeal>,
    pub modifiers: Vec<DieModifier>,
}

impl Die {
    /// `sides` est borné à `1..=MAX_DIE_SIDES`. `current_value` démarre à 1,
    /// le dé est déverrouillé, sans sceau et sans modificateur.
    pub fn new(id: DieId, sides: u8) -> Self {
        Self {
            id,
            current_value: 1,
            sides: sides.clamp(1, MAX_DIE_SIDES),
            locked: false,
            seal: None,
            modifiers: Vec::new(),
        }
    }

    // La borne `Rng + RngExt` est redondante — `RngExt` a `Rng` pour supertrait
    // — mais elle est imposée verbatim par le § 2.3 du ticket et par la
    // check-list § 12 des Contraintes Bevy 0.19.1. L'attribut lève le refus de
    // clippy sans modifier la signature d'un seul caractère.
    #[allow(clippy::implied_bounds_in_impls)]
    /// Relance le dé. Sans `force`, un dé verrouillé n'est jamais modifié.
    /// `force = true` est réservé aux effets qui relancent explicitement un dé
    /// verrouillé (Rune de Mutation, reliques de manipulation — Étape 9).
    pub fn roll(&mut self, rng: &mut (impl Rng + RngExt), force: bool) {
        if self.locked && !force {
            return;
        }
        // `max(1)` défend la borne haute : les champs étant publics, un `sides`
        // à 0 poserait un intervalle vide, sur lequel `random_range` panique.
        self.current_value = rng.random_range(1..=self.sides.max(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::collections::BTreeSet;

    #[test]
    fn test_roll_reaches_every_face() {
        // Une borne haute exclusive ferait qu'un D6 ne montre jamais 6. Le dé
        // bougerait, les partitions resteraient justes, et rien d'autre dans
        // la suite ne le verrait : c'est le seul test qui garde
        // l'atteignabilité de la face la plus haute.
        for sides in [2_u8, 6, 8, MAX_DIE_SIDES] {
            let mut rng = ChaCha8Rng::seed_from_u64(u64::from(sides));
            let mut die = Die::new(DieId(0), sides);
            let mut seen = BTreeSet::new();

            for _ in 0..1_000 {
                die.roll(&mut rng, false);
                seen.insert(die.current_value);
            }

            let attendu: BTreeSet<u8> = (1..=sides).collect();
            assert_eq!(seen, attendu, "dé à {sides} faces");
        }
    }

    #[test]
    fn test_new_die_starts_unlocked_on_face_one() {
        // L'état initial est un contrat documenté, et il s'affiche avant le
        // premier lancer.
        let die = Die::new(DieId(7), 8);

        assert_eq!(die.id, DieId(7));
        assert_eq!(die.current_value, 1);
        assert_eq!(die.sides, 8);
        assert!(!die.locked);
        assert_eq!(die.seal, None);
        assert!(die.modifiers.is_empty());
    }

    #[test]
    fn test_locked_die_is_not_rerolled() {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut die = Die::new(DieId(0), 6);
        die.locked = true;
        let before = die.current_value;

        for _ in 0..10 {
            die.roll(&mut rng, false);
            assert_eq!(die.current_value, before);
        }
    }

    #[test]
    fn test_forced_roll_moves_locked_die() {
        // Graine déterminée empiriquement : ChaCha8Rng::seed_from_u64(0) rend 4
        // au premier tirage dans 1..=6, alors qu'un dé neuf porte la valeur 1.
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut die = Die::new(DieId(0), 6);
        die.locked = true;
        let before = die.current_value;

        die.roll(&mut rng, true);

        assert_ne!(die.current_value, before);
    }

    #[test]
    fn test_roll_stays_within_sides() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);

        for sides in [6_u8, 8, 20] {
            let mut die = Die::new(DieId(u32::from(sides)), sides);
            for _ in 0..200 {
                die.roll(&mut rng, false);
                assert!(
                    (1..=sides).contains(&die.current_value),
                    "valeur {} hors de 1..={sides}",
                    die.current_value
                );
            }
        }
    }

    #[test]
    fn test_unlocked_die_actually_moves() {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut die = Die::new(DieId(0), 6);
        // Un ensemble ordonné, et non une table de hachage : le § 5 du
        // glossaire proscrit dans core_engine tout conteneur dont l'ordre
        // d'itération n'est pas déterministe.
        let mut seen = BTreeSet::new();

        for _ in 0..20 {
            die.roll(&mut rng, false);
            seen.insert(die.current_value);
        }

        assert!(seen.len() >= 2, "une seule valeur observée : {seen:?}");
    }

    #[test]
    fn test_sides_are_clamped() {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut narrowest = Die::new(DieId(0), 0);
        let mut widest = Die::new(DieId(1), 255);

        assert_eq!(narrowest.sides, 1);
        assert_eq!(widest.sides, MAX_DIE_SIDES);

        narrowest.roll(&mut rng, false);
        widest.roll(&mut rng, false);

        assert_eq!(narrowest.current_value, 1);
        assert!((1..=MAX_DIE_SIDES).contains(&widest.current_value));
    }

    #[test]
    fn test_die_roundtrip_serde() {
        let die = Die {
            id: DieId(42),
            current_value: 5,
            sides: 8,
            locked: true,
            seal: Some(DieSeal::Purple),
            modifiers: vec![DieModifier::BonusChips(30), DieModifier::BonusMult(150)],
        };

        let encoded = serde_json::to_string(&die).expect("sérialisation");
        let decoded: Die = serde_json::from_str(&encoded).expect("désérialisation");

        assert_eq!(die, decoded);
    }
}

// Les deux tests d'intégration ECS de TASK-14 vivent ici plutôt que dans un
// neuvième module : `Die` est le sujet des deux, et c'est lui que le piège du
// § 2.1 détruirait. `lib.rs` reste le bloc verbatim posé par TASK-01.
#[cfg(all(test, feature = "bevy"))]
mod bevy_tests {
    use super::{Die, DieId};
    use crate::hands::{HandLevels, YahtzeeHand};
    use bevy_ecs::world::World;

    #[test]
    fn test_die_is_component_and_hand_levels_is_resource() {
        let mut world = World::new();

        let die = Die::new(DieId(7), 8);
        let entity = world.spawn(die.clone()).id();

        let mut levels = HandLevels::default();
        levels.upgrade(YahtzeeHand::FullHouse);
        world.insert_resource(levels.clone());

        assert_eq!(world.get::<Die>(entity), Some(&die));
        assert_eq!(world.resource::<HandLevels>(), &levels);
        assert_eq!(
            world.resource::<HandLevels>().level(YahtzeeHand::FullHouse),
            2
        );
    }

    #[test]
    fn test_multiple_dice_entities_coexist() {
        let mut world = World::new();

        // Cinq dés distincts par identifiant et par face affichée : un `Resource`
        // posé par erreur sur `Die` despawnerait les quatre premiers sans la
        // moindre erreur de compilation.
        let dice: Vec<Die> = (0..5)
            .map(|index| {
                let mut die = Die::new(DieId(index), 6);
                die.current_value = (index as u8) + 1;
                die
            })
            .collect();

        let entities: Vec<_> = dice
            .iter()
            .map(|die| world.spawn(die.clone()).id())
            .collect();

        for (entity, die) in entities.iter().zip(&dice) {
            assert_eq!(world.get::<Die>(*entity), Some(die));
        }
    }
}
