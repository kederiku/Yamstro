//! Modèle de dé : DieId, Die, DieSeal, DieModifier.

use rand::{Rng, RngExt};

/// Nombre de faces maximal accepté par le moteur. Borne technique et non
/// valeur de gameplay : le nombre de faces réel vient du gobelet.
pub const MAX_DIE_SIDES: u8 = 20;

/// Identifiant d'un dé, unique et jamais réutilisé au sein d'une même run.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct DieId(pub u32);

/// Sceau gravé sur un dé. Les effets sont spécifiés à l'Étape 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DieSeal {
    Gold,
    Red,
    Blue,
    Purple,
}

/// Modificateur porté par un dé. Le catalogue s'étend à l'Étape 9.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DieModifier {
    BonusChips(u64),
    /// Mult exprimé en centièmes : 150 vaut +1,50 Mult.
    BonusMult(i64),
}

/// Un dé unitaire : sa valeur courante, son nombre de faces, son verrouillage,
/// son sceau et ses modificateurs.
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
        // BTreeSet et non HashSet : § 5 du glossaire proscrit les conteneurs
        // à ordre d'itération non déterministe dans core_engine.
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
