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

// ---- L'inventaire, vu du dehors (TASK-54) ----

use core_engine::config::RunConfig;
use core_engine::cups::CupId;
use core_engine::cups::definitions::cup;
use core_engine::relics::{RelicInstance, RelicInventory, RelicState};

fn config(id: CupId) -> RunConfig {
    RunConfig::from_cup(&cup(id))
}

/// Inventaire rempli de reliques distinctes, dans l'ordre du catalogue.
fn rempli(capacity: u8) -> RelicInventory {
    let mut inventaire = RelicInventory::new(capacity);
    for def in CATALOG.iter().take(capacity as usize) {
        inventaire.add_relic(*def).expect("slot libre");
    }
    inventaire
}

fn defs(inventaire: &RelicInventory) -> Vec<Option<RelicId>> {
    inventaire
        .slots
        .iter()
        .map(|slot| slot.map(|relique| relique.def))
        .collect()
}

#[test]
fn test_relic_inventory_serde_roundtrip() {
    // **Les quatre variantes de `RelicState`**, plus un slot vide et un
    // `next_uid` non nul. C'est ce test qui interdit mécaniquement le retour
    // d'un objet-trait boxé ou d'une table associative à clés textuelles : ni
    // l'un ni l'autre ne survit à un aller-retour serde sans bibliothèque
    // supplémentaire, et la sauvegarde de l'Étape 10 sérialisera `RunSession`
    // en entier. Les noms de ces deux formes sont eux-mêmes proscrits par le
    // volet 1, d'où cette périphrase.
    //
    // Écrit depuis `tests/`, il ne peut nommer que des reliques de production :
    // un inventaire sérialisé depuis l'extérieur ne peut structurellement pas
    // contenir de fixture.
    let mut inventaire = RelicInventory::new(5);
    inventaire.slots = vec![
        Some(RelicInstance {
            uid: 1,
            def: RelicId::CrackedDie,
            state: RelicState::None,
        }),
        Some(RelicInstance {
            uid: 2,
            def: RelicId::PolishedStone,
            state: RelicState::Counter(7),
        }),
        None,
        Some(RelicInstance {
            uid: 3,
            def: RelicId::Pendulum,
            state: RelicState::Perishable { rounds_left: 2 },
        }),
        Some(RelicInstance {
            uid: 4,
            def: RelicId::GhostDie,
            state: RelicState::Disabled,
        }),
    ];
    inventaire.next_uid = 42;

    let json = serde_json::to_string(&inventaire).expect("sérialisation");
    let retour: RelicInventory = serde_json::from_str(&json).expect("désérialisation");
    assert_eq!(inventaire, retour);
    assert_eq!(retour.next_uid, 42, "le compteur n'a pas survécu");
}

#[test]
fn test_capacity_from_run_config() {
    let standard = config(CupId::Standard);
    let fortune = config(CupId::Fortune);

    // Un seul littéral dans tout le test, pour épingler le gobelet standard ;
    // l'écart s'écrit en termes de configuration, jamais en littéraux.
    assert_eq!(standard.relic_capacity, 5);
    assert_eq!(fortune.relic_capacity, standard.relic_capacity + 1);

    for conf in [standard, fortune] {
        let mut inventaire = RelicInventory::new(conf.relic_capacity);
        for _ in 0..conf.relic_capacity {
            assert!(
                inventaire.add_relic(RelicId::CrackedDie).is_some(),
                "un ajout sous la capacité a été refusé"
            );
        }
        assert_eq!(inventaire.len(), usize::from(conf.relic_capacity));
        assert_eq!(
            inventaire.add_relic(RelicId::CrackedDie),
            None,
            "un ajout au-delà de la capacité a été accepté"
        );
    }
}

#[test]
fn test_reorder_shifts_intermediate_slots() {
    let mut inventaire = rempli(5);
    let avant = defs(&inventaire);
    inventaire.reorder(0, 2);

    // Décalage, jamais échange : A passe en 2, B et C glissent d'un cran,
    // D et E ne bougent pas.
    assert_eq!(
        defs(&inventaire),
        vec![avant[1], avant[2], avant[0], avant[3], avant[4]]
    );
    assert_eq!(inventaire.capacity(), 5, "la capacité a changé");
}

#[test]
fn test_reorder_out_of_bounds_is_noop() {
    let mut inventaire = rempli(5);
    let avant = inventaire.clone();

    inventaire.reorder(0, 9);
    inventaire.reorder(9, 0);
    inventaire.reorder(0, 0);

    assert_eq!(
        inventaire, avant,
        "un appel hors bornes a modifié l'inventaire"
    );
}

#[test]
fn test_two_adds_of_same_def_get_distinct_uids() {
    let mut inventaire = RelicInventory::new(5);
    assert_eq!(
        inventaire.next_uid, 0,
        "un inventaire neuf part d'un compteur nul"
    );
    let premier = inventaire
        .add_relic(RelicId::CrackedDie)
        .expect("slot libre");
    let second = inventaire
        .add_relic(RelicId::CrackedDie)
        .expect("slot libre");

    assert_ne!(premier, second, "deux copies partagent un identifiant");
    let copies: Vec<&RelicInstance> = inventaire
        .iter_slots()
        .map(|(_, relique)| relique)
        .filter(|relique| relique.def == RelicId::CrackedDie)
        .collect();
    assert_eq!(copies.len(), 2);
    assert_ne!(copies[0].uid, copies[1].uid);
}

#[test]
fn test_removed_uid_is_never_reused() {
    let mut inventaire = RelicInventory::new(5);
    let uids: Vec<u32> = (0..3)
        .map(|_| {
            inventaire
                .add_relic(RelicId::CrackedDie)
                .expect("slot libre")
        })
        .collect();

    let avant = defs(&inventaire);
    let retiree = inventaire.remove_relic(1).expect("relique présente");
    assert_eq!(retiree.uid, uids[1]);

    // **Le slot est vidé, il n'est pas supprimé.** Un `Vec::remove` décalerait
    // toutes les reliques suivantes d'un cran : l'ordre des slots **est**
    // l'ordre d'application (ADR-005), donc le score du joueur changerait sans
    // qu'il ait rien demandé. Le banc a montré qu'aucun test ne le voyait.
    assert_eq!(inventaire.capacity(), 5, "la capacité a changé");
    assert_eq!(
        defs(&inventaire),
        vec![avant[0], None, avant[2], avant[3], avant[4]],
        "les reliques suivantes ont glissé"
    );

    let neuf = inventaire
        .add_relic(RelicId::PolishedStone)
        .expect("slot libre");
    assert!(
        uids.iter().all(|ancien| neuf > *ancien),
        "un identifiant libéré a été réattribué : {neuf} après {uids:?}"
    );
}

#[test]
fn test_full_inventory_add_returns_none_and_burns_no_uid() {
    let mut inventaire = rempli(5);
    let compteur = inventaire.next_uid;
    let avant = inventaire.clone();

    assert_eq!(inventaire.add_relic(RelicId::DoubleMirror), None);
    assert_eq!(
        inventaire.next_uid, compteur,
        "un refus a brûlé un identifiant"
    );
    assert_eq!(inventaire, avant, "un refus a modifié l'inventaire");
}

// ---- Squelette des quatre comportements (TASK-56) ----

use core_engine::blind::{BlindContext, BlindDefinition};
use core_engine::dice::{Die, DieId};
use core_engine::evaluator::HandMatch;
use core_engine::hands::{HandGrid, HandLevels, YahtzeeHand};
use core_engine::relics::effects::{advance_state, effects_for, gold_for, roll_modifier_for};
use core_engine::scoring::{Hook, TriggerCtx};

const HOOKS: [Hook; 4] = [
    Hook::OnRoll,
    Hook::OnScoringDie,
    Hook::OnHandScored,
    Hook::OnRoundEnd,
];

const ETATS: [RelicState; 4] = [
    RelicState::None,
    RelicState::Counter(7),
    RelicState::Perishable { rounds_left: 2 },
    RelicState::Disabled,
];

/// Décor minimal d'un déclenchement, monté **depuis l'extérieur de la crate** :
/// c'est ce qui prouve que les quatre fonctions sont publiquement atteignables
/// avec les signatures annoncées.
struct Decor {
    hand: HandMatch,
    dice: Vec<Die>,
    hand_levels: HandLevels,
    blind: BlindContext,
}

impl Decor {
    fn new() -> Self {
        Self {
            hand: HandMatch {
                hand: YahtzeeHand::FullHouse,
                scoring_dice: vec![DieId(1), DieId(2)],
                discarded_dice: Vec::new(),
                potential_score: 0,
            },
            dice: vec![Die::new(DieId(1), 6), Die::new(DieId(2), 6)],
            hand_levels: HandLevels::default(),
            blind: BlindContext {
                blind: BlindDefinition::default(),
                target_score: 300,
                current_score: 0,
                hands_remaining: 4,
                used_hands: HandGrid::default(),
            },
        }
    }

    fn ctx(&self, state: RelicState) -> TriggerCtx<'_> {
        TriggerCtx {
            hand: &self.hand,
            dice: &self.dice,
            hand_levels: &self.hand_levels,
            blind: &self.blind,
            uid: 1,
            slot: 0,
            state,
            die: Some((DieId(1), 6)),
            base_chips: 30,
            base_mult: 400,
            left_effects: &[],
            roll_index: 0,
            rerolls_left: 2,
        }
    }
}

#[test]
fn test_skeleton_effects_are_empty() {
    let decor = Decor::new();
    let ctx = decor.ctx(RelicState::None);
    for def in CATALOG {
        for hook in HOOKS {
            assert!(
                effects_for(*def, hook, &ctx).is_empty(),
                "{def:?} produit déjà un effet sur {hook:?}"
            );
        }
    }
}

#[test]
fn test_skeleton_roll_modifier_is_neutral() {
    let decor = Decor::new();
    let ctx = decor.ctx(RelicState::None);
    for def in CATALOG {
        let modificateur = roll_modifier_for(*def, &ctx);
        assert_eq!(modificateur.reroll_delta, 0, "{def:?}");
        assert!(modificateur.force_values.is_empty(), "{def:?}");
    }
}

#[test]
fn test_skeleton_gold_is_zero() {
    let decor = Decor::new();
    let ctx = decor.ctx(RelicState::None);
    for def in CATALOG {
        assert_eq!(gold_for(*def, &ctx), 0, "{def:?}");
    }
}

#[test]
fn test_skeleton_advance_state_is_identity() {
    // Douze définitions x quatre déclencheurs x quatre états : 192 cas.
    let decor = Decor::new();
    for def in CATALOG {
        for hook in HOOKS {
            for etat in ETATS {
                let ctx = decor.ctx(etat);
                assert_eq!(
                    advance_state(*def, hook, &ctx, etat),
                    etat,
                    "{def:?} fait déjà avancer son état sur {hook:?}"
                );
            }
        }
    }
}

#[test]
fn test_effects_for_is_pure() {
    // **Deux appels identiques rendent deux résultats égaux**, et l'état porté
    // par le contexte ne bouge pas : `advance_state` est le seul écrivain
    // d'état (ADR-010). La propriété est aujourd'hui structurelle — rien n'est
    // mutable sur ce chemin — mais elle cessera de l'être dès que TASK-57
    // remplira un bras, et c'est là que ce test servira.
    let decor = Decor::new();
    for def in CATALOG {
        for hook in HOOKS {
            let ctx = decor.ctx(RelicState::Counter(3));
            let premier = effects_for(*def, hook, &ctx);
            let second = effects_for(*def, hook, &ctx);
            assert_eq!(premier, second, "{def:?} n'est pas pure sur {hook:?}");
            assert_eq!(ctx.state, RelicState::Counter(3), "{def:?} a touché l'état");
        }
    }
}
