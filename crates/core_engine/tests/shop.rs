//! Étalage de boutique : composition, pondération, déterminisme.
//!
//! Tests **purs**, sans ECS et sans Bevy. Les flux se construisent depuis une
//! graine maîtresse par `RunRng`, comme en jeu : un `ChaCha8Rng` fabriqué à la
//! main éprouverait un flux que la partie n'emploie jamais.

use core_engine::consumables::ConsumableId;
use core_engine::relics::{CATALOG, RelicRarity, rarity_of};
use core_engine::rng::RunRng;
use core_engine::shop::generator::{
    RARITY_ORDER, RARITY_WEIGHTS_PERMILLE, generate_shop, rarity_for_permille, relic_pool,
};
use core_engine::shop::{INITIAL_REROLL_COST, ShopItem};

/// L'étalage d'une graine, tel que la partie le produirait.
fn etalage(graine: u64) -> Vec<ShopItem> {
    let mut rng = RunRng::from_seed(graine);
    generate_shop(&mut rng.shop).items
}

#[test]
fn test_rarity_weights() {
    assert_eq!(RARITY_WEIGHTS_PERMILLE, [650, 250, 90, 10]);
    assert_eq!(RARITY_WEIGHTS_PERMILLE.iter().sum::<u32>(), 1_000);
    assert_eq!(
        RARITY_ORDER,
        [
            RelicRarity::Common,
            RelicRarity::Uncommon,
            RelicRarity::Rare,
            RelicRarity::Legendary,
        ],
        "les poids suivent l'ordre de déclaration des variantes"
    );

    // **Les frontières, valeur par valeur.** Sans elles, le test n'assère que
    // les constantes : un cumul décalé d'un pour-mille rendrait d'autres
    // raretés sans qu'aucune assertion ne bouge.
    for (seuil, attendu) in [
        (0, RelicRarity::Common),
        (649, RelicRarity::Common),
        (650, RelicRarity::Uncommon),
        (899, RelicRarity::Uncommon),
        (900, RelicRarity::Rare),
        (989, RelicRarity::Rare),
        (990, RelicRarity::Legendary),
        (999, RelicRarity::Legendary),
    ] {
        assert_eq!(rarity_for_permille(seuil), attendu, "seuil {seuil}");
    }

    // Reproductible à l'identique : deux générations de même graine.
    assert_eq!(etalage(42), etalage(42));
}

#[test]
fn test_shop_has_four_items_in_expected_composition() {
    for graine in 0..64 {
        let items = etalage(graine);
        assert_eq!(items.len(), 4, "graine {graine}");

        assert!(
            matches!(items[0], ShopItem::RelicCard(_))
                && matches!(items[1], ShopItem::RelicCard(_)),
            "graine {graine} : les deux premiers sont des reliques"
        );
        assert!(
            matches!(items[2], ShopItem::GridUpgrade(_)),
            "graine {graine} : le troisième est un parchemin"
        );
        assert!(
            matches!(items[3], ShopItem::DieMod(_) | ShopItem::Consumable(_)),
            "graine {graine} : le quatrième est un dé modifié ou un consommable"
        );
    }

    // Les deux branches du quatrième article sortent bien toutes les deux :
    // sans cela une branche morte passerait inaperçue.
    let quatriemes: Vec<ShopItem> = (0..64).map(|g| etalage(g)[3].clone()).collect();
    assert!(quatriemes.iter().any(|i| matches!(i, ShopItem::DieMod(_))));
    assert!(
        quatriemes
            .iter()
            .any(|i| matches!(i, ShopItem::Consumable(_)))
    );
}

#[test]
fn test_same_seed_same_item_sequence() {
    // Position par position, et non le même multi-ensemble : deux étalages de
    // même contenu dans un autre ordre ne sont pas le même étalage.
    for graine in [0, 1, 7, 99, 1_000] {
        assert_eq!(etalage(graine), etalage(graine), "graine {graine}");
    }

    // Contre-épreuve : deux graines donnent des étalages différents, sans quoi
    // le test passerait sur un générateur constant. Comparaison par égalité :
    // `ShopItem` n'est pas ordonné, et lui ajouter `Ord` pour la commodité d'un
    // test étendrait un type que le ticket fixe.
    let premier = etalage(0);
    assert!(
        (1..32).any(|g| etalage(g) != premier),
        "le générateur rend toujours le même étalage"
    );
}

#[test]
fn test_generation_does_not_consume_dice_stream() {
    use rand::RngExt;

    let mut rng = RunRng::from_seed(5);
    let mut temoin = rng.dice.clone();
    let mut temoin_boss = rng.boss.clone();

    generate_shop(&mut rng.shop);

    // Les trois autres flux n'ont pas bougé d'un cran.
    assert_eq!(
        rng.dice.random_range(0..u32::MAX),
        temoin.random_range(0..u32::MAX)
    );
    assert_eq!(
        rng.boss.random_range(0..u32::MAX),
        temoin_boss.random_range(0..u32::MAX)
    );
}

#[test]
fn test_rune_of_fate_never_duplicated() {
    // **Vrai par construction aujourd'hui**, et c'est pourquoi le mécanisme est
    // asséré avec la conséquence : un seul emplacement de consommable, et un
    // seul identifiant déclaré. Le jour où l'Étape 9 ajoute une rune ou un
    // second emplacement, c'est ici que la question se repose.
    assert_eq!(
        ConsumableId::ALL.len(),
        1,
        "le catalogue des consommables a grandi"
    );

    for graine in 0..256 {
        let runes = etalage(graine)
            .iter()
            .filter(|i| matches!(i, ShopItem::Consumable(ConsumableId::RuneOfFate)))
            .count();
        assert!(runes <= 1, "graine {graine} : {runes} Runes du Destin");
    }
}

#[test]
fn test_rarity_fallback_is_deterministic() {
    // Le repli est une **règle**, pas un détail : il s'éprouve directement.
    // Le forcer depuis `generate_shop` est impossible, le palier Légendaire
    // sortant à un pour cent et la fonction ne prenant qu'un flux.
    assert!(
        relic_pool(RelicRarity::Legendary)
            .iter()
            .all(|d| rarity_of(*d) == RelicRarity::Rare),
        "le vivier Légendaire doit se replier sur les Rares"
    );
    assert_eq!(
        relic_pool(RelicRarity::Legendary),
        relic_pool(RelicRarity::Rare),
        "un palier, et un seul, à la fois"
    );

    for rarete in RARITY_ORDER {
        let vivier = relic_pool(rarete);
        assert!(!vivier.is_empty(), "{rarete:?} : vivier vide");
        assert_eq!(vivier, relic_pool(rarete), "{rarete:?} : non déterministe");
    }

    // Les trois paliers peuplés ne se replient pas.
    for rarete in [
        RelicRarity::Common,
        RelicRarity::Uncommon,
        RelicRarity::Rare,
    ] {
        assert!(
            relic_pool(rarete).iter().all(|d| rarity_of(*d) == rarete),
            "{rarete:?} s'est replié alors que son vivier est peuplé"
        );
    }
}

#[test]
fn test_no_legendary_relic_is_offered() {
    // Le mécanisme, puis la conséquence. Sans la première assertion, le jour où
    // l'Étape 9 ajoute une Légendaire au catalogue, ce test resterait vert sur
    // un repli devenu faux.
    assert!(
        !CATALOG
            .iter()
            .any(|d| rarity_of(*d) == RelicRarity::Legendary),
        "le catalogue porte une Légendaire : le repli est à revoir"
    );

    for graine in 0..256 {
        for item in etalage(graine) {
            if let ShopItem::RelicCard(def) = item {
                assert_ne!(rarity_of(def), RelicRarity::Legendary, "graine {graine}");
            }
        }
    }
}

#[test]
fn test_reroll_cost_starts_at_its_constant() {
    // `generate_shop` pose le coût à chaque appel : la remise à zéro par visite
    // est vraie par construction, et non par un système qui y penserait.
    // La valeur est épinglée : comparer la sortie à la constante seule est une
    // tautologie, les deux bougeant ensemble.
    assert_eq!(INITIAL_REROLL_COST, 5);

    let mut rng = RunRng::from_seed(3);
    assert_eq!(
        generate_shop(&mut rng.shop).reroll_cost,
        INITIAL_REROLL_COST
    );
}
