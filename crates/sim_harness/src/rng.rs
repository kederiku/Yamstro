//! Le cinquième flux : l'aléa propre à l'instrument.
//!
//! **Il appartient au harnais, et le rendez-vous était pris.** TASK-08 l'a
//! inscrit noir sur blanc dans ce qu'elle n'avait pas le droit de faire :
//! déclarer le générateur du harnais de simulation, « qui ne touche jamais » le
//! générateur de run. Celui-ci **reste à quatre flux** : un cinquième champ
//! ajouté dans le moteur serait une modification du moteur, donc la règle n°1
//! violée.
//!
//! La graine est **celle du run**, la même que celle des quatre autres flux.
//! C'est ce qui rend l'aléa d'une politique reproductible sans le coupler aux
//! dés : deux exécutions de la même graine donnent la même suite de choix.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

/// Le décalage qui sépare l'aléa de l'instrument de celui du jeu.
///
/// **Cinquième constante de flux du projet, et la seule qui vive ici** : ce
/// n'est pas une valeur de jouabilité. Elle est prise parmi les mélangeurs
/// publics, comme les quatre autres, pour sa provenance vérifiable — celui-ci
/// vient de MurmurHash3.
///
/// **Une valeur qui coïnciderait avec l'une des quatre du moteur corrélerait
/// rigoureusement ce flux à celui des dés** : la politique tirerait exactement
/// la séquence qui fabrique les dés qu'elle évalue, et le témoin cesserait
/// d'être indépendant du jeu qu'il mesure — sans la moindre erreur de
/// compilation. Les quatre constantes du moteur ne sont pas publiques : **la
/// non-corrélation ne se prouve donc pas en comparant des constantes, elle se
/// prouve en comparant des tirages.**
const STREAM_SIM_POLICY: u64 = 0xFF51_AFD7_ED55_8CCD;

/// L'aléa du harnais. Un seul flux, celui des politiques.
#[derive(Debug, Clone)]
pub struct SimRng {
    pub policy: ChaCha8Rng,
}

impl SimRng {
    /// Seule fabrique : ni flux par défaut, ni graine implicite.
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        Self {
            policy: ChaCha8Rng::seed_from_u64(seed ^ STREAM_SIM_POLICY),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::rng::RunRng;
    use rand_chacha::rand_core::Rng;

    fn tirages(flux: &ChaCha8Rng) -> Vec<u64> {
        let mut clone = flux.clone();
        (0..8).map(|_| clone.next_u64()).collect()
    }

    #[test]
    fn test_sim_rng_is_seed_derived() {
        // Même graine, mêmes tirages.
        assert_eq!(
            tirages(&SimRng::from_seed(7).policy),
            tirages(&SimRng::from_seed(7).policy)
        );
        assert_ne!(
            tirages(&SimRng::from_seed(7).policy),
            tirages(&SimRng::from_seed(8).policy),
            "deux graines rendent la même suite"
        );

        // **Non corrélé aux quatre flux du jeu, prouvé sur des tirages.**
        let jeu = RunRng::from_seed(7);
        let sim = tirages(&SimRng::from_seed(7).policy);
        for (nom, flux) in [
            ("dés", &jeu.dice),
            ("boutique", &jeu.shop),
            ("boss", &jeu.boss),
            ("effets", &jeu.relic_effects),
        ] {
            assert_ne!(
                sim,
                tirages(flux),
                "le flux des politiques suit celui des {nom}"
            );
        }
    }

    #[test]
    fn test_run_rng_keeps_its_four_streams() {
        // Le compilateur tient l'invariant : un cinquième flux ajouté au
        // générateur de run donnerait `E0027` en nommant le champ oublié.
        let RunRng {
            dice: _,
            shop: _,
            boss: _,
            relic_effects: _,
        } = RunRng::from_seed(1);
    }
}
