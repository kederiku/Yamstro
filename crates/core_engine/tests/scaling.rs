//! La courbe de difficulté, **vue du dehors**.
//!
//! Ces trois tests passent par `target_score` et rien d'autre : c'est la seule
//! surface publique de la courbe, et la seule que l'Étape 6 bis calibrera.

use core_engine::blinds::{BlindModifier, BlindType, target_score};
use core_engine::cups::CupId;

/// Les huit antes, dans l'ordre.
const ANTES: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

fn courbe(kind: BlindType, cup: CupId, stake: u8, modifier: Option<&BlindModifier>) -> Vec<u64> {
    ANTES
        .into_iter()
        .map(|ante| target_score(ante, kind, cup, stake, modifier))
        .collect()
}

#[test]
fn test_base_target_progression() {
    assert_eq!(
        courbe(BlindType::Small, CupId::Standard, 1, None),
        vec![300, 480, 768, 1229, 1966, 3146, 5033, 8053]
    );

    // **Deux de ces valeurs changent par rapport à la courbe de l'Étape 3.**
    // Celle-ci arrondissait à chaque étage et travaillait en points entiers ;
    // elle rendait 5034 et 8054. La forme normative tronque à chaque facteur et
    // n'arrondit qu'une fois, à la fin.
    for (kind, attendu) in [
        (BlindType::Small, 8053),
        (BlindType::Big, 12080),
        (BlindType::Boss, 16106),
    ] {
        assert_eq!(target_score(8, kind, CupId::Standard, 1, None), attendu);
    }
}

#[test]
fn test_stacked_scaling_wall_fortune_stake3() {
    // **Signal de calibrage n° 1, remis à l'Étape 6 bis.** Le rapport à la Boss
    // neutre passe de ×3,75 à l'ante 1 à ×9,96 à l'ante 8, parce que **seul le
    // facteur de stake compose avec l'ante**. La combinaison est légale,
    // reproductible, et probablement injouable en l'état.
    //
    // Ce n'est pas un défaut à corriger ici : aucun plafond, aucun garde-fou,
    // aucun ajustement des deux constantes. Le harnais de simulation est seul
    // juge, et ce test est la mesure qu'on lui remet.
    let mur = BlindModifier::TargetMultiplier(3_000);
    assert_eq!(
        courbe(BlindType::Boss, CupId::Fortune, 3, Some(&mur)),
        vec![2250, 4140, 7615, 14008, 25775, 47422, 87237, 160477]
    );

    let neutre = courbe(BlindType::Boss, CupId::Standard, 1, None);
    assert_eq!(neutre[0], 600, "boss neutre à l'ante 1");
    assert_eq!(neutre[7], 16106, "boss neutre à l'ante 8");
}

#[test]
fn test_cup_mult_applies_to_boss_only() {
    // Le second paramètre de `cup_mult_permille` n'est pas décoratif : le
    // Gobelet de Fortune ne majore que les Boss.
    for kind in [BlindType::Small, BlindType::Big] {
        assert_eq!(
            courbe(kind, CupId::Fortune, 1, None),
            courbe(kind, CupId::Standard, 1, None),
            "{kind:?} a bougé sous le Gobelet de Fortune"
        );
    }

    let fortune = courbe(BlindType::Boss, CupId::Fortune, 1, None);
    let neutre = courbe(BlindType::Boss, CupId::Standard, 1, None);
    assert_ne!(fortune, neutre);
    assert_eq!(fortune[0], 750, "600 majoré d'un quart");
}
