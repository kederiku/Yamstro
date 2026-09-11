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
    /// Décor à dés imposés : les `comptabilises` premiers dés entrent dans la
    /// figure, les autres restent sur le plateau. C'est ce qui permet de
    /// distinguer une lecture des dés comptabilisés d'une lecture du plateau.
    fn avec_des(valeurs: &[u8], figure: YahtzeeHand, comptabilises: usize) -> Self {
        let dice = des(valeurs);
        let mut decor = Self::new();
        decor.hand = HandMatch {
            hand: figure,
            scoring_dice: dice[..comptabilises].iter().map(|de| de.id).collect(),
            discarded_dice: dice[comptabilises..].iter().map(|de| de.id).collect(),
            potential_score: 0,
        };
        decor.dice = dice;
        decor
    }

    /// Décor sur une figure choisie : la condition de ces reliques porte sur la
    /// figure **retenue**, pas sur les dés.
    fn avec_figure(figure: YahtzeeHand) -> Self {
        let mut decor = Self::new();
        decor.hand.hand = figure;
        decor
    }

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

/// Reliques encore muettes sur `effects_for`.
///
/// **Cette liste est le travail restant, encodée dans un test.** Chaque ticket
/// qui implémente une relique doit l'y retirer ; l'oublier fait échouer
/// `test_skeleton_effects_are_empty` bruyamment, et c'est le but — il est alors
/// impossible d'implémenter une relique sans regarder cette liste en face. À
/// TASK-62 elle sera vide et le test se supprimera de lui-même.
///
/// `ClayPiggyBank` et `GhostDie` y resteront jusqu'au bout : leur neutralité sur
/// ce hook est **définitive**, elles agissent ailleurs.
const EFFETS_ENCORE_NEUTRES: [RelicId; 3] = [
    RelicId::ClayPiggyBank,
    RelicId::GhostDie,
    RelicId::DoubleMirror,
];

#[test]
fn test_skeleton_effects_are_empty() {
    let decor = Decor::new();
    let ctx = decor.ctx(RelicState::None);
    for def in EFFETS_ENCORE_NEUTRES {
        for hook in HOOKS {
            assert!(
                effects_for(def, hook, &ctx).is_empty(),
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

// ---- Les trois reliques `OnScoringDie` (TASK-57) ----

use core_engine::evaluator::HandEvaluator;
use core_engine::scoring::{ScoreAction, ScoreEffect, ScoringPipeline, StepSource};

/// Dés de valeurs imposées, identifiants dans l'ordre du tableau.
fn des(valeurs: &[u8]) -> Vec<Die> {
    valeurs
        .iter()
        .enumerate()
        .map(|(index, valeur)| {
            let mut de = Die::new(DieId(index as u32), 6);
            de.current_value = *valeur;
            de
        })
        .collect()
}

fn figure(dice: &[Die], voulue: YahtzeeHand) -> HandMatch {
    HandEvaluator::evaluate(dice)
        .into_iter()
        .find(|candidate| candidate.hand == voulue)
        .expect("figure attendue")
}

fn inventaire_de(def: RelicId) -> RelicInventory {
    let mut inventaire = RelicInventory::new(5);
    inventaire.add_relic(def).expect("slot libre");
    inventaire
}

fn blind_nu() -> BlindContext {
    BlindContext {
        blind: BlindDefinition::default(),
        target_score: 300,
        current_score: 0,
        hands_remaining: 4,
        used_hands: HandGrid::default(),
    }
}

/// Les effets d'un palier, filtrés sur ceux que la relique a produits.
fn effets_de_relique(rapport: &core_engine::scoring::ScoringReport) -> Vec<ScoreAction> {
    rapport
        .steps
        .iter()
        .filter(|pas| matches!(pas.source, StepSource::Relic { .. }))
        .map(|pas| pas.action)
        .collect()
}

/// Un appel direct sur un dé donné : éprouve la **logique** de la relique,
/// sans décor de pipeline.
fn sur_le_de(def: RelicId, decor: &Decor, valeur: u8) -> Vec<ScoreAction> {
    let ctx = TriggerCtx {
        die: Some((DieId(1), valeur)),
        ..decor.ctx(RelicState::None)
    };
    effects_for(def, Hook::OnScoringDie, &ctx)
        .into_iter()
        .map(|effet| effet.action)
        .collect()
}

#[test]
fn test_cracked_die_on_odd_dice() {
    // **Par le pipeline**, parce que ce test affirme un *score* : trois dés
    // impairs sur une Grande Suite de niveau 1 donnent 40 + 15 = 55 Chips et
    // 400 + 300 = 700 centièmes de Mult, soit 385.
    let dice = des(&[1, 3, 5, 2, 4]);
    let main = figure(&dice, YahtzeeHand::LargeStraight);
    assert_eq!(
        main.scoring_dice.len(),
        5,
        "les cinq dés sont comptabilisés"
    );

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire_de(RelicId::CrackedDie),
        &blind_nu(),
    );

    assert_eq!(
        effets_de_relique(&rapport),
        vec![
            ScoreAction::AddMult(100),
            ScoreAction::AddMult(100),
            ScoreAction::AddMult(100)
        ],
        "un effet par dé impair, et rien d'autre"
    );
    // **Qui signe l'effet.** La source identifie la relique et l'exemplaire :
    // le journal l'affiche, et la mise en scène de l'Étape 4 s'en sert pour
    // frapper la bonne carte. Rien ne le vérifiait, et le banc a montré qu'une
    // relique pouvait signer du nom d'une autre.
    for pas in rapport
        .steps
        .iter()
        .filter(|pas| matches!(pas.source, StepSource::Relic { .. }))
    {
        assert_eq!(
            pas.source,
            StepSource::Relic {
                uid: 0,
                def: RelicId::CrackedDie
            }
        );
    }

    assert_eq!(rapport.chips, 55);
    assert_eq!(rapport.mult, 700);
    assert_eq!(rapport.final_score, 385);
}

#[test]
fn test_polished_stone_on_even_dice() {
    let decor = Decor::new();
    assert_eq!(
        sur_le_de(RelicId::PolishedStone, &decor, 2),
        vec![ScoreAction::AddChips(10)]
    );
    assert_eq!(
        sur_le_de(RelicId::PolishedStone, &decor, 4),
        vec![ScoreAction::AddChips(10)]
    );
    assert!(sur_le_de(RelicId::PolishedStone, &decor, 1).is_empty());
    assert!(sur_le_de(RelicId::PolishedStone, &decor, 5).is_empty());
    // Une face au-delà de six garde sa parité naturelle.
    assert_eq!(
        sur_le_de(RelicId::PolishedStone, &decor, 8),
        vec![ScoreAction::AddChips(10)]
    );
    assert!(sur_le_de(RelicId::PolishedStone, &decor, 7).is_empty());
}

#[test]
fn test_cracked_die_logic_is_parity() {
    let decor = Decor::new();
    for impair in [1, 3, 5, 7] {
        assert_eq!(
            sur_le_de(RelicId::CrackedDie, &decor, impair),
            vec![ScoreAction::AddMult(100)],
            "face {impair}"
        );
    }
    for pair in [2, 4, 6, 8] {
        assert!(
            sur_le_de(RelicId::CrackedDie, &decor, pair).is_empty(),
            "face {pair}"
        );
    }
}

#[test]
fn test_pyramid_without_six_is_silent() {
    let decor = Decor::new();
    for valeur in [1, 2, 3, 4, 5, 7, 8] {
        assert!(
            sur_le_de(RelicId::PyramidOfSixes, &decor, valeur).is_empty(),
            "face {valeur}"
        );
    }
}

#[test]
fn test_pyramid_fires_once_per_six() {
    // Par le pipeline : ce test parle du **nombre** de déclenchements, donc du
    // parcours, pas de la logique d'un dé.
    let dice = des(&[6, 6, 6, 1, 2]);
    let main = figure(&dice, YahtzeeHand::Sixes);
    assert_eq!(main.scoring_dice.len(), 3);

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire_de(RelicId::PyramidOfSixes),
        &blind_nu(),
    );
    assert_eq!(
        effets_de_relique(&rapport),
        vec![
            ScoreAction::AddChips(15),
            ScoreAction::AddChips(15),
            ScoreAction::AddChips(15)
        ]
    );
}

#[test]
fn test_discarded_die_triggers_nothing() {
    // **Par le pipeline**, parce que ce test parle d'une *sélection* de dés,
    // qui est le travail de l'évaluateur. Figure « Deux » : les deux 2 sont
    // comptabilisés, et l'écart porte un impair, un pair et un six.
    let dice = des(&[2, 2, 1, 4, 6]);
    let main = figure(&dice, YahtzeeHand::Twos);
    assert_eq!(main.scoring_dice.len(), 2);
    assert_eq!(
        main.discarded_dice.len(),
        3,
        "un impair, un pair et un six écartés"
    );

    for (def, attendu) in [
        (RelicId::CrackedDie, vec![]),
        (
            RelicId::PolishedStone,
            vec![ScoreAction::AddChips(10), ScoreAction::AddChips(10)],
        ),
        (RelicId::PyramidOfSixes, vec![]),
    ] {
        let rapport = ScoringPipeline::resolve(
            &main,
            &dice,
            &HandLevels::default(),
            &inventaire_de(def),
            &blind_nu(),
        );
        assert_eq!(effets_de_relique(&rapport), attendu, "{def:?}");
    }
}

#[test]
fn test_scoring_die_relics_silent_on_hand_scored() {
    // **Chaque relique est éprouvée sur une face qui la ferait parler**, et sa
    // source est vérifiée au passage. Avec une face unique, la garde de hook
    // serait masquée par la condition de valeur : un six laisse *Le Dé Fêlé*
    // muet parce qu'il est pair, pas parce que le hook ne lui convient pas. Et
    // sans l'assertion de source, une relique pourrait signer du nom d'une
    // autre — la mise en scène de l'Étape 4 frapperait alors la mauvaise carte.
    // Le banc a montré les deux trous.
    let decor = Decor::new();
    for (def, face_qui_declenche) in [
        (RelicId::CrackedDie, 3),
        (RelicId::PolishedStone, 4),
        (RelicId::PyramidOfSixes, 6),
    ] {
        let ctx = TriggerCtx {
            die: Some((DieId(1), face_qui_declenche)),
            ..decor.ctx(RelicState::None)
        };

        let produits = effects_for(def, Hook::OnScoringDie, &ctx);
        assert_eq!(
            produits.len(),
            1,
            "{def:?} ne parle pas sur la face qui devrait la déclencher"
        );
        assert_eq!(
            produits[0].source,
            StepSource::Relic { uid: ctx.uid, def },
            "{def:?} signe du nom d'une autre"
        );

        for hook in [Hook::OnRoll, Hook::OnHandScored, Hook::OnRoundEnd] {
            assert!(
                effects_for(def, hook, &ctx).is_empty(),
                "{def:?} parle sur {hook:?}"
            );
        }
    }
}

#[test]
fn test_scoring_die_relics_tolerate_none_die() {
    // Rien n'interdit structurellement un appel sans dé : la lecture doit
    // rendre une file vide, jamais interrompre le calcul.
    let decor = Decor::new();
    let ctx = TriggerCtx {
        die: None,
        ..decor.ctx(RelicState::None)
    };
    for def in [
        RelicId::CrackedDie,
        RelicId::PolishedStone,
        RelicId::PyramidOfSixes,
    ] {
        assert!(
            effects_for(def, Hook::OnScoringDie, &ctx).is_empty(),
            "{def:?}"
        );
    }
}

// ---- Les trois reliques additives `OnHandScored` (TASK-58) ----

/// Ce qu'une relique produit sur la figure retenue. Appel **direct** : ces
/// tests parlent d'une condition sur la figure, pas d'un parcours.
fn sur_la_figure(def: RelicId, figure: YahtzeeHand) -> Vec<ScoreEffect> {
    let decor = Decor::avec_figure(figure);
    let ctx = TriggerCtx {
        die: None,
        ..decor.ctx(RelicState::None)
    };
    effects_for(def, Hook::OnHandScored, &ctx)
        .into_iter()
        .collect()
}

fn actions(effets: &[ScoreEffect]) -> Vec<ScoreAction> {
    effets.iter().map(|effet| effet.action).collect()
}

#[test]
fn test_triplet_master_fires_on_three_and_four_of_a_kind() {
    for figure in [YahtzeeHand::ThreeOfAKind, YahtzeeHand::FourOfAKind] {
        let effets = sur_la_figure(RelicId::TripletMaster, figure);
        assert_eq!(
            actions(&effets),
            vec![ScoreAction::AddMult(600)],
            "{figure:?}"
        );
        assert_eq!(
            effets[0].source,
            StepSource::Relic {
                uid: 1,
                def: RelicId::TripletMaster
            }
        );
    }
}

#[test]
fn test_triplet_master_silent_on_full_house() {
    // Un Full contient un brelan, mais la condition porte sur la figure
    // **retenue** : recomposer à partir des dés la ferait parler ici.
    assert!(sur_la_figure(RelicId::TripletMaster, YahtzeeHand::FullHouse).is_empty());
}

#[test]
fn test_full_house_architect_emits_two_effects_in_order() {
    // **L'ordre est normatif** : la passe B replie séquentiellement, un palier
    // par effet, et l'Étape 4 les anime dans cet ordre.
    let effets = sur_la_figure(RelicId::FullHouseArchitect, YahtzeeHand::FullHouse);
    assert_eq!(
        actions(&effets),
        vec![ScoreAction::AddChips(40), ScoreAction::AddMult(500)]
    );
}

#[test]
fn test_full_house_architect_silent_elsewhere() {
    for figure in YahtzeeHand::ALL {
        if figure == YahtzeeHand::FullHouse {
            continue;
        }
        assert!(
            sur_la_figure(RelicId::FullHouseArchitect, figure).is_empty(),
            "{figure:?}"
        );
    }
}

#[test]
fn test_stellar_alignment_fires_on_both_straights() {
    for figure in [YahtzeeHand::SmallStraight, YahtzeeHand::LargeStraight] {
        assert_eq!(
            actions(&sur_la_figure(RelicId::StellarAlignment, figure)),
            vec![ScoreAction::AddMult(800)],
            "{figure:?}"
        );
    }
}

#[test]
fn test_stellar_alignment_silent_on_other_hands() {
    for figure in YahtzeeHand::ALL {
        if matches!(
            figure,
            YahtzeeHand::SmallStraight | YahtzeeHand::LargeStraight
        ) {
            continue;
        }
        assert!(
            sur_la_figure(RelicId::StellarAlignment, figure).is_empty(),
            "{figure:?}"
        );
    }
}

#[test]
fn test_additive_relics_never_multiply() {
    // La multiplication est réservée aux raretés hautes : deux de ces trois
    // reliques sont Communes, la troisième Peu commune.
    for def in [
        RelicId::TripletMaster,
        RelicId::FullHouseArchitect,
        RelicId::StellarAlignment,
    ] {
        for figure in YahtzeeHand::ALL {
            for action in actions(&sur_la_figure(def, figure)) {
                assert!(
                    !matches!(action, ScoreAction::MultiplyMult(_)),
                    "{def:?} multiplie sur {figure:?}"
                );
            }
        }
    }
}

#[test]
fn test_additive_relics_silent_on_scoring_die() {
    // Éprouvées sur la figure qui les déclenche, sans quoi la garde de hook
    // serait masquée par la condition de figure.
    let decor = Decor::avec_figure(YahtzeeHand::FullHouse);
    for (def, figure) in [
        (RelicId::TripletMaster, YahtzeeHand::ThreeOfAKind),
        (RelicId::FullHouseArchitect, YahtzeeHand::FullHouse),
        (RelicId::StellarAlignment, YahtzeeHand::LargeStraight),
    ] {
        let sur_mesure = Decor::avec_figure(figure);
        let ctx = TriggerCtx {
            die: Some((DieId(1), 3)),
            ..sur_mesure.ctx(RelicState::None)
        };
        // **Qui signe l'effet**, pour chacune des trois. C'est le second
        // ticket d'affilée où le banc trouve une relique capable d'émettre au
        // nom d'une autre : la vérification devient systématique, toute relique
        // qui parle doit prouver sa signature.
        let produits = effects_for(def, Hook::OnHandScored, &ctx);
        assert!(!produits.is_empty(), "{def:?} muette sur sa propre figure");
        for effet in &produits {
            assert_eq!(
                effet.source,
                StepSource::Relic { uid: ctx.uid, def },
                "{def:?} signe du nom d'une autre"
            );
        }
        for hook in [Hook::OnRoll, Hook::OnScoringDie, Hook::OnRoundEnd] {
            assert!(
                effects_for(def, hook, &ctx).is_empty(),
                "{def:?} sur {hook:?}"
            );
        }
    }
    let _ = decor;
}

#[test]
fn test_additive_relic_fires_once_per_hand_not_per_die() {
    // **Par le pipeline**, parce que c'est une propriété du parcours : câblée
    // sur le mauvais hook, cette relique rendrait un effet par dé comptabilisé.
    // Aucun appel direct ne peut voir la différence.
    let dice = des(&[4, 4, 4, 1, 2]);
    let main = figure(&dice, YahtzeeHand::ThreeOfAKind);
    assert_eq!(main.scoring_dice.len(), 3, "trois dés comptabilisés");

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire_de(RelicId::TripletMaster),
        &blind_nu(),
    );
    assert_eq!(
        effets_de_relique(&rapport),
        vec![ScoreAction::AddMult(600)],
        "un effet par main, jamais un par dé"
    );
}

// ---- Les trois reliques multiplicatives (TASK-59) ----

/// Score d'une main donnée avec un inventaire donné.
fn score_avec(dice: &[Die], main: &HandMatch, relics: &RelicInventory) -> u64 {
    ScoringPipeline::resolve(main, dice, &HandLevels::default(), relics, &blind_nu()).final_score
}

fn inventaire_ordonne(defs: &[RelicId]) -> RelicInventory {
    let mut inventaire = RelicInventory::new(5);
    for def in defs {
        inventaire.add_relic(*def).expect("slot libre");
    }
    inventaire
}

#[test]
fn test_triplet_master_then_pendulum() {
    // Brelan de 4 : base 10 Chips et 200 centièmes de Mult, plus 4+4+4 = 12
    // Chips des dés comptabilisés, donc 22 Chips. Somme comptabilisée **paire**
    // — le plateau, lui, vaut 19, impair. Le Mult passe par 200 + 600 = 800
    // puis ×1,5 = 1200, et 22 × 12,00 donne 264.
    let dice = des(&[4, 4, 4, 6, 1]);
    let main = figure(&dice, YahtzeeHand::ThreeOfAKind);
    let inventaire = inventaire_ordonne(&[RelicId::TripletMaster, RelicId::Pendulum]);

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    assert_eq!(rapport.chips, 22);
    assert_eq!(rapport.mult, 1200);
    assert_eq!(rapport.final_score, 264);
}

#[test]
fn test_relic_reorder_changes_score() {
    // **Le test de non-régression central de l'étape.** ADR-005 ne dit pas que
    // deux inventaires différents donnent deux scores : il dit que le
    // **réordonnancement par le joueur** change le score. Ce geste a un nom
    // dans le code, et c'est lui qu'on appelle ici — le même inventaire, avant
    // et après.
    let dice = des(&[4, 4, 4, 6, 1]);
    let main = figure(&dice, YahtzeeHand::ThreeOfAKind);
    let mut inventaire = inventaire_ordonne(&[RelicId::TripletMaster, RelicId::Pendulum]);

    let avant = score_avec(&dice, &main, &inventaire);
    inventaire.reorder(0, 1);
    let apres = score_avec(&dice, &main, &inventaire);

    // 200 + 600 = 800 puis x1,5 = 1200 ; contre 200 x1,5 = 300 puis + 600 = 900.
    assert_eq!(avant, 264);
    assert_eq!(apres, 198);
    assert_ne!(
        avant, apres,
        "l'ordre de l'inventaire ne change pas le score"
    );
}

#[test]
fn test_divine_yahtzee_is_multiply_mult() {
    // Le Yams multiplie le **Mult**, jamais le score total : la formule
    // Score = Chips x Mult ne se remplace pas.
    let effets = sur_la_figure(RelicId::DivineYahtzee, YahtzeeHand::Yahtzee);
    assert_eq!(actions(&effets), vec![ScoreAction::MultiplyMult(300)]);
}

#[test]
fn test_pendulum_silent_on_odd_sum() {
    // Miroir du cas de référence : plateau 16 (pair), comptabilisés 9 (impair).
    // Une lecture du plateau déclencherait à tort.
    let decor = Decor::avec_des(&[3, 3, 3, 2, 5], YahtzeeHand::ThreeOfAKind, 3);
    let ctx = TriggerCtx {
        die: None,
        ..decor.ctx(RelicState::None)
    };
    assert!(effects_for(RelicId::Pendulum, Hook::OnHandScored, &ctx).is_empty());
}

#[test]
fn test_pendulum_reads_scoring_dice_not_board() {
    // Les deux mains sont choisies pour **inverser** le verdict selon qu'on lit
    // les dés comptabilisés ou le plateau entier.
    for (valeurs, declenche) in [([4u8, 4, 4, 6, 1], true), ([3, 3, 3, 2, 5], false)] {
        let decor = Decor::avec_des(&valeurs, YahtzeeHand::ThreeOfAKind, 3);
        let ctx = TriggerCtx {
            die: None,
            ..decor.ctx(RelicState::None)
        };
        let effets = effects_for(RelicId::Pendulum, Hook::OnHandScored, &ctx);
        assert_eq!(!effets.is_empty(), declenche, "{valeurs:?}");
    }
}

#[test]
fn test_pendulum_finds_dice_by_identity_not_index() {
    // **Le pool retire et ajoute des dés en cours de manche**, donc un
    // identifiant n'est pas un index. Tous les autres montages du fichier
    // donnent `DieId(i)` au dé d'index `i` : indexer et chercher par identité
    // y coïncident, et le banc a montré qu'aucun test ne les séparait.
    //
    // Ici les identifiants sont 0, 1 et 5 pour trois dés valant 1, 2 et 3. La
    // recherche par identité somme 6 — pair, la relique parle. Une indexation
    // ne trouverait que les deux premiers, sommerait 3 — impair — et la
    // relique se tairait.
    let dice: Vec<Die> = [(0u32, 1u8), (1, 2), (5, 3)]
        .into_iter()
        .map(|(id, valeur)| {
            let mut de = Die::new(DieId(id), 6);
            de.current_value = valeur;
            de
        })
        .collect();

    let mut decor = Decor::new();
    decor.hand = HandMatch {
        hand: YahtzeeHand::ThreeOfAKind,
        scoring_dice: dice.iter().map(|de| de.id).collect(),
        discarded_dice: Vec::new(),
        potential_score: 0,
    };
    decor.dice = dice;

    let ctx = TriggerCtx {
        die: None,
        ..decor.ctx(RelicState::None)
    };
    assert_eq!(
        actions(
            &effects_for(RelicId::Pendulum, Hook::OnHandScored, &ctx)
                .into_iter()
                .collect::<Vec<_>>()
        ),
        vec![ScoreAction::MultiplyMult(150)],
        "la somme est calculée par indexation, pas par identité"
    );
}

#[test]
fn test_unstable_obsidian_is_unconditional() {
    for figure in YahtzeeHand::ALL {
        assert_eq!(
            actions(&sur_la_figure(RelicId::UnstableObsidian, figure)),
            vec![ScoreAction::MultiplyMult(200)],
            "{figure:?}"
        );
    }
}

#[test]
fn test_divine_yahtzee_only_on_yahtzee() {
    for figure in YahtzeeHand::ALL {
        if figure == YahtzeeHand::Yahtzee {
            continue;
        }
        assert!(
            sur_la_figure(RelicId::DivineYahtzee, figure).is_empty(),
            "{figure:?}"
        );
    }
}

#[test]
fn test_multiplicative_relics_silent_on_scoring_die() {
    // Chacune éprouvée sur ce qui la déclenche, et sa signature vérifiée —
    // la vérification systématique arrêtée à TASK-58.
    for (def, valeurs) in [
        (RelicId::Pendulum, [4u8, 4, 4, 6, 1]),
        (RelicId::UnstableObsidian, [4, 4, 4, 6, 1]),
        (RelicId::DivineYahtzee, [4, 4, 4, 4, 4]),
    ] {
        let voulue = if def == RelicId::DivineYahtzee {
            YahtzeeHand::Yahtzee
        } else {
            YahtzeeHand::ThreeOfAKind
        };
        let decor = Decor::avec_des(&valeurs, voulue, 3);
        let ctx = TriggerCtx {
            die: Some((DieId(0), 4)),
            ..decor.ctx(RelicState::None)
        };

        let produits = effects_for(def, Hook::OnHandScored, &ctx);
        assert!(!produits.is_empty(), "{def:?} muette sur son propre cas");
        for effet in &produits {
            assert_eq!(
                effet.source,
                StepSource::Relic { uid: ctx.uid, def },
                "{def:?}"
            );
        }

        for hook in [Hook::OnRoll, Hook::OnScoringDie, Hook::OnRoundEnd] {
            assert!(
                effects_for(def, hook, &ctx).is_empty(),
                "{def:?} sur {hook:?}"
            );
        }
    }
}
