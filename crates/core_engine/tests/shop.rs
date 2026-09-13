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

// ---- Tarification, revente et coût de relance (TASK-77) ----

use core_engine::dice::DieModifier;
use core_engine::hands::YahtzeeHand;
use core_engine::relics::RelicId;
use core_engine::shop::pricing::{REROLL_COST_STEP, bump_reroll_cost, price_of, sell_value};

#[test]
fn test_reroll_cost_increments() {
    // Une visite : on paie le coût courant, **puis** on l'incrémente.
    let mut rng = RunRng::from_seed(1);
    let mut etalage = generate_shop(&mut rng.shop);

    let mut payes = Vec::new();
    for _ in 0..3 {
        payes.push(etalage.reroll_cost);
        etalage.reroll_cost = bump_reroll_cost(etalage.reroll_cost);
    }
    assert_eq!(payes, vec![5, 6, 7]);
    assert_eq!(
        etalage.reroll_cost, 8,
        "le champ porte le coût de la prochaine"
    );

    // **La visite suivante repart à cinq**, et personne ne remet rien à zéro :
    // `generate_shop` pose la constante à chaque appel.
    let suivante = generate_shop(&mut rng.shop);
    assert_eq!(suivante.reroll_cost, INITIAL_REROLL_COST);
    assert_eq!(suivante.reroll_cost, 5);
}

#[test]
fn test_sell_value_floor_is_one() {
    // Troncature vers zéro, jamais d'arrondi au plus proche.
    assert_eq!(sell_value(7), 3, "on tronque");
    assert_eq!(sell_value(10), 5);
    assert_eq!(sell_value(3), 1);
    // Le plancher : sans lui, `1 / 2` rendrait zéro et vendre coûterait de l'or.
    assert_eq!(sell_value(2), 1);
    assert_eq!(sell_value(1), 1);
}

#[test]
fn test_sell_value_of_free_item_is_zero() {
    // Le plancher protège une revente réelle, il ne fabrique pas d'or. Sans
    // cette clause, le parchemin à zéro du `BlueSeal` (Étape 9) se revendrait
    // un dollar tiré de nulle part.
    assert_eq!(sell_value(0), 0);
}

#[test]
fn test_price_is_total_and_deterministic() {
    let articles = [
        ShopItem::RelicCard(RelicId::CrackedDie),
        ShopItem::GridUpgrade(YahtzeeHand::Chance),
        ShopItem::DieMod(DieModifier::BonusChips(30)),
        ShopItem::Consumable(ConsumableId::RuneOfFate),
    ];

    for article in &articles {
        assert!(price_of(article) > 0, "{article:?} est gratuit");
        assert_eq!(price_of(article), price_of(article), "{article:?} varie");
    }

    // Les quatre paliers de la table, par des articles qui les empruntent.
    assert_eq!(price_of(&ShopItem::GridUpgrade(YahtzeeHand::Chance)), 4);
    assert_eq!(price_of(&ShopItem::DieMod(DieModifier::BonusMult(100))), 6);
    // Une Commune, une Rare : la relique suit sa rareté.
    assert_eq!(price_of(&ShopItem::RelicCard(RelicId::CrackedDie)), 4);
    assert_eq!(price_of(&ShopItem::RelicCard(RelicId::DoubleMirror)), 8);

    // **Les trois magnitudes de dé coûtent le même prix.** Écart assumé : elles
    // empruntent un palier, et ni les magnitudes ni ce prix ne sont chiffrés
    // par le corpus. À calibrer par l'Étape 6 bis, avec les magnitudes.
    assert_eq!(
        price_of(&ShopItem::DieMod(DieModifier::BonusChips(30))),
        price_of(&ShopItem::DieMod(DieModifier::BonusChips(50)))
    );
}

#[test]
fn test_rune_of_fate_costs_eight() {
    // Huit, **par le palier Rare** et non par un cas particulier : la seconde
    // assertion tombe si quelqu'un code le chiffre en dur.
    assert_eq!(price_of(&ShopItem::Consumable(ConsumableId::RuneOfFate)), 8);
    assert_eq!(
        core_engine::consumables::rarity_of(ConsumableId::RuneOfFate),
        RelicRarity::Rare
    );
    assert_eq!(
        price_of(&ShopItem::Consumable(ConsumableId::RuneOfFate)),
        price_of(&ShopItem::RelicCard(RelicId::DoubleMirror)),
        "la Rune du Destin et une relique Rare tombent du même palier"
    );
}

#[test]
fn test_reroll_cost_does_not_overflow() {
    assert_eq!(REROLL_COST_STEP, 1);

    let mut cout = INITIAL_REROLL_COST;
    for _ in 0..50 {
        cout = bump_reroll_cost(cout);
    }
    assert_eq!(cout, 55);

    assert_eq!(
        bump_reroll_cost(u32::MAX),
        u32::MAX,
        "aucune panique au plafond"
    );
}

#[test]
fn test_sell_value_never_exceeds_price() {
    for prix in [1, 2, 3, 4, 6, 8, 10, 17, 1_000, u32::MAX] {
        assert!(sell_value(prix) <= prix, "prix {prix}");
    }
    // Et toute la table, par ses articles.
    for article in [
        ShopItem::RelicCard(RelicId::CrackedDie),
        ShopItem::GridUpgrade(YahtzeeHand::Aces),
        ShopItem::DieMod(DieModifier::BonusChips(30)),
        ShopItem::Consumable(ConsumableId::RuneOfFate),
    ] {
        let prix = price_of(&article);
        assert!(sell_value(prix) <= prix, "{article:?}");
        assert!(sell_value(prix) >= 1, "{article:?} se revend zéro");
    }
}
