//! Déterminisme : RunRng, quatre flux ChaCha8Rng.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

// Constantes de séparation des flux, mêlées à la graine maîtresse par un « ou
// exclusif ». Ce ne sont pas des valeurs de gameplay.
//
// Leur magnitude compte autant que leur distinction. La dérivation étant
// `seed ^ CONSTANTE`, deux constantes petites font coïncider les flux de deux
// rôles pour des graines voisines : avec 0 et 1, `from_seed(1).dice` et
// `from_seed(0).shop` rendraient exactement la même séquence, et la graine
// maîtresse est justement ce que le joueur saisit et partage. Les quatre
// valeurs retenues sont les mélangeurs publics de splitmix64, préférés à des
// constantes inventées pour leur provenance vérifiable.
const STREAM_DICE: u64 = 0x9E37_79B9_7F4A_7C15;
const STREAM_SHOP: u64 = 0xBF58_476D_1CE4_E5B9;
const STREAM_BOSS: u64 = 0x94D0_49BB_1331_11EB;
const STREAM_RELIC_EFFECTS: u64 = 0xD6E8_FEB8_6659_FD93;

/// Les quatre flux aléatoires d'une run, indépendants et sérialisables.
///
/// Une consommation dans un flux n'affecte aucun des trois autres : relancer
/// l'étalage de la boutique ne décale pas la séquence des dés. Sans cette
/// séparation, deux joueurs partis de la même graine divergeraient dès qu'ils
/// cliquent un nombre différent de fois sur Relancer.
///
/// Aucun dérivé Bevy : `ChaCha8Rng` n'implémente pas `Reflect`. Aucun
/// `PartialEq` non plus, l'égalité d'état interne n'étant pas le contrat à
/// vérifier ; ce sont les tirages qui doivent coïncider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunRng {
    pub dice: ChaCha8Rng,
    pub shop: ChaCha8Rng,
    pub boss: ChaCha8Rng,
    pub relic_effects: ChaCha8Rng,
}

impl RunRng {
    /// Dérive les quatre flux d'une graine maîtresse, affichable et partageable
    /// par le joueur. Seul point de création : ni flux par défaut, ni graine
    /// implicite.
    pub fn from_seed(seed: u64) -> Self {
        Self {
            dice: ChaCha8Rng::seed_from_u64(seed ^ STREAM_DICE),
            shop: ChaCha8Rng::seed_from_u64(seed ^ STREAM_SHOP),
            boss: ChaCha8Rng::seed_from_u64(seed ^ STREAM_BOSS),
            relic_effects: ChaCha8Rng::seed_from_u64(seed ^ STREAM_RELIC_EFFECTS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, RngExt};
    use rand_chacha::ChaCha8Rng;

    /// Avance un flux sans regarder les valeurs.
    fn consume(rng: &mut ChaCha8Rng, count: usize) {
        for _ in 0..count {
            rng.next_u64();
        }
    }

    /// Tirages bruts, assez longs pour qu'une coïncidence soit impossible.
    fn draws(rng: &mut ChaCha8Rng, count: usize) -> Vec<u64> {
        (0..count).map(|_| rng.next_u64()).collect()
    }

    /// Une main de cinq dés à six faces.
    fn hand(rng: &mut ChaCha8Rng) -> Vec<u8> {
        (0..5).map(|_| rng.random_range(1..=6u8)).collect()
    }

    #[test]
    fn test_rng_streams_are_independent() {
        let mut reference = RunRng::from_seed(7);
        let mut consumed = RunRng::from_seed(7);

        consume(&mut consumed.shop, 100);

        assert_eq!(hand(&mut reference.dice), hand(&mut consumed.dice));
    }

    #[test]
    fn test_rng_roundtrip_serde() {
        let mut original = RunRng::from_seed(1234);
        // Consommation partielle et inégale : l'état des quatre flux diffère.
        consume(&mut original.dice, 3);
        consume(&mut original.shop, 7);
        consume(&mut original.boss, 1);
        consume(&mut original.relic_effects, 11);

        let encoded = serde_json::to_string(&original).expect("sérialisation");
        let mut decoded: RunRng = serde_json::from_str(&encoded).expect("désérialisation");

        assert_eq!(draws(&mut original.dice, 5), draws(&mut decoded.dice, 5));
        assert_eq!(draws(&mut original.shop, 5), draws(&mut decoded.shop, 5));
        assert_eq!(draws(&mut original.boss, 5), draws(&mut decoded.boss, 5));
        assert_eq!(
            draws(&mut original.relic_effects, 5),
            draws(&mut decoded.relic_effects, 5)
        );
    }

    #[test]
    fn test_same_seed_same_rolls() {
        let mut left = RunRng::from_seed(42);
        let mut right = RunRng::from_seed(42);

        assert_eq!(hand(&mut left.dice), hand(&mut right.dice));
    }

    #[test]
    fn test_different_seeds_differ() {
        let mut one = RunRng::from_seed(1);
        let mut two = RunRng::from_seed(2);

        assert_ne!(draws(&mut one.dice, 20), draws(&mut two.dice, 20));
    }

    #[test]
    fn test_stream_constants_are_distinct() {
        let constants = [STREAM_DICE, STREAM_SHOP, STREAM_BOSS, STREAM_RELIC_EFFECTS];

        for (index, left) in constants.iter().enumerate() {
            for right in constants.iter().skip(index + 1) {
                assert_ne!(left, right);
            }
        }
    }

    #[test]
    fn test_streams_differ_within_one_rng() {
        let mut run = RunRng::from_seed(9);
        let sequences = [
            draws(&mut run.dice, 20),
            draws(&mut run.shop, 20),
            draws(&mut run.boss, 20),
            draws(&mut run.relic_effects, 20),
        ];

        for (index, left) in sequences.iter().enumerate() {
            for right in sequences.iter().skip(index + 1) {
                assert_ne!(left, right);
            }
        }
    }

    #[test]
    fn test_streams_do_not_collide_across_seeds() {
        // Verrouille la magnitude des constantes. Avec des valeurs ordinales
        // 0 à 3, `from_seed(1).dice` et `from_seed(0).shop` rendraient la même
        // séquence, puisque 1 ^ 0 vaut 0 ^ 1. Ce test échouerait alors.
        let mut sequences = Vec::new();
        for seed in 0..4_u64 {
            let mut run = RunRng::from_seed(seed);
            sequences.push(draws(&mut run.dice, 8));
            sequences.push(draws(&mut run.shop, 8));
            sequences.push(draws(&mut run.boss, 8));
            sequences.push(draws(&mut run.relic_effects, 8));
        }

        for (index, left) in sequences.iter().enumerate() {
            for right in sequences.iter().skip(index + 1) {
                assert_ne!(left, right, "collision de flux");
            }
        }
    }
}
