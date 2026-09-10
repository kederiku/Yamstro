//! Le catalogue de reliques **vu du dehors**.
//!
//! Ces tests compilent `core_engine` **sans `cfg(test)`** : les trois fixtures
//! `SixFire`, `MagicPair` et `BrokenGlass` n'y existent pas. C'est la seule
//! façon de prouver qu'elles ne franchissent pas la frontière de crate, et cela
//! rend « douze variantes » littéralement vrai plutôt que « quinze dont trois
//! filtrées ».
//!
//! **Règle du dépôt, posée ici :** un test qui doit voir la crate *comme un
//! consommateur* vit dans `tests/` ; tout le reste vit en `#[cfg(test)] mod
//! tests` dans `src/`, au contact de ce qu'il éprouve.

use core_engine::relics::{CATALOG, RelicId, RelicRarity, rarity_of};

#[test]
fn test_catalog_has_twelve_entries() {
    assert_eq!(CATALOG.len(), 12);
}

#[test]
fn test_catalog_has_no_duplicates() {
    for (position, left) in CATALOG.iter().enumerate() {
        for right in &CATALOG[position + 1..] {
            assert_ne!(left, right, "doublon dans le catalogue : {left:?}");
        }
    }
}

#[test]
fn test_rarity_of_is_total() {
    // La totalité est garantie par l'exhaustivité du `match` : retirer un bras
    // produit E0004 et rien ne compile. Ce test n'en est que le témoin
    // d'exécution — il ne peut pas échouer là où le compilateur a déjà parlé.
    for def in CATALOG {
        let _ = rarity_of(*def);
    }
}

#[test]
fn test_rarity_distribution_is_four_four_four() {
    let compte = |cible: RelicRarity| CATALOG.iter().filter(|d| rarity_of(**d) == cible).count();
    assert_eq!(compte(RelicRarity::Common), 4, "Communes");
    assert_eq!(compte(RelicRarity::Uncommon), 4, "Peu communes");
    assert_eq!(compte(RelicRarity::Rare), 4, "Rares");
    assert_eq!(compte(RelicRarity::Legendary), 0, "Légendaires");
}

#[test]
fn test_relic_id_serde_roundtrip() {
    for def in CATALOG {
        let encode = serde_json::to_string(def).expect("sérialisation");
        let decode: RelicId = serde_json::from_str(&encode).expect("désérialisation");
        assert_eq!(*def, decode);
    }
}

#[test]
fn test_relic_rarity_serde_roundtrip() {
    // **Les quatre**, `Legendary` comprise : elle est déclarée sans porteur, et
    // une sauvegarde de l'Étape 10 devra la relire le jour où l'Étape 9 lui en
    // donnera un.
    for rarete in [
        RelicRarity::Common,
        RelicRarity::Uncommon,
        RelicRarity::Rare,
        RelicRarity::Legendary,
    ] {
        let encode = serde_json::to_string(&rarete).expect("sérialisation");
        let decode: RelicRarity = serde_json::from_str(&encode).expect("désérialisation");
        assert_eq!(rarete, decode);
    }
}
