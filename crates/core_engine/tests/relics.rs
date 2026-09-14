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
    // produit E0004 et rien ne compile. Cette boucle-ci n'en est que le témoin
    // d'exécution — elle ne peut pas échouer là où le compilateur a déjà parlé.
    for def in CATALOG {
        let _ = rarity_of(*def);
    }

    // **La répartition est déjà testée**, et ailleurs : TASK-53 a écrit
    // `test_rarity_distribution_is_four_four_four`, aux mêmes quatre
    // assertions. La recopier ici donnerait deux exemplaires à maintenir et un
    // à oublier le jour où l'Étape 9 fera passer le catalogue à soixante.
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

use core_engine::config::{RunConfig, effective_rerolls};
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

use core_engine::blinds::{BlindContext, BlindDefinition, BlindModifier};
use core_engine::dice::{Die, DieId};
use core_engine::economy::round_end_gold;
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
/// impossible d'implémenter une relique sans regarder cette liste en face.
///
/// **Sa part résiduelle est épuisée depuis TASK-62.** Ce qui reste est
/// permanent : `ClayPiggyBank` et `GhostDie` sont définitivement muettes sur ce
/// hook, elles agissent ailleurs. Elle ne se videra donc pas et le test ne se
/// supprimera pas — la promesse inverse, écrite à TASK-56, contredisait déjà la
/// phrase suivante.
const EFFETS_ENCORE_NEUTRES: [RelicId; 2] = [RelicId::ClayPiggyBank, RelicId::GhostDie];

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

/// Reliques neutres sur `roll_modifier_for`.
///
/// **Contrairement à `EFFETS_ENCORE_NEUTRES`, cette liste ne rétrécira plus.**
/// Les dix qui y figurent ne toucheront jamais au lancer : leur neutralité est
/// définitive, pas datée. Elle ne se videra donc pas d'elle-même, et le test
/// qu'elle nourrit ne se supprimera pas.
const LANCER_NEUTRE: [RelicId; 10] = [
    RelicId::CrackedDie,
    RelicId::PolishedStone,
    RelicId::TripletMaster,
    RelicId::FullHouseArchitect,
    RelicId::StellarAlignment,
    RelicId::PyramidOfSixes,
    RelicId::Pendulum,
    RelicId::DivineYahtzee,
    RelicId::ClayPiggyBank,
    RelicId::DoubleMirror,
];

#[test]
fn test_skeleton_roll_modifier_is_neutral() {
    // **Ce test ne garde pas le Dé Fantôme, et ne l'a jamais gardé.** Les deux
    // dés de `Decor::new()` affichent 1 — `Die::new` démarre à 1 — donc sa
    // garde « aucun dé n'affiche 1 » y est fausse quoi qu'il arrive. Il serait
    // resté vert sur une implémentation à l'envers. Sa couverture réelle vient
    // de ses cinq tests propres, plus bas.
    let decor = Decor::new();
    assert_eq!(
        LANCER_NEUTRE.len() + 2,
        CATALOG.len(),
        "seules l'Obsidienne et le Dé Fantôme touchent au lancer"
    );
    for def in LANCER_NEUTRE {
        let modificateur = roll_modifier_for(def, &decor.dice, 0);
        assert_eq!(modificateur.reroll_delta, 0, "{def:?}");
        assert!(modificateur.force_values.is_empty(), "{def:?}");
    }
}

#[test]
fn test_skeleton_gold_is_zero() {
    // **Ne garde plus la Tirelire, et ne l'a jamais gardée.** Le décor pose
    // `RelicState::None`, sur lequel elle rend zéro de toute façon : ce test
    // serait resté vert sur un plafond faux. Sa couverture propre est plus bas.
    let decor = Decor::new();
    let ctx = decor.ctx(RelicState::None);
    for def in SANS_OR_NI_MEMOIRE {
        assert_eq!(gold_for(def, &ctx), 0, "{def:?}");
    }
}

/// Reliques sans bourse et sans mémoire : ni or, ni avancement d'état.
///
/// **Une seule liste pour deux propriétés distinctes**, parce qu'elles portent
/// aujourd'hui sur exactement les mêmes onze reliques. Deux noms pour un même
/// contenu n'ajouteraient rien ; si une relique future rapportait de l'or sans
/// garder d'état, ou l'inverse, c'est ici qu'il faudrait scinder.
///
/// **Cette liste ne rétrécira plus**, comme `LANCER_NEUTRE` et contrairement à
/// `EFFETS_ENCORE_NEUTRES`.
const SANS_OR_NI_MEMOIRE: [RelicId; 11] = [
    RelicId::CrackedDie,
    RelicId::PolishedStone,
    RelicId::TripletMaster,
    RelicId::FullHouseArchitect,
    RelicId::StellarAlignment,
    RelicId::PyramidOfSixes,
    RelicId::Pendulum,
    RelicId::UnstableObsidian,
    RelicId::DivineYahtzee,
    RelicId::GhostDie,
    RelicId::DoubleMirror,
];

#[test]
fn test_advance_state_touches_no_other_relic() {
    // Onze définitions x quatre déclencheurs x quatre états : 176 cas. La
    // douzième, la Tirelire, a ses propres tests : elle est le seul cas où
    // l'identité serait un défaut.
    let decor = Decor::new();
    for def in SANS_OR_NI_MEMOIRE {
        for hook in HOOKS {
            for etat in ETATS {
                let ctx = decor.ctx(etat);
                assert_eq!(
                    advance_state(def, hook, &ctx, etat),
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

// ---- *Miroir Double* et la ré-émission (TASK-60) ----

/// Les paliers de source relique, avec leur identifiant d'émetteur.
fn paliers_de_relique(
    rapport: &core_engine::scoring::ScoringReport,
) -> Vec<(u32, RelicId, ScoreAction)> {
    rapport
        .steps
        .iter()
        .filter_map(|pas| match pas.source {
            StepSource::Relic { uid, def } => Some((uid, def, pas.action)),
            _ => None,
        })
        .collect()
}

fn brelan_de_quatre() -> (Vec<Die>, HandMatch) {
    let dice = des(&[4, 4, 4, 6, 1]);
    let main = figure(&dice, YahtzeeHand::ThreeOfAKind);
    (dice, main)
}

#[test]
fn test_double_mirror_no_borrow_conflict() {
    // Le Mult passe par 200, +600 du Brelan, +600 du Miroir qui ré-émet, soit
    // 1400 ; 22 x 14,00 donne 308.
    //
    // **La garantie d'absence de conflit d'emprunt n'est pas testée ici, et ne
    // peut pas l'être** : elle est structurelle. La phase A ne dispose d'aucun
    // emprunt exclusif, donc le conflit ne peut pas se former ; si elle en
    // disposait, la crate entière refuserait de compiler bien avant ce test.
    // Ce que ce test mesure, c'est le score et l'attribution.
    let (dice, main) = brelan_de_quatre();
    let inventaire = inventaire_ordonne(&[RelicId::TripletMaster, RelicId::DoubleMirror]);

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    assert_eq!(rapport.chips, 22);
    assert_eq!(rapport.mult, 1400);
    assert_eq!(rapport.final_score, 308);
}

#[test]
fn test_double_mirror_alone_emits_nothing() {
    let (dice, main) = brelan_de_quatre();
    let inventaire = inventaire_ordonne(&[RelicId::DoubleMirror]);

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    assert!(
        paliers_de_relique(&rapport).is_empty(),
        "rien à gauche du slot 0"
    );
    assert_eq!(rapport.mult, 200);
}

#[test]
fn test_double_mirror_with_disabled_neighbour() {
    // **Désactiver un slot casse la chaîne du Miroir.** C'est ce qui rend les
    // boss d'extinction déterministes sans branche particulière : le Miroir ne
    // teste pas si sa voisine est éteinte, il lit une tranche vide.
    let (dice, main) = brelan_de_quatre();
    let mut inventaire = inventaire_ordonne(&[RelicId::TripletMaster, RelicId::DoubleMirror]);
    inventaire.slots[0]
        .as_mut()
        .expect("relique au slot 0")
        .state = RelicState::Disabled;

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    assert!(paliers_de_relique(&rapport).is_empty());
    assert_eq!(rapport.mult, 200);
}

#[test]
fn test_disabled_slot_breaks_the_chain_at_three_slots() {
    // **Le cas discriminant de l'arbitrage.** La règle écartée — « un slot
    // stérile ne réinitialise pas la tranche », la voisine étant alors le
    // dernier slot ayant réellement produit — coïncide avec la règle retenue
    // sur **toutes** les configurations à deux slots. Le test à deux slots ne
    // distingue donc pas les deux règles : il faut trois slots et un slot
    // éteint **au milieu**.
    //
    // Sans ce test, restaurer la règle écartée dans `scan_relics` ne casserait
    // rien, et rendrait le Miroir transparent à `DisableRelicSlot`,
    // `DisableRightmostRelic` et `DisableRarity` — les trois boss que
    // l'arbitrage rend déterministes.
    let (dice, main) = brelan_de_quatre();
    let mut inventaire = inventaire_ordonne(&[
        RelicId::TripletMaster,
        RelicId::TripletMaster,
        RelicId::DoubleMirror,
    ]);
    inventaire.slots[1]
        .as_mut()
        .expect("relique au slot 1")
        .state = RelicState::Disabled;
    let uids: Vec<u32> = inventaire
        .iter_slots()
        .map(|(_, relique)| relique.uid)
        .collect();

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    assert_eq!(
        paliers_de_relique(&rapport),
        vec![(uids[0], RelicId::TripletMaster, ScoreAction::AddMult(600))],
        "le slot éteint casse la chaîne : le Miroir lit une tranche vide, \
         il ne remonte pas au dernier slot productif"
    );
}

#[test]
fn test_empty_slot_breaks_the_chain_at_three_slots() {
    // Le jumeau du test précédent. Le § 2.2 traite d'un seul souffle le voisin
    // **absent** et le voisin éteint, mais ce sont deux branches distinctes de
    // `scan_relics` : retirer la réinitialisation de l'une laisse l'autre
    // verte. `remove_relic` prend par `take`, donc il laisse un trou au milieu
    // plutôt que de tasser les slots — c'est ce qui rend ce montage possible.
    let (dice, main) = brelan_de_quatre();
    let mut inventaire = inventaire_ordonne(&[
        RelicId::TripletMaster,
        RelicId::TripletMaster,
        RelicId::DoubleMirror,
    ]);
    let uids: Vec<u32> = inventaire
        .iter_slots()
        .map(|(_, relique)| relique.uid)
        .collect();
    inventaire.remove_relic(1).expect("relique au slot 1");
    assert!(
        inventaire.slots[1].is_none(),
        "le retrait laisse un trou, il ne tasse pas"
    );

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    assert_eq!(
        paliers_de_relique(&rapport),
        vec![(uids[0], RelicId::TripletMaster, ScoreAction::AddMult(600))]
    );
}

#[test]
fn test_double_mirror_reemits_under_own_uid() {
    // **Seule l'action est recopiée ; la source est celle du Miroir.** Le score
    // est identique dans les deux cas, et seule l'attribution diffère : c'est
    // elle qui garde le journal juste et qui fait que la mise en scène frappe
    // la carte du Miroir, non celle de la voisine déjà secouée à son palier.
    let (dice, main) = brelan_de_quatre();
    let inventaire = inventaire_ordonne(&[RelicId::TripletMaster, RelicId::DoubleMirror]);
    let uids: Vec<u32> = inventaire
        .iter_slots()
        .map(|(_, relique)| relique.uid)
        .collect();

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    assert_eq!(
        paliers_de_relique(&rapport),
        vec![
            (uids[0], RelicId::TripletMaster, ScoreAction::AddMult(600)),
            (uids[1], RelicId::DoubleMirror, ScoreAction::AddMult(600)),
        ]
    );
}

#[test]
fn test_two_mirrors_do_not_cascade() {
    // Le second Miroir voit **la production du premier**, un effet, jamais
    // l'union des slots précédents : trois effets au total, pas quatre.
    let (dice, main) = brelan_de_quatre();
    let inventaire = inventaire_ordonne(&[
        RelicId::TripletMaster,
        RelicId::DoubleMirror,
        RelicId::DoubleMirror,
    ]);

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    let paliers = paliers_de_relique(&rapport);
    assert_eq!(paliers.len(), 3, "cascade : {paliers:?}");
    assert!(
        paliers
            .iter()
            .all(|(_, _, action)| *action == ScoreAction::AddMult(600))
    );

    let emetteurs: Vec<u32> = paliers.iter().map(|(uid, _, _)| *uid).collect();
    assert_eq!(
        emetteurs.len(),
        emetteurs
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        "les trois paliers viennent de trois émetteurs distincts"
    );
}

#[test]
fn test_double_mirror_doubles_the_obsidian_multiplication() {
    // Le Miroir double la multiplication de l'Obsidienne — et **pas** son malus
    // de relance, qui ne transite pas par ce canal : il sort d'une autre
    // fonction, sur un autre hook, que la phase A n'atteint pas.
    //
    // La neutralité du Miroir sur le modificateur de lancer est déjà tenue par
    // `test_skeleton_roll_modifier_is_neutral` : la dupliquer ici donnerait une
    // seconde copie à maintenir, pas une couverture de plus.
    let (dice, main) = brelan_de_quatre();
    let inventaire = inventaire_ordonne(&[RelicId::UnstableObsidian, RelicId::DoubleMirror]);

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    assert_eq!(
        paliers_de_relique(&rapport)
            .into_iter()
            .map(|(_, _, action)| action)
            .collect::<Vec<_>>(),
        vec![
            ScoreAction::MultiplyMult(200),
            ScoreAction::MultiplyMult(200)
        ]
    );
}

#[test]
fn test_double_mirror_reemits_the_whole_slice_in_order() {
    // Les sept tests imposés prennent tous une voisine à **un seul** effet, si
    // bien qu'aucun ne distingue « ré-émettre la tranche » de « ré-émettre son
    // premier effet », ni l'ordre de son inverse. Le banc de mutation le
    // confirme : `.take(1)` et `.rev()` leur survivent tous les deux.
    //
    // *Architecte du Full* est la seule voisine à en produire deux, dans un
    // ordre lui-même normatif (TASK-58 § 2.1) : elle referme les deux trous
    // d'un coup.
    let dice = des(&[3, 3, 5, 5, 5]);
    let main = figure(&dice, YahtzeeHand::FullHouse);
    let inventaire = inventaire_ordonne(&[RelicId::FullHouseArchitect, RelicId::DoubleMirror]);
    let uids: Vec<u32> = inventaire
        .iter_slots()
        .map(|(_, relique)| relique.uid)
        .collect();

    let rapport = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    assert_eq!(
        paliers_de_relique(&rapport),
        vec![
            (
                uids[0],
                RelicId::FullHouseArchitect,
                ScoreAction::AddChips(40)
            ),
            (
                uids[0],
                RelicId::FullHouseArchitect,
                ScoreAction::AddMult(500)
            ),
            (uids[1], RelicId::DoubleMirror, ScoreAction::AddChips(40)),
            (uids[1], RelicId::DoubleMirror, ScoreAction::AddMult(500)),
        ]
    );
}

#[test]
fn test_double_mirror_silent_on_scoring_die() {
    // Miroiter par dé démultiplierait la voisine par le nombre de dés
    // comptabilisés.
    let decor = Decor::avec_des(&[4, 4, 4, 6, 1], YahtzeeHand::ThreeOfAKind, 3);
    let voisine = [ScoreEffect {
        source: StepSource::Relic {
            uid: 9,
            def: RelicId::TripletMaster,
        },
        action: ScoreAction::AddMult(600),
    }];
    let ctx = TriggerCtx {
        die: Some((DieId(0), 4)),
        left_effects: &voisine,
        ..decor.ctx(RelicState::None)
    };

    assert!(effects_for(RelicId::DoubleMirror, Hook::OnScoringDie, &ctx).is_empty());
    assert_eq!(
        effects_for(RelicId::DoubleMirror, Hook::OnHandScored, &ctx).len(),
        1,
        "le montage ne teste rien si le Miroir est muet sur son propre hook"
    );
}

// ---- Les deux reliques du lancer (TASK-61) ----

/// Dés dont l'identifiant **ne suit pas** la position.
///
/// `des` aligne `DieId(i)` sur l'indice `i`, si bien qu'aucun test bâti dessus
/// ne distingue une recherche par identité d'un accès par indice — les deux
/// rendent le même dé. Ce montage-ci les sépare, et c'est ce que le § 2.4
/// demande de garder : le boss *La Meule* retire un dé en cours de manche, et
/// tout indice capturé avant serait faux après.
fn des_identifies(paires: &[(u32, u8)]) -> Vec<Die> {
    paires
        .iter()
        .map(|(id, valeur)| {
            let mut de = Die::new(DieId(*id), 6);
            de.current_value = *valeur;
            de
        })
        .collect()
}

fn decor_de_lancer(dice: Vec<Die>) -> Decor {
    let mut decor = Decor::new();
    decor.dice = dice;
    decor
}

#[test]
fn test_unstable_obsidian_reroll_delta() {
    // **Le `-1` vient de `roll_modifier_for`, pas d'un littéral.** Écrit en
    // dur, ce test mesurerait `apply_delta`, que `test_rerolls_underflow_
    // saturates` couvre depuis l'Étape 1, et rien de cette relique.
    //
    // La saturation elle-même ne distingue pas debug de release : `apply_delta`
    // est en `saturating_sub` depuis l'Étape 1, donc aucun `0 - 1` n'existe
    // dans la chaîne. Le mode de compilation ne change rien ici.
    let decor = decor_de_lancer(des(&[2, 3, 4, 5, 6]));
    let delta = roll_modifier_for(RelicId::UnstableObsidian, &decor.dice, 0).reroll_delta;
    assert_eq!(delta, -1);

    let config = RunConfig::from_cup(&cup(CupId::Abandoned));
    assert_eq!(config.base_rerolls, 0, "le Gobelet Abandonné part de zéro");
    assert_eq!(effective_rerolls(&config, 0, None, delta), 0);
}

#[test]
fn test_obsidian_force_values_empty() {
    // Inconditionnelle : le rang du lancer et les faces ne la concernent pas.
    for valeurs in [vec![2, 3, 4, 5, 6], vec![1, 1, 1, 1, 1]] {
        let decor = decor_de_lancer(des(&valeurs));
        for roll_index in [0, 1, 7] {
            let modificateur =
                roll_modifier_for(RelicId::UnstableObsidian, &decor.dice, roll_index);
            assert_eq!(
                modificateur.reroll_delta, -1,
                "{valeurs:?} au lancer {roll_index}"
            );
            assert!(modificateur.force_values.is_empty());
        }
    }
}

#[test]
fn test_ghost_die_forces_six() {
    let decor = decor_de_lancer(des(&[2, 3, 4, 5, 6]));
    let modificateur = roll_modifier_for(RelicId::GhostDie, &decor.dice, 0);

    assert_eq!(modificateur.force_values, vec![(DieId(0), 6)]);
}

#[test]
fn test_ghost_die_forces_by_identity_not_by_position() {
    // Le plus petit dé est en **dernière** position et porte `DieId(3)` ; le dé
    // de tête porte `DieId(7)`. Un `dice[0]`, un `min` sur les indices ou un
    // `position()` rendus tels quels donneraient une autre paire.
    let decor = decor_de_lancer(des_identifies(&[(7, 5), (2, 6), (9, 4), (3, 2)]));
    let modificateur = roll_modifier_for(RelicId::GhostDie, &decor.dice, 0);

    assert_eq!(modificateur.force_values, vec![(DieId(3), 6)]);
}

#[test]
fn test_ghost_die_silent_when_one_present() {
    let decor = decor_de_lancer(des(&[1, 3, 4, 5, 6]));
    let modificateur = roll_modifier_for(RelicId::GhostDie, &decor.dice, 0);

    assert!(modificateur.force_values.is_empty());
}

#[test]
fn test_ghost_die_silent_after_first_roll() {
    // Zéro est le premier lancer. Une convention partant de un rendrait la
    // relique silencieuse pour toujours.
    let decor = decor_de_lancer(des(&[2, 3, 4, 5, 6]));
    for roll_index in [1, 2] {
        let modificateur = roll_modifier_for(RelicId::GhostDie, &decor.dice, roll_index);
        assert!(modificateur.force_values.is_empty(), "lancer {roll_index}");
    }
}

#[test]
fn test_ghost_die_tie_breaks_on_lowest_die_id() {
    // Deux 2, et le plus petit identifiant n'est **pas** le premier rencontré :
    // départager sur l'ordre du balayage donnerait `DieId(8)`. Le gagnant,
    // `DieId(5)`, est en position 1 : rang et identifiant ne coïncident pas
    // davantage ici.
    let decor = decor_de_lancer(des_identifies(&[(8, 2), (5, 2), (4, 4), (6, 5), (9, 6)]));
    let modificateur = roll_modifier_for(RelicId::GhostDie, &decor.dice, 0);

    assert_eq!(modificateur.force_values, vec![(DieId(5), 6)]);
}

#[test]
fn test_ghost_die_forces_six_literally_not_the_max_face() {
    // La cible est la valeur 6, littéralement. Sur un D8 (ADR-008), un dé forcé
    // passe à 6 et non à 8 ; « la face maximale » serait une autre relique.
    let mut dice = des(&[2, 3, 4, 5, 7]);
    for de in &mut dice {
        de.sides = 8;
    }
    let decor = decor_de_lancer(dice);
    let modificateur = roll_modifier_for(RelicId::GhostDie, &decor.dice, 0);

    assert_eq!(modificateur.force_values, vec![(DieId(0), 6)]);
}

#[test]
fn test_ghost_die_has_no_reroll_delta() {
    for (valeurs, roll_index) in [
        (vec![2, 3, 4, 5, 6], 0),
        (vec![1, 3, 4, 5, 6], 0),
        (vec![2, 3, 4, 5, 6], 1),
    ] {
        let decor = decor_de_lancer(des(&valeurs));
        let modificateur = roll_modifier_for(RelicId::GhostDie, &decor.dice, roll_index);
        assert_eq!(
            modificateur.reroll_delta, 0,
            "{valeurs:?} au lancer {roll_index}"
        );
    }
}

// ---- *Tirelire en Terre*, l'or et l'avancement d'état (TASK-62) ----

/// Enchaîne les mains d'une blind : un `advance_state(OnHandScored)` par main,
/// avec les relances non utilisées de chacune.
fn manche(decor: &Decor, depart: RelicState, relances: &[u8]) -> RelicState {
    relances.iter().fold(depart, |etat, restantes| {
        let ctx = TriggerCtx {
            rerolls_left: *restantes,
            ..decor.ctx(etat)
        };
        advance_state(RelicId::ClayPiggyBank, Hook::OnHandScored, &ctx, etat)
    })
}

fn or_a(decor: &Decor, etat: RelicState) -> u32 {
    gold_for(RelicId::ClayPiggyBank, &decor.ctx(etat))
}

#[test]
fn test_clay_piggy_bank_on_round_end() {
    let decor = Decor::new();

    // Quatre mains, 2 + 1 + 0 + 2 relances non utilisées.
    let apres_manche = manche(&decor, RelicState::None, &[2, 1, 0, 2]);
    assert_eq!(apres_manche, RelicState::Counter(5));
    assert_eq!(or_a(&decor, apres_manche), 5);

    let ctx = decor.ctx(apres_manche);
    assert_eq!(
        advance_state(RelicId::ClayPiggyBank, Hook::OnRoundEnd, &ctx, apres_manche),
        RelicState::Counter(0),
        "jamais RelicState::None : la variante reste stable d'une blind à l'autre"
    );

    // Second cas : le plafond mord. Sept cumulés rendent cinq.
    let au_dessus = manche(&decor, RelicState::None, &[3, 2, 2]);
    assert_eq!(au_dessus, RelicState::Counter(7));
    assert_eq!(or_a(&decor, au_dessus), 5);
}

#[test]
fn test_piggy_cap_boundary() {
    // Le plafond mord à partir de six et ne rogne rien en deçà.
    let decor = Decor::new();
    for (compteur, attendu) in [(4, 4), (5, 5), (6, 5)] {
        assert_eq!(or_a(&decor, RelicState::Counter(compteur)), attendu);
    }
}

#[test]
fn test_piggy_counter_starts_from_none() {
    // `add_relic` rend `None` : le premier avancement doit le lire comme zéro,
    // pas paniquer et pas repartir d'une valeur arbitraire.
    let decor = Decor::new();
    assert_eq!(
        manche(&decor, RelicState::None, &[3]),
        RelicState::Counter(3)
    );
}

#[test]
fn test_piggy_resets_between_rounds() {
    let decor = Decor::new();
    let premiere = manche(&decor, RelicState::None, &[2, 1, 0, 2]);
    assert_eq!(or_a(&decor, premiere), 5);

    let ctx = decor.ctx(premiere);
    let remis = advance_state(RelicId::ClayPiggyBank, Hook::OnRoundEnd, &ctx, premiere);
    let seconde = manche(&decor, remis, &[1, 1, 1, 1]);

    assert_eq!(seconde, RelicState::Counter(4), "rien ne franchit la blind");
    assert_eq!(or_a(&decor, seconde), 4);
}

#[test]
fn test_piggy_is_silent_on_the_two_other_hooks() {
    // Elle n'a que deux déclencheurs. Sur les deux autres, l'état passe tel
    // quel : sans ce test, un bras attrape-tout qui remettrait à zéro sur
    // `OnRoll` viderait la tirelire à chaque relance.
    let decor = Decor::new();
    for hook in [Hook::OnRoll, Hook::OnScoringDie] {
        for etat in ETATS {
            let ctx = decor.ctx(etat);
            assert_eq!(
                advance_state(RelicId::ClayPiggyBank, hook, &ctx, etat),
                etat,
                "{hook:?} sur {etat:?}"
            );
        }
    }
}

#[test]
fn test_piggy_advance_reads_the_state_argument_not_the_context() {
    // **Le contrat du § 2.3 : l'argument `state` fait foi.** Les deux sont
    // censés désigner la même chose, donc seul un contexte délibérément
    // désaccordé peut dire lequel est lu.
    let decor = Decor::new();
    let ctx = TriggerCtx {
        rerolls_left: 1,
        ..decor.ctx(RelicState::Counter(100))
    };
    assert_eq!(
        advance_state(
            RelicId::ClayPiggyBank,
            Hook::OnHandScored,
            &ctx,
            RelicState::Counter(2)
        ),
        RelicState::Counter(3)
    );
}

#[test]
fn test_gold_is_zero_for_other_relics() {
    let decor = Decor::new();
    for def in SANS_OR_NI_MEMOIRE {
        for etat in ETATS {
            assert_eq!(gold_for(def, &decor.ctx(etat)), 0, "{def:?} sur {etat:?}");
        }
    }
}

#[test]
fn test_effects_for_is_pure_and_piggy_stays_silent() {
    // **L'assertion « `ctx.state` inchangé » du ticket n'est pas exprimable** :
    // `effects_for` reçoit un `&TriggerCtx` et `state` est un champ `Copy` lu
    // par valeur. La muter ne compile pas, donc un test qui l'affirme ne peut
    // pas échouer. Ce qui se mesure, c'est la pureté — deux appels identiques
    // rendent deux listes égales — et le silence définitif de la Tirelire.
    let decor = Decor::new();
    for hook in HOOKS {
        for etat in ETATS {
            let ctx = decor.ctx(etat);
            assert!(
                effects_for(RelicId::ClayPiggyBank, hook, &ctx).is_empty(),
                "{hook:?} sur {etat:?}"
            );
            for def in CATALOG {
                assert_eq!(
                    effects_for(*def, hook, &ctx),
                    effects_for(*def, hook, &ctx),
                    "{def:?} n'est pas pure sur {hook:?}"
                );
            }
        }
    }
}

#[test]
fn test_round_end_gold_sums_inventory() {
    let decor = Decor::new();
    let mut inventaire = inventaire_ordonne(&[
        RelicId::ClayPiggyBank,
        RelicId::TripletMaster,
        RelicId::ClayPiggyBank,
    ]);
    inventaire.slots[0]
        .as_mut()
        .expect("relique au slot 0")
        .state = RelicState::Counter(5);
    inventaire.slots[1]
        .as_mut()
        .expect("relique au slot 1")
        .state = RelicState::Disabled;
    inventaire.slots[2]
        .as_mut()
        .expect("relique au slot 2")
        .state = RelicState::Counter(3);

    assert_eq!(
        round_end_gold(&inventaire, &decor.ctx(RelicState::None)),
        8,
        "le slot éteint et les slots vides ne versent rien"
    );
}

#[test]
fn test_round_end_gold_reads_each_slot_own_state() {
    // Le contexte de base porte `Counter(5)` ; si la somme le lisait au lieu
    // de l'état de chaque slot, deux tirelires vides rendraient dix.
    let decor = Decor::new();
    let inventaire = inventaire_ordonne(&[RelicId::ClayPiggyBank, RelicId::ClayPiggyBank]);

    assert_eq!(
        round_end_gold(&inventaire, &decor.ctx(RelicState::Counter(5))),
        0
    );
}

#[test]
fn test_round_end_gold_saturates() {
    // Cinq tirelires pleines ne débordent pas un u32, mais la somme est écrite
    // en saturation : ce test fige le choix plutôt que de le laisser au hasard
    // de la capacité d'inventaire.
    let decor = Decor::new();
    let mut inventaire = RelicInventory::new(5);
    for _ in 0..5 {
        inventaire
            .add_relic(RelicId::ClayPiggyBank)
            .expect("slot libre");
    }
    for slot in inventaire.slots.iter_mut().flatten() {
        slot.state = RelicState::Counter(u32::MAX);
    }

    assert_eq!(
        round_end_gold(&inventaire, &decor.ctx(RelicState::None)),
        25
    );
}

// ---- L'invariant de budget de puissance (TASK-63) ----

/// Budgets du § 4.2 du glossaire, en unités de compte. **1 Mult ≡ 10 Chips.**
const BUDGET_COMMUNE: u64 = 40;
const BUDGET_PEU_COMMUNE: u64 = 80;

/// Facteur de dépassement maximal accordé à une relique conditionnelle.
const PLAFOND_CONDITIONNEL: u64 = 3;

/// Les cinq variantes d'état de la matrice : les quatre du type, `Counter`
/// comptant double pour séparer le compteur vide du compteur plein.
const ETATS_BUDGET: [RelicState; 5] = [
    RelicState::None,
    RelicState::Counter(0),
    RelicState::Counter(7),
    RelicState::Perishable { rounds_left: 2 },
    RelicState::Disabled,
];

/// Un point de la matrice, avec de quoi nommer le contexte fautif.
struct ScenarioBudget<'a> {
    ctx: TriggerCtx<'a>,
    figure: YahtzeeHand,
    valeur_de_de: u8,
    hook: Hook,
    etat: RelicState,
    somme_paire: bool,
}

/// Le produit cartésien sur lequel l'invariant s'évalue.
///
/// **Six axes, et non cinq.** Le sixième — la parité de la somme des dés
/// comptabilisés — manquait au ticket, alors qu'il est l'axe de déclenchement
/// du *Balancier* : faire varier `ctx.die` ne le touche pas, puisqu'il lit
/// `ctx.hand.scoring_dice` à travers `ctx.dice`. Sans cet axe, une relique
/// Commune gardée par une somme impaire traverserait toute la matrice sans être
/// vue, ce qui est exactement le défaut que le ticket décrit sur l'axe des
/// figures.
///
/// `left_effects` est non vide et porte une multiplication : une relique qui
/// ré-émettrait naïvement la tranche de sa voisine est prise.
fn pour_chaque_contexte(mut visiter: impl FnMut(&ScenarioBudget<'_>)) {
    let niveaux = HandLevels::default();
    let manche = blind_nu();
    let gauche = [ScoreEffect {
        source: StepSource::Relic {
            uid: 99,
            def: RelicId::UnstableObsidian,
        },
        action: ScoreAction::MultiplyMult(200),
    }];

    for (valeurs, somme_paire) in [([2u8, 2, 2, 2, 2], true), ([1, 2, 2, 2, 2], false)] {
        let dice = des(&valeurs);
        let identifiants: Vec<DieId> = dice.iter().map(|de| de.id).collect();

        for figure in YahtzeeHand::ALL {
            let main = HandMatch {
                hand: figure,
                scoring_dice: identifiants.clone(),
                discarded_dice: Vec::new(),
                potential_score: 0,
            };

            for valeur_de_de in 1u8..=8 {
                for hook in HOOKS {
                    for etat in ETATS_BUDGET {
                        let ctx = TriggerCtx {
                            hand: &main,
                            dice: &dice,
                            hand_levels: &niveaux,
                            blind: &manche,
                            uid: 1,
                            slot: 0,
                            state: etat,
                            // Le dé courant n'existe que sur son propre
                            // déclencheur : le poser ailleurs testerait un
                            // contexte que le pipeline ne construit jamais.
                            die: (hook == Hook::OnScoringDie)
                                .then_some((identifiants[0], valeur_de_de)),
                            base_chips: 30,
                            base_mult: 400,
                            left_effects: &gauche,
                            roll_index: 0,
                            rerolls_left: 2,
                        };
                        visiter(&ScenarioBudget {
                            ctx,
                            figure,
                            valeur_de_de,
                            hook,
                            etat,
                            somme_paire,
                        });
                    }
                }
            }
        }
    }
}

fn multiplie(def: RelicId, cas: &ScenarioBudget<'_>) -> bool {
    effects_for(def, cas.hook, &cas.ctx)
        .iter()
        .any(|effet| matches!(effet.action, ScoreAction::MultiplyMult(_)))
}

#[test]
fn test_rarity_budget_invariant() {
    // **Ce test ne doit jamais être assoupli.** Si une valeur ne tient pas dans
    // le budget, c'est la valeur qui change. Une liste blanche, un `if def !=`,
    // et l'Étape 9 équilibre soixante reliques à l'aveugle.
    assert_eq!(
        CATALOG.len(),
        12,
        "le catalogue reste à douze jusqu'à l'Étape 9"
    );

    for def in CATALOG {
        let rarete = rarity_of(*def);
        if !matches!(rarete, RelicRarity::Common | RelicRarity::Uncommon) {
            continue;
        }
        pour_chaque_contexte(|cas| {
            assert!(
                !multiplie(*def, cas),
                "{def:?} ({rarete:?}) multiplie le Mult — figure {:?}, dé {}, {:?}, {:?}, somme {}",
                cas.figure,
                cas.valeur_de_de,
                cas.hook,
                cas.etat,
                if cas.somme_paire { "paire" } else { "impaire" }
            );
        });
    }
}

#[test]
fn test_budget_matrix_reaches_every_rare() {
    // **« Au moins un `MultiplyMult` » ne prouve rien.** *Obsidienne Instable*
    // est inconditionnelle : elle en émet dans chacun des milliers de contextes,
    // donc une matrice qui n'atteindrait aucune branche conditionnelle passerait
    // quand même. Ce qui se mesure, c'est que **chacune** des quatre Rares est
    // atteinte : le *Balancier* par la parité, le *Yams Divin* par la figure, le
    // *Miroir* par la tranche gauche, l'*Obsidienne* sans condition.
    let emetteurs: Vec<RelicId> = CATALOG
        .iter()
        .copied()
        .filter(|def| {
            let mut vu = false;
            pour_chaque_contexte(|cas| vu |= multiplie(*def, cas));
            vu
        })
        .collect();

    assert_eq!(
        emetteurs,
        vec![
            RelicId::Pendulum,
            RelicId::UnstableObsidian,
            RelicId::DivineYahtzee,
            RelicId::DoubleMirror,
        ]
    );
}

#[test]
fn test_multiply_mult_emitters_are_rare_or_legendary() {
    for def in [
        RelicId::Pendulum,
        RelicId::UnstableObsidian,
        RelicId::DivineYahtzee,
        RelicId::DoubleMirror,
    ] {
        assert!(
            matches!(rarity_of(def), RelicRarity::Rare | RelicRarity::Legendary),
            "{def:?} multiplie sans être Rare ni Légendaire"
        );
    }
}

/// Unités de compte d'une action. **1 Mult ≡ 10 Chips**, et le Mult est en
/// centièmes : `AddMult(100)` vaut dix unités.
///
/// `MultiplyMult` rend zéro : une multiplication n'est pas une quantité
/// additive, son budget est une autre échelle (§ 2.1).
fn unites(action: ScoreAction) -> u64 {
    match action {
        ScoreAction::AddChips(chips) => chips,
        ScoreAction::AddMult(mult) => u64::try_from(mult).unwrap_or(0) / 10,
        ScoreAction::MultiplyMult(_) => 0,
    }
}

fn unites_de(effets: &[ScoreEffect]) -> u64 {
    effets.iter().map(|effet| unites(effet.action)).sum()
}

/// Somme des unités sur les six faces et les cinq dés comptabilisés, **sans
/// jamais diviser**.
///
/// L'espérance vaut ce total sur six, et `2,5` dés impairs n'est pas un entier.
/// Plutôt que d'introduire un flottant — que ce projet interdit — on compare le
/// total à six fois le budget : tout reste entier et exact.
fn total_sur_les_six_faces(def: RelicId) -> u64 {
    let dice = des(&[1, 1, 1, 1, 1]);
    let main = HandMatch {
        hand: YahtzeeHand::Chance,
        scoring_dice: dice.iter().map(|de| de.id).collect(),
        discarded_dice: Vec::new(),
        potential_score: 0,
    };
    let niveaux = HandLevels::default();
    let manche = blind_nu();
    let base = TriggerCtx {
        hand: &main,
        dice: &dice,
        hand_levels: &niveaux,
        blind: &manche,
        uid: 1,
        slot: 0,
        state: RelicState::None,
        die: None,
        base_chips: 30,
        base_mult: 400,
        left_effects: &[],
        roll_index: 0,
        rerolls_left: 0,
    };

    (1u8..=6)
        .map(|valeur| {
            dice.iter()
                .map(|de| {
                    let ctx = TriggerCtx {
                        die: Some((de.id, valeur)),
                        ..base
                    };
                    unites_de(&effects_for(def, Hook::OnScoringDie, &ctx))
                })
                .sum::<u64>()
        })
        .sum()
}

/// Unités brutes émises par une relique sur la figure qui la déclenche.
fn brut_sur_figure(def: RelicId, figure: YahtzeeHand) -> u64 {
    let decor = Decor::avec_figure(figure);
    unites_de(&effects_for(
        def,
        Hook::OnHandScored,
        &decor.ctx(RelicState::None),
    ))
}

#[test]
fn test_common_relics_within_forty_units() {
    // Trois faces impaires x un Mult x cinq dés : l'espérance est de 25 unités,
    // soit 63 % d'un budget Commune de 40.
    assert_eq!(total_sur_les_six_faces(RelicId::CrackedDie), 150);
    assert!(total_sur_les_six_faces(RelicId::CrackedDie) <= 6 * BUDGET_COMMUNE);

    assert_eq!(total_sur_les_six_faces(RelicId::PolishedStone), 150);
    assert!(total_sur_les_six_faces(RelicId::PolishedStone) <= 6 * BUDGET_COMMUNE);
}

#[test]
fn test_uncommon_per_die_within_eighty_units() {
    // Une seule face sur six, quinze Chips, cinq dés : 12,5 unités d'espérance,
    // 16 % d'un budget Peu commune de 80.
    assert_eq!(total_sur_les_six_faces(RelicId::PyramidOfSixes), 75);
    assert!(total_sur_les_six_faces(RelicId::PyramidOfSixes) <= 6 * BUDGET_PEU_COMMUNE);
}

/// Les cinq reliques conditionnelles du § 2.4, avec le dénominateur de leur
/// fréquence de déclenchement et le facteur de dépassement que la
/// documentation leur accorde.
const CONDITIONNELLES: [(RelicId, u64, u64); 5] = [
    (RelicId::TripletMaster, 3, 3),
    (RelicId::FullHouseArchitect, 6, 3),
    (RelicId::StellarAlignment, 6, 3),
    (RelicId::Pendulum, 2, 2),
    (RelicId::DivineYahtzee, 20, 3),
];

#[test]
fn test_conditional_cap_never_exceeds_three() {
    // **La moitié documentaire de ce test n'en est pas une.** Le ticket demande
    // de vérifier que le `//!` « nomme `f` et le plafond » : un test ne lit pas
    // une documentation à l'exécution, et y parvenir demanderait de fouiller de
    // la prose française — le défaut que ce corpus rejoue depuis TASK-59. La
    // présence du tableau est une garde de CI ; ce qui se teste ici est la
    // règle numérique, qui échoue si quelqu'un écrit un plafond à quatre.
    for (def, denominateur, facteur) in CONDITIONNELLES {
        assert_eq!(
            facteur,
            denominateur.min(PLAFOND_CONDITIONNEL),
            "{def:?} : le facteur documenté ne découle pas de sa fréquence"
        );
        assert!(facteur <= PLAFOND_CONDITIONNEL, "{def:?}");
    }
}

#[test]
fn test_additive_conditionals_stay_under_their_cap() {
    // Les trois conditionnelles additives, mesurées sur la figure qui les
    // déclenche, contre le plafond que leur rareté et leur fréquence leur
    // accordent. C'est la seule moitié du budget qui se compare en unités : les
    // deux conditionnelles multiplicatives vivent sur l'autre échelle.
    for (def, figure, brut, budget) in [
        (
            RelicId::TripletMaster,
            YahtzeeHand::ThreeOfAKind,
            60,
            BUDGET_COMMUNE,
        ),
        (
            RelicId::FullHouseArchitect,
            YahtzeeHand::FullHouse,
            90,
            BUDGET_PEU_COMMUNE,
        ),
        (
            RelicId::StellarAlignment,
            YahtzeeHand::SmallStraight,
            80,
            BUDGET_PEU_COMMUNE,
        ),
    ] {
        assert_eq!(brut_sur_figure(def, figure), brut, "{def:?}");

        let facteur = CONDITIONNELLES
            .iter()
            .find(|(candidate, _, _)| *candidate == def)
            .map(|(_, _, facteur)| *facteur)
            .expect("relique conditionnelle tabulée");
        assert!(
            brut <= budget * facteur,
            "{def:?} : {brut} u dépasse le plafond de {} u",
            budget * facteur
        );
    }
}

// ---- *La Cage* : neutralisation lue depuis la manche (TASK-74) ----

/// Manche mettant un slot en cage.
fn manche_en_cage(slot: u8) -> BlindContext {
    BlindContext {
        blind: BlindDefinition {
            modifier: Some(BlindModifier::DisableRelicSlot(slot)),
            ..BlindDefinition::default()
        },
        ..blind_nu()
    }
}

/// Combien de paliers une relique donnée a émis. **Ancré sur la relique et non
/// sur l'uid** : `add_relic` numérote, et le test ne doit pas dépendre de sa
/// convention.
fn emis(rapport: &core_engine::scoring::ScoringReport, def: RelicId) -> usize {
    paliers_de_relique(rapport)
        .iter()
        .filter(|(_, d, _)| *d == def)
        .count()
}

#[test]
fn test_cage_neutralises_target_slot() {
    let (dice, main) = brelan_de_quatre();
    let inventaire = inventaire_ordonne(&[RelicId::TripletMaster, RelicId::PolishedStone]);

    let libre = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &blind_nu(),
    );
    let en_cage = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &manche_en_cage(0),
    );

    assert!(
        emis(&libre, RelicId::TripletMaster) > 0,
        "le Maître du Brelan n'émettait déjà rien : le montage ne prouve rien"
    );
    assert_eq!(
        emis(&en_cage, RelicId::TripletMaster),
        0,
        "le slot en cage a émis"
    );
    assert_eq!(
        emis(&en_cage, RelicId::PolishedStone),
        emis(&libre, RelicId::PolishedStone),
        "le voisin a été touché"
    );
    assert_ne!(en_cage.final_score, libre.final_score);
}

#[test]
fn test_cage_empties_left_effects_of_neighbour() {
    // *Miroir Double* ré-émet la tranche de son voisin de gauche. Derrière une
    // relique en cage il n'hérite de rien, exactement comme derrière un slot
    // `Disabled` : c'est la remise à zéro de `left` qui le donne, et elle n'a
    // lieu que si la garde précède la construction du contexte.
    let (dice, main) = brelan_de_quatre();
    let inventaire = inventaire_ordonne(&[RelicId::TripletMaster, RelicId::DoubleMirror]);

    let en_cage = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &manche_en_cage(0),
    );
    assert!(
        paliers_de_relique(&en_cage).is_empty(),
        "le Miroir a hérité d'une tranche non vide : {:?}",
        paliers_de_relique(&en_cage)
    );

    // **Le cas qui discrimine vraiment, et il faut trois slots.** Une cage
    // posée sur le **premier** slot ne prouve rien : `left` y est déjà vide,
    // et un parcours qui oublierait de le remettre à zéro passerait le test.
    // Il faut un slot qui a émis, puis la cage, puis le Miroir.
    let trois = inventaire_ordonne(&[
        RelicId::TripletMaster,
        RelicId::PolishedStone,
        RelicId::DoubleMirror,
    ]);
    let cage_au_milieu = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &trois,
        &manche_en_cage(1),
    );
    assert!(
        emis(&cage_au_milieu, RelicId::TripletMaster) > 0,
        "le slot 0 n'émettait rien : le montage ne prouve rien"
    );
    assert_eq!(
        emis(&cage_au_milieu, RelicId::DoubleMirror),
        0,
        "le Miroir a hérité de la tranche du slot 0 par-dessus la cage"
    );

    // Et une cage posée sur le Miroir ne touche pas son voisin de gauche.
    let miroir_en_cage = ScoringPipeline::resolve(
        &main,
        &dice,
        &HandLevels::default(),
        &inventaire,
        &manche_en_cage(1),
    );
    let sources: Vec<RelicId> = paliers_de_relique(&miroir_en_cage)
        .iter()
        .map(|(_, def, _)| *def)
        .collect();
    assert!(
        !sources.is_empty() && sources.iter().all(|def| *def == RelicId::TripletMaster),
        "une cage sur le slot 1 a touché le slot 0 : {sources:?}"
    );
}
