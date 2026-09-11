//! Courbe de difficulté : la cible **effective** d'une manche.
//!
//! # Arithmétique normative : aucun flottant
//!
//! La cible est portée en **millièmes de point** dans un `u64`, chaque facteur
//! est un entier en **pour-mille** appliqué par **troncature**, et l'arrondi au
//! plus proche a lieu **une seule fois**, à la conversion finale. Un flottant
//! ici ferait diverger la cible entre x86, ARM et WASM, casserait le partage de
//! graines et rendrait les valeurs imposées approximatives (ADR-003).
//!
//! Intercaler un arrondi entre deux facteurs change les huit valeurs : c'est
//! exactement ce que faisait la courbe de l'Étape 3, qui rendait **5034** et
//! **8054** aux antes 7 et 8 là où celle-ci rend **5033** et **8053**.
//!
//! # Aucun plafond
//!
//! Ni `min`, ni `clamp`, ni « cible maximale raisonnable ». L'empilement
//! maximal est légal et mesuré ; le corriger ici masquerait le signal que
//! l'Étape 6 bis doit recevoir.

use crate::blinds::{BlindModifier, BlindType};
use crate::cups::CupId;

/// Cible de l'ante 1, en millièmes de point.
///
/// **Point de départ, pas résultat d'équilibrage.** `300` et `1,6` sont posés
/// pour que la courbe existe ; seul le harnais de simulation de l'Étape 6 bis
/// est juge des valeurs finales. Ne les ajuste pas pour rendre un empilement
/// jouable.
const BASE_ANTE_1_MILLI: u64 = 300_000;

/// Croissance d'un ante au suivant, en pour-mille : `1,6` s'écrit `1_600`.
///
/// **Point de départ, pas résultat d'équilibrage** — voir ci-dessus.
const ANTE_GROWTH_PERMILLE: u32 = 1_600;

/// Croissance de cible qu'ajoute un stake, en pour-mille.
///
/// Les stakes sont **cumulatifs** : un niveau supérieur porte les contraintes
/// des précédents. Le corpus ne fixe qu'une valeur, celle du Stake 3 (*Mise
/// Verte*), d'où ce seuil unique.
const STAKE_GROWTH_PERMILLE: u32 = 1_150;

/// Premier stake qui majore la cible.
const STAKE_FIRST_GROWING: u8 = 3;

/// Majoration du Gobelet de Fortune, en pour-mille. **Sur les Boss seulement.**
const FORTUNE_BOSS_PERMILLE: u32 = 1_250;

/// Facteur neutre.
const NEUTRAL_PERMILLE: u32 = 1_000;

/// Applique un facteur en pour-mille, **par troncature**.
///
/// Le produit intermédiaire passe par `u128` : à l'empilement maximal, un `u64`
/// déborderait. La conversion de retour est un rétrécissement **muet** — elle
/// ne panique pas —, et c'est un test de valeur exacte qui la garde, non un
/// test de non-panique.
#[inline]
pub(crate) fn apply_permille(value_milli: u64, factor_permille: u32) -> u64 {
    ((u128::from(value_milli) * u128::from(factor_permille)) / 1_000) as u64
}

/// Facteur du rang de la manche.
pub(crate) fn blind_mult_permille(kind: BlindType) -> u32 {
    match kind {
        BlindType::Small => 1_000,
        BlindType::Big => 1_500,
        BlindType::Boss => 2_000,
    }
}

/// Facteur du gobelet. **Le rang n'est pas décoratif** : le Gobelet de Fortune
/// ne majore que les Boss.
pub(crate) fn cup_mult_permille(cup: CupId, kind: BlindType) -> u32 {
    match (cup, kind) {
        (CupId::Fortune, BlindType::Boss) => FORTUNE_BOSS_PERMILLE,
        _ => NEUTRAL_PERMILLE,
    }
}

/// Facteur du stake, accumulé sur `ante − 1` applications.
///
/// **L'accumulation se fait sur le facteur, jamais sur la valeur courante.**
/// Le pour-mille part de `1_000` et reçoit la croissance `ante − 1` fois, avec
/// troncature à chaque pas — `1000, 1150, 1322, 1520, 1748, 2010, 2311, 2657`
/// —, puis il est appliqué **une seule fois** à la cible. Appliquer la
/// croissance directement à la valeur donnerait `7618` au lieu de `7615` dès
/// l'ante 3 : la troncature ne tombe pas au même endroit.
///
/// **Raccord aval.** L'Étape 10 déplacera cette fonction dans
/// `core_engine/src/stakes/`, aux côtés du malus de relance. Ce sera un
/// **déplacement**, jamais une seconde implémentation.
pub(crate) fn stake_mult_permille(stake_level: u8, ante: u8) -> u32 {
    let croissance = if stake_level >= STAKE_FIRST_GROWING {
        STAKE_GROWTH_PERMILLE
    } else {
        NEUTRAL_PERMILLE
    };

    let mut facteur = NEUTRAL_PERMILLE;
    for _ in 1..ante {
        facteur =
            u32::try_from((u64::from(facteur) * u64::from(croissance)) / 1_000).unwrap_or(u32::MAX);
    }
    facteur
}

/// Facteur du boss : celui qu'il porte, ou le neutre.
pub(crate) fn boss_mult_permille(modifier: Option<&BlindModifier>) -> u32 {
    match modifier {
        Some(BlindModifier::TargetMultiplier(v)) => *v,
        _ => NEUTRAL_PERMILLE,
    }
}

/// Cible **effective** d'une manche.
///
/// Les quatre facteurs sont multiplicatifs et s'appliquent **dans cet ordre
/// exact**, en quatre applications distinctes.
///
/// **Le motif habituellement donné à cette règle est faux aujourd'hui.** On lit
/// que regrouper deux facteurs en un produit changerait la troncature : mesuré,
/// non. Les valeurs de rang valent 1000, 1500 ou 2000, celle du gobelet 1000 ou
/// 1250, et **aucune de ces paires ne produit un nombre qui ne divise pas
/// exactement par mille**. Le regroupement est donc exact, et aucun test ne
/// peut l'attraper.
///
/// La règle tient quand même, mais pour une autre raison : elle vaut pour les
/// facteurs **à venir**. Un multiplicateur de boss arbitraire — `TargetMultiplier`
/// porte un `u32` — regroupé avec un autre casserait l'exactitude au premier
/// produit non rond. C'est une garde de CI qui la tient, pas un test.
#[must_use]
pub fn target_score(
    ante: u8,
    kind: BlindType,
    cup: CupId,
    stake_level: u8,
    modifier: Option<&BlindModifier>,
) -> u64 {
    let mut v = BASE_ANTE_1_MILLI;
    for _ in 1..ante {
        v = apply_permille(v, ANTE_GROWTH_PERMILLE);
    }
    v = apply_permille(v, blind_mult_permille(kind));
    v = apply_permille(v, cup_mult_permille(cup, kind));
    v = apply_permille(v, stake_mult_permille(stake_level, ante));
    v = apply_permille(v, boss_mult_permille(modifier));
    (v + 500) / 1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factor_order_matters() {
        // **Permuter les appels, jamais recopier la formule.** Un calcul local
        // qui réécrirait la chaîne dériverait de l'implémentation au premier
        // changement.
        //
        // **Le scénario prescrit ne peut pas le démontrer, et c'est mesuré.**
        // Permuter gobelet et rang, ou stake et gobelet, sur l'empilement
        // maximal donne le **même** résultat aux huit antes : la multiplication
        // est commutative, et l'arrondi final avale les écarts d'une unité que
        // la troncature introduit. Balayé sur 1 152 combinaisons, l'ordre n'est
        // observable qu'en permutant **boss et stake**, et seulement à
        // multiplicateur extrême — trente-cinq cas, tous à ×1000.
        //
        // L'ordre reste une règle : il change la valeur intermédiaire à chaque
        // fois. Ce qui n'est pas vrai, c'est qu'il change toujours le résultat.
        let (ante, kind, cup, stake) = (5u8, BlindType::Small, CupId::Standard, 3u8);
        let extreme = BlindModifier::TargetMultiplier(1_000_000);

        let mut permute = BASE_ANTE_1_MILLI;
        for _ in 1..ante {
            permute = apply_permille(permute, ANTE_GROWTH_PERMILLE);
        }
        permute = apply_permille(permute, blind_mult_permille(kind));
        permute = apply_permille(permute, cup_mult_permille(cup, kind));
        // Boss avant stake : les deux mêmes facteurs, l'ordre inverse.
        permute = apply_permille(permute, boss_mult_permille(Some(&extreme)));
        permute = apply_permille(permute, stake_mult_permille(stake, ante));
        let permute = (permute + 500) / 1_000;

        let officiel = target_score(ante, kind, cup, stake, Some(&extreme));
        assert_eq!(officiel, 3_436_707);
        assert_eq!(permute, 3_436_708, "la permutation ne décale plus rien");
        assert_ne!(permute, officiel);
    }

    #[test]
    fn test_factor_order_is_invisible_on_the_named_scenario() {
        // La contre-épreuve du test précédent, et la mesure qu'elle porte :
        // sur l'empilement que le ticket nomme, permuter gobelet et rang ne
        // change **rien**, à aucun ante. Écrire le test d'ordre sur ce
        // scénario-là aurait donné un rouge permanent.
        let mur = BlindModifier::TargetMultiplier(3_000);
        for ante in 1..=8u8 {
            let mut permute = BASE_ANTE_1_MILLI;
            for _ in 1..ante {
                permute = apply_permille(permute, ANTE_GROWTH_PERMILLE);
            }
            permute = apply_permille(permute, cup_mult_permille(CupId::Fortune, BlindType::Boss));
            permute = apply_permille(permute, blind_mult_permille(BlindType::Boss));
            permute = apply_permille(permute, stake_mult_permille(3, ante));
            permute = apply_permille(permute, boss_mult_permille(Some(&mur)));

            assert_eq!(
                (permute + 500) / 1_000,
                target_score(ante, BlindType::Boss, CupId::Fortune, 3, Some(&mur)),
                "ante {ante}"
            );
        }
    }

    #[test]
    fn test_no_overflow_at_ante_eight() {
        // **`as u64` ne panique jamais**, il tronque en silence : une assertion
        // de non-panique serait vraie par construction et ne mesurerait rien.
        // Ce qui se garde, c'est la **valeur exacte** au maximum — elle attrape
        // aussi bien une panique qu'un rétrécissement muet.
        let enorme = BlindModifier::TargetMultiplier(u32::MAX);
        let cible = target_score(8, BlindType::Boss, CupId::Fortune, 6, Some(&enorme));

        // 53 492 464 millièmes avant le facteur de boss, puis x 4 294 967 295,
        // puis l'arrondi final : le produit intermédiaire dépasse un u64, pas
        // le résultat.
        assert_eq!(cible, 229_748_383_409);
        assert!(cible < u64::MAX / 1_000, "la marge de tête reste large");
    }

    #[test]
    fn test_stake_factor_accumulates_on_itself() {
        // Les huit facteurs du Stake 3, tronqués à chaque pas. C'est la seule
        // façon de distinguer l'accumulation sur le facteur de l'accumulation
        // sur la valeur, que le résultat final confond aux deux premiers antes.
        let facteurs: Vec<u32> = (1..=8).map(|ante| stake_mult_permille(3, ante)).collect();
        assert_eq!(
            facteurs,
            vec![1000, 1150, 1322, 1520, 1748, 2010, 2311, 2657]
        );

        // En dessous du seuil, le stake est neutre à tous les antes.
        for ante in 1..=8 {
            assert_eq!(stake_mult_permille(2, ante), 1_000, "ante {ante}");
        }
    }
}
