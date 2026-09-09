//! Main active : DicePool.

use crate::config::RunConfig;
use crate::dice::{Die, DieId};
use rand::{Rng, RngExt};

/// La main active, de taille variable, adressée exclusivement par identifiant.
///
/// Aucun index de position n'a de sens ici : un dé peut être retiré au milieu
/// d'une manche, et toute réindexation invaliderait les identifiants que les
/// reliques ont mémorisés.
///
/// Ce type ne tient aucun compteur de relance : ce compteur appartient au
/// contexte de main de l'Étape 3.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DicePool {
    dice: Vec<Die>,
    /// Compteur monotone, jamais décrémenté : un identifiant n'est jamais
    /// réattribué au sein d'une run.
    next_id: u32,
}

impl DicePool {
    /// `sides` décrit le gobelet, ex. [6,6,6,6,8]
    ///
    /// Construit `config.dice_count` dés. Le nombre de faces du dé d'indice `i`
    /// vient de `sides[i]`, à défaut du dernier élément. Le bornage à
    /// `MAX_DIE_SIDES` est celui de `Die::new`, seul endroit où il est écrit.
    /// Sans aucune face décrite, la main est vide plutôt que fautive.
    pub fn new(config: &RunConfig, sides: &[u8]) -> Self {
        let Some(&fallback) = sides.last() else {
            return Self {
                dice: Vec::new(),
                next_id: 0,
            };
        };

        let dice = (0..config.dice_count)
            .map(|index| {
                let faces = sides.get(usize::from(index)).copied().unwrap_or(fallback);
                Die::new(DieId(u32::from(index)), faces)
            })
            .collect();

        Self {
            dice,
            next_id: u32::from(config.dice_count),
        }
    }

    /// La main, dans son ordre courant.
    pub fn dice(&self) -> &[Die] {
        &self.dice
    }

    /// Nombre de dés en main.
    pub fn len(&self) -> usize {
        self.dice.len()
    }

    /// Vrai si la main ne contient aucun dé.
    pub fn is_empty(&self) -> bool {
        self.dice.is_empty()
    }

    /// Le dé portant cet identifiant, s'il est encore en main.
    pub fn get_mut(&mut self, id: DieId) -> Option<&mut Die> {
        self.dice.iter_mut().find(|die| die.id == id)
    }

    /// Inverse le verrouillage et rend le **nouvel** état. Sur un identifiant
    /// inconnu, rend `false` sans rien modifier.
    pub fn toggle_lock(&mut self, id: DieId) -> bool {
        match self.get_mut(id) {
            Some(die) => {
                die.locked = !die.locked;
                die.locked
            }
            None => false,
        }
    }

    // La borne `Rng + RngExt` est redondante, `RngExt` ayant `Rng` pour
    // supertrait, mais elle est imposée verbatim par le document source et par
    // la check-list § 12 des Contraintes Bevy 0.19.1. L'attribut lève le refus
    // de clippy sans modifier la signature.
    #[allow(clippy::implied_bounds_in_impls)]
    /// Relance la main et rend les identifiants effectivement relancés, soit
    /// ceux dont le dé n'était pas verrouillé, ou tous si `force`.
    ///
    /// Un dé relancé qui retombe sur la même face figure quand même dans la
    /// liste : ce qui est rendu, ce sont les dés relancés, pas les dés modifiés.
    pub fn roll_all(&mut self, rng: &mut (impl Rng + RngExt), force: bool) -> Vec<DieId> {
        let mut rolled = Vec::new();

        for die in &mut self.dice {
            if !die.locked || force {
                rolled.push(die.id);
            }
            die.roll(rng, force);
        }

        rolled
    }

    /// Retire le dé de la main et le rend. Le compteur d'identifiants n'est pas
    /// décrémenté et les dés restants ne sont pas réindexés.
    pub fn remove(&mut self, id: DieId) -> Option<Die> {
        let index = self.dice.iter().position(|die| die.id == id)?;

        Some(self.dice.remove(index))
    }

    /// Ajoute un dé à la main. Son identifiant est **réécrit** : c'est le pool,
    /// et lui seul, qui les attribue.
    pub fn push(&mut self, mut die: Die) {
        die.id = DieId(self.next_id);
        // La saturation ne protège rien d'utile ici : atteindre le plafond
        // demanderait plus de quatre milliards de dés dans une seule run, et
        // rendrait de toute façon l'unicité intenable.
        self.next_id = self.next_id.saturating_add(1);

        self.dice.push(die);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cups::CupDeck;
    use crate::cups::CupId;
    use crate::cups::definitions::cup;
    use crate::dice::MAX_DIE_SIDES;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    /// Dérive un gobelet de test du standard, avec un autre nombre de dés. Les
    /// faces reprennent celles du standard, complétées par sa dernière valeur :
    /// aucun nombre de faces n'est inventé ici. Aucun gobelet du catalogue ne
    /// porte quatre dés, que le premier test exige pourtant.
    fn deck_of(dice_count: u8) -> CupDeck {
        let mut deck = cup(CupId::Standard);
        let face = deck.sides.last().copied().unwrap_or(1);

        deck.sides.resize(usize::from(dice_count), face);
        deck.dice_count = dice_count;
        deck
    }

    fn pool_of(dice_count: u8) -> DicePool {
        let deck = deck_of(dice_count);
        DicePool::new(&RunConfig::from_cup(&deck), &deck.sides)
    }

    #[test]
    fn test_pool_size_follows_config() {
        for dice_count in [4_u8, 5, 6] {
            let pool = pool_of(dice_count);

            assert_eq!(pool.len(), usize::from(dice_count));
            assert_eq!(pool.dice().len(), pool.len());
        }
    }

    #[test]
    fn test_pool_survives_die_removal() {
        let mut pool = pool_of(5);
        let ids: Vec<DieId> = pool.dice().iter().map(|die| die.id).collect();
        let middle = ids[2];

        assert!(pool.remove(middle).is_some());
        assert_eq!(pool.len(), 4);

        for id in ids.iter().filter(|id| **id != middle) {
            assert!(pool.get_mut(*id).is_some(), "{id:?} devrait rester valide");
        }
        assert!(pool.get_mut(middle).is_none());
    }

    #[test]
    fn test_push_never_reuses_die_id() {
        let mut pool = pool_of(5);
        let mut seen: Vec<DieId> = pool.dice().iter().map(|die| die.id).collect();

        for _ in 0..3 {
            let victim = pool
                .dice()
                .first()
                .map(|die| die.id)
                .expect("pool non vide");
            pool.remove(victim);
            pool.push(Die::new(DieId(0), 6));

            let fresh = pool.dice().last().map(|die| die.id).expect("pool non vide");
            assert!(
                seen.iter().all(|id| id.0 < fresh.0),
                "{fresh:?} devrait dépasser tous les identifiants déjà vus"
            );
            seen.push(fresh);
        }

        let mut ids: Vec<DieId> = pool.dice().iter().map(|die| die.id).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before, "doublon d'identifiant dans le pool");
    }

    #[test]
    fn test_toggle_lock_unknown_id_is_false() {
        let mut pool = pool_of(5);

        assert!(!pool.toggle_lock(DieId(9_999)));
        assert_eq!(pool.len(), 5);
        assert!(pool.dice().iter().all(|die| !die.locked));
    }

    #[test]
    fn test_roll_all_skips_locked() {
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let mut pool = pool_of(5);
        let ids: Vec<DieId> = pool.dice().iter().map(|die| die.id).collect();

        assert!(pool.toggle_lock(ids[1]));
        assert!(pool.toggle_lock(ids[3]));
        let before: Vec<u8> = pool
            .dice()
            .iter()
            .filter(|die| die.locked)
            .map(|die| die.current_value)
            .collect();

        let rolled = pool.roll_all(&mut rng, false);

        assert_eq!(rolled.len(), 3);
        assert!(!rolled.contains(&ids[1]));
        assert!(!rolled.contains(&ids[3]));

        let after: Vec<u8> = pool
            .dice()
            .iter()
            .filter(|die| die.locked)
            .map(|die| die.current_value)
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn test_roll_all_forced_returns_every_id() {
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let mut pool = pool_of(5);
        let ids: Vec<DieId> = pool.dice().iter().map(|die| die.id).collect();

        pool.toggle_lock(ids[1]);
        pool.toggle_lock(ids[3]);

        let rolled = pool.roll_all(&mut rng, true);

        assert_eq!(rolled, ids);
    }

    #[test]
    fn test_polyhedron_sides_are_honoured() {
        let deck = cup(CupId::Polyhedron);
        let pool = DicePool::new(&RunConfig::from_cup(&deck), &deck.sides);
        let faces: Vec<u8> = pool.dice().iter().map(|die| die.sides).collect();

        assert_eq!(faces, [6, 6, 6, 6, 8]);
    }

    #[test]
    fn test_empty_sides_yields_empty_pool() {
        // Cas dégénéré : le gobelet annonce des dés mais ne décrit aucune face.
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut deck = deck_of(5);
        deck.sides.clear();

        let mut pool = DicePool::new(&RunConfig::from_cup(&deck), &deck.sides);

        assert_eq!(pool.len(), 0);
        assert!(pool.is_empty());
        assert!(pool.roll_all(&mut rng, true).is_empty());
    }

    #[test]
    fn test_dice_pool_roundtrip_serde() {
        let mut pool = pool_of(5);
        let ids: Vec<DieId> = pool.dice().iter().map(|die| die.id).collect();
        pool.toggle_lock(ids[0]);
        pool.remove(ids[4]);
        pool.push(Die::new(DieId(0), MAX_DIE_SIDES));

        let encoded = serde_json::to_string(&pool).expect("sérialisation");
        let decoded: DicePool = serde_json::from_str(&encoded).expect("désérialisation");

        assert_eq!(decoded, pool);
    }
}
