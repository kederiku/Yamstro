//! Passe A : production pure des effets, sans aucune mutation.

use smallvec::SmallVec;

use crate::blind::BlindContext;
use crate::dice::{Die, DieId, DieModifier, DieSeal};
use crate::evaluator::HandMatch;
use crate::hands::{HandLevels, YahtzeeHand};
use crate::relics::effects::effects_for;
use crate::relics::{RelicInventory, RelicState};
use crate::scoring::levels::resolved_base;
use crate::scoring::{Hook, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

/// Premier segment de la passe A : la base de la figure, en deux effets.
///
/// L'ordre `AddChips` puis `AddMult` donne le même score final que l'inverse
/// mais un **journal différent** ; il est fixé par l'exemple résolu, pas par
/// l'arithmétique. C'est aussi ce qui donne au premier pas du journal un score
/// nul, le Mult n'étant pas encore posé.
///
/// Le couple ainsi calculé alimentera les champs homonymes du contexte de
/// déclenchement pour toute la suite de la résolution.
pub(crate) fn base_effects(
    hand: YahtzeeHand,
    hand_levels: &HandLevels,
    blind: &BlindContext,
) -> Vec<ScoreEffect> {
    let (base_chips, base_mult) = resolved_base(hand, hand_levels, blind);
    let source = StepSource::HandBase { hand };

    vec![
        ScoreEffect {
            source,
            action: ScoreAction::AddChips(base_chips),
        },
        ScoreEffect {
            source,
            action: ScoreAction::AddMult(base_mult),
        },
    ]
}

/// Effets produits par le sceau d'un dé.
///
/// **Aucun sceau ne produit d'effet de score à cette étape.** Les quatre bras
/// rendent une liste vide, et c'est l'Étape 9 qui les spécifiera. N'invente
/// aucune valeur ici : un « Gold vaut trois pièces » ou un « Red multiplie par
/// une fois et demie » n'existe nulle part dans le corpus et fausserait tous
/// les tests d'intégration. En conséquence, **aucun pas de source `Seal`
/// n'apparaît dans le journal de l'Étape 2**, dont l'exemple résolu compte neuf
/// pas et zéro sceau.
///
/// Le `match` est exhaustif : une cinquième variante à l'Étape 9 fera échouer
/// la compilation plutôt que de passer inaperçue.
fn seal_effects(die_id: DieId, seal: DieSeal) -> SmallVec<[ScoreEffect; 2]> {
    // Le dé est déjà identifié, mais rien ne le lit tant qu'aucun bras ne pose
    // d'effet. L'Étape 9 s'en servira pour la source `Seal`.
    let _ = die_id;

    match seal {
        DieSeal::Gold => SmallVec::new(),
        DieSeal::Red => SmallVec::new(),
        DieSeal::Blue => SmallVec::new(),
        DieSeal::Purple => SmallVec::new(),
    }
}

/// Balaie l'inventaire de gauche à droite pour un déclencheur donné.
///
/// **Une seule boucle sert les deux déclencheurs de cette étape.** L'étape 2
/// l'appelle une fois par dé comptabilisé, sur un prototype dérivé par
/// `on_scoring_die` ; l'étape 3 l'appelle une fois pour la main, sur un
/// prototype à `die: None`. Deux boucles jumelles divergeraient, et *Miroir
/// Double* se comporterait alors différemment selon le déclencheur.
///
/// Le prototype porte les six champs constants du contexte ; seuls `uid`,
/// `slot`, `state`, `left_effects` et `die` varient d'un slot à l'autre. Le
/// passer ainsi tient la fonction sous le seuil de `clippy::too_many_arguments`
/// et dit lesquels de ces champs sont invariants.
///
/// **`left_effects` est la tranche du slot immédiatement à gauche, quel qu'il
/// soit :** vide si ce voisin est absent, désactivé, ou n'a rien produit. La
/// plage est donc réassignée à **chaque** itération, y compris sur les chemins
/// qui sautent le slot. La règle vaut au caractère près pour `OnHandScored`
/// (TASK-24), sinon *Miroir Double* se comporterait différemment selon le
/// déclencheur, et les boss qui désactivent un slot deviendraient
/// silencieusement plus faibles.
fn scan_relics<O: FnMut(Hook, &TriggerCtx<'_>)>(
    proto: TriggerCtx<'_>,
    relics: &RelicInventory,
    effects: &mut Vec<ScoreEffect>,
    hook: Hook,
    observer: &mut O,
) {
    // Tranche du slot immédiatement à gauche, **quel qu'il soit**. Un voisin
    // stérile — absent, désactivé, ou n'ayant rien produit — la remet à vide :
    // la plage est donc réassignée à chaque itération, y compris sur les
    // chemins qui sautent le slot. La règle inverse, « le dernier slot ayant
    // produit », rendrait *Miroir Double* transparent à une désactivation et
    // affaiblirait silencieusement les boss qui éteignent une relique.
    let mut left: core::ops::Range<usize> = 0..0;

    for (slot, entry) in relics.slots.iter().enumerate() {
        let Some(inst) = entry else {
            left = 0..0;
            continue;
        };
        // Un slot désactivé est sauté avant même la construction du contexte :
        // le boss qui éteint une relique s'appuie exclusivement là-dessus. Les
        // autres états passent tels quels, et c'est `effects_for` qui décide ;
        // le pipeline ne fait avancer aucun état.
        if inst.state == RelicState::Disabled {
            left = 0..0;
            continue;
        }

        let slot = u8::try_from(slot).unwrap_or(u8::MAX);

        // L'emprunt partagé de `effects` s'achève au retour de `effects_for`,
        // ce qui autorise l'extension qui suit. Ne jamais garder cette tranche
        // dans une variable qui survivrait à l'extension.
        let produced = {
            let ctx = TriggerCtx {
                uid: inst.uid,
                slot,
                state: inst.state,
                left_effects: &effects[left.clone()],
                ..proto
            };
            observer(hook, &ctx);
            effects_for(inst.def, hook, &ctx)
        };

        let start = effects.len();
        effects.extend(produced);
        left = start..effects.len();
    }
}

/// Passe A complète telle qu'elle existe à ce stade : la base de la figure,
/// puis les dés comptabilisés et le déclencheur `OnScoringDie`.
///
/// La passe A est **pure** : elle ne touche aucun contexte de score, ne fait
/// avancer aucun état de relique, et ne fait que produire une liste ordonnée.
pub fn pass_a(
    hand: &HandMatch,
    dice: &[Die],
    hand_levels: &HandLevels,
    blind: &BlindContext,
    relics: &RelicInventory,
) -> Vec<ScoreEffect> {
    pass_a_with(hand, dice, hand_levels, blind, relics, &mut |_, _| {})
}

/// Variante instrumentée de [`pass_a`]. L'observateur reçoit, avant chaque
/// interrogation de relique, le déclencheur et le contexte exact qui va être
/// présenté à la relique. Le déclencheur distingue les appels de l'étape 2,
/// refaits à chaque dé, de l'appel unique de l'étape 3.
///
/// Elle existe parce qu'aucune relique de cette étape ne lit `left_effects` :
/// sans ce point d'écoute, la règle des slots stériles et la remise à zéro
/// entre deux dés ne seraient vérifiables par aucun test. La production passe
/// un observateur inerte.
fn pass_a_with<O: FnMut(Hook, &TriggerCtx<'_>)>(
    hand: &HandMatch,
    dice: &[Die],
    hand_levels: &HandLevels,
    blind: &BlindContext,
    relics: &RelicInventory,
    observer: &mut O,
) -> Vec<ScoreEffect> {
    let mut effects = base_effects(hand.hand, hand_levels, blind);
    // Second appel à une fonction pure, et non un report du couple calculé par
    // `base_effects` : ce sont les mêmes valeurs, et les faire circuler
    // ajouterait un paramètre sans rien garantir de plus.
    let (base_chips, base_mult) = resolved_base(hand.hand, hand_levels, blind);

    // Prototype des champs que le balayage ne fait jamais varier. Seuls `uid`,
    // `slot`, `state`, `left_effects` et `die` changent d'un appel à l'autre.
    let proto = TriggerCtx {
        hand,
        dice,
        hand_levels,
        blind,
        uid: 0,
        slot: 0,
        state: RelicState::None,
        die: None,
        base_chips,
        base_mult,
        left_effects: &[],
    };

    for die_id in &hand.scoring_dice {
        // Recherche linéaire, jamais un index : le pool retire et ajoute des
        // dés en cours de manche, et un identifiant introuvable est ignoré
        // sans panique.
        let Some(die) = dice.iter().find(|candidate| candidate.id == *die_id) else {
            continue;
        };
        let die_id = *die_id;
        let value = die.current_value;
        let source = StepSource::Die { die_id, value };

        effects.push(ScoreEffect {
            source,
            action: ScoreAction::AddChips(u64::from(value)),
        });

        // Les modificateurs suivent l'ordre de leur liste et gardent la source
        // du dé : il n'existe pas de variante de source pour un modificateur.
        for modifier in &die.modifiers {
            let action = match modifier {
                DieModifier::BonusChips(chips) => ScoreAction::AddChips(*chips),
                DieModifier::BonusMult(mult) => ScoreAction::AddMult(*mult),
            };
            effects.push(ScoreEffect { source, action });
        }

        if let Some(seal) = die.seal {
            effects.extend(seal_effects(die_id, seal));
        }

        // L'inventaire est interrogé après le dé entier, pas entre ses effets :
        // c'est ce qui donne un bonus de relique par dé rencontré, à sa place
        // dans le journal, au lieu d'un groupe en fin d'étape.
        let sur_ce_de = proto.on_scoring_die(die_id, value);
        scan_relics(
            sur_ce_de,
            relics,
            &mut effects,
            Hook::OnScoringDie,
            observer,
        );
    }

    // Étape 3 : l'inventaire est balayé **une fois pour la main**, dans l'ordre
    // strict des slots. Aucun regroupement par type d'action : placer une
    // relique multiplicative avant ou après une relique additive doit changer
    // le score, et c'est la seule décision de construction que le jeu laisse au
    // joueur (ADR-005). Le canal `left_effects` repart d'une tranche vide : ce
    // balayage ne voit pas ce que l'étape 2 a produit.
    scan_relics(
        proto.on_hand_scored(),
        relics,
        &mut effects,
        Hook::OnHandScored,
        observer,
    );

    effects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::{Die, DieId, DieModifier, DieSeal};
    use crate::evaluator::HandMatch;
    use crate::relics::{RelicId, RelicInstance, RelicInventory, RelicState};
    use smallvec::SmallVec;

    /// Journal des tranches `left_effects` vues par chaque slot, dans l'ordre
    /// des appels. C'est le seul moyen d'observer un canal qu'aucune fixture ne
    /// lit, sans ajouter de quatrième fixture qui devrait muter.
    /// Ce que l'observateur retient de chaque contexte présenté à une relique.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Vu {
        hook: Hook,
        slot: u8,
        die: Option<(DieId, u8)>,
        left: Vec<ScoreEffect>,
    }

    type Journal = Vec<Vu>;

    fn observateur(journal: &mut Journal) -> impl FnMut(Hook, &TriggerCtx<'_>) + '_ {
        move |hook, ctx| {
            journal.push(Vu {
                hook,
                slot: ctx.slot,
                die: ctx.die,
                left: ctx.left_effects.to_vec(),
            });
        }
    }

    /// Les seules entrées du journal produites par un déclencheur donné.
    fn sur(journal: &Journal, hook: Hook) -> Vec<Vu> {
        journal
            .iter()
            .filter(|vu| vu.hook == hook)
            .cloned()
            .collect()
    }

    fn de(id: u32, value: u8) -> Die {
        let mut die = Die::new(DieId(id), 6);
        die.current_value = value;
        die
    }

    fn main_de(scoring: &[u32], discarded: &[u32]) -> HandMatch {
        HandMatch {
            hand: YahtzeeHand::FullHouse,
            scoring_dice: scoring.iter().map(|id| DieId(*id)).collect(),
            discarded_dice: discarded.iter().map(|id| DieId(*id)).collect(),
            potential_score: 30,
        }
    }

    fn blind_nu() -> BlindContext {
        BlindContext {
            modifiers: SmallVec::new(),
        }
    }

    fn inventaire(slots: &[Option<RelicId>]) -> RelicInventory {
        RelicInventory {
            slots: slots
                .iter()
                .enumerate()
                .map(|(index, def)| {
                    def.map(|def| RelicInstance {
                        uid: index as u32 + 1,
                        def,
                        state: RelicState::None,
                    })
                })
                .collect(),
        }
    }

    /// Effets de dé et de relique seulement : les deux effets de base de
    /// TASK-22 ouvrent toujours le journal et ne sont pas le sujet ici.
    fn apres_la_base(effects: &[ScoreEffect]) -> Vec<ScoreEffect> {
        effects[2..].to_vec()
    }

    #[test]
    fn test_dice_emit_face_values_in_slice_order() {
        let dice = [de(0, 5), de(1, 5), de(2, 5), de(3, 2), de(4, 2)];
        let hand = main_de(&[0, 1, 2, 3, 4], &[]);
        let levels = HandLevels::default();
        let blind = blind_nu();
        let relics = inventaire(&[]);

        let effects = pass_a(&hand, &dice, &levels, &blind, &relics);

        let suite = apres_la_base(&effects);
        assert_eq!(suite.len(), 5);
        for (position, attendu) in [5_u8, 5, 5, 2, 2].iter().enumerate() {
            assert_eq!(
                suite[position].action,
                ScoreAction::AddChips(u64::from(*attendu)),
                "position {position}"
            );
            assert_eq!(
                suite[position].source,
                StepSource::Die {
                    die_id: DieId(position as u32),
                    value: *attendu
                },
                "position {position}"
            );
        }
    }

    #[test]
    fn test_six_fire_fires_once_per_six() {
        // `scoring_dice` vaut 2,2,2,6,6 : l'ordre vient de la tranche, pas de
        // la valeur des faces. Chaque bonus suit immédiatement son dé.
        let dice = [de(0, 2), de(1, 2), de(2, 2), de(3, 6), de(4, 6)];
        let hand = main_de(&[0, 1, 2, 3, 4], &[]);
        let relics = inventaire(&[Some(RelicId::SixFire)]);

        let effects = pass_a(&hand, &dice, &HandLevels::default(), &blind_nu(), &relics);
        let suite = apres_la_base(&effects);

        let bonus: Vec<usize> = suite
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.action == ScoreAction::AddChips(10)
                    && e.source
                        == StepSource::Relic {
                            uid: 1,
                            def: RelicId::SixFire,
                        }
            })
            .map(|(position, _)| position)
            .collect();

        assert_eq!(bonus.len(), 2, "un bonus par 6 comptabilisé");
        // Le bonus suit immédiatement l'effet du dé qui l'a déclenché, et n'est
        // donc jamais groupé en fin d'étape.
        for position in &bonus {
            assert_eq!(
                suite[position - 1].source,
                StepSource::Die {
                    die_id: DieId(if *position == 4 { 3 } else { 4 }),
                    value: 6
                }
            );
        }
        assert_eq!(bonus, vec![4, 6]);
    }

    #[test]
    fn test_die_modifier_follows_face_value() {
        let mut die = de(0, 3);
        die.modifiers = vec![DieModifier::BonusChips(7)];
        let dice = [die];
        let hand = main_de(&[0], &[]);

        let effects = pass_a(
            &hand,
            &dice,
            &HandLevels::default(),
            &blind_nu(),
            &inventaire(&[]),
        );
        let suite = apres_la_base(&effects);

        assert_eq!(suite.len(), 2);
        assert_eq!(suite[0].action, ScoreAction::AddChips(3));
        assert_eq!(suite[1].action, ScoreAction::AddChips(7));
        let source = StepSource::Die {
            die_id: DieId(0),
            value: 3,
        };
        assert_eq!(suite[0].source, source);
        assert_eq!(suite[1].source, source);
    }

    #[test]
    fn test_die_modifiers_keep_vec_order() {
        let mut die = de(0, 4);
        die.modifiers = vec![
            DieModifier::BonusChips(3),
            DieModifier::BonusMult(50),
            DieModifier::BonusChips(1),
        ];
        let dice = [die];
        let hand = main_de(&[0], &[]);

        let effects = pass_a(
            &hand,
            &dice,
            &HandLevels::default(),
            &blind_nu(),
            &inventaire(&[]),
        );
        let suite = apres_la_base(&effects);

        assert_eq!(suite.len(), 4);
        assert_eq!(suite[1].action, ScoreAction::AddChips(3));
        assert_eq!(suite[2].action, ScoreAction::AddMult(50));
        assert_eq!(suite[3].action, ScoreAction::AddChips(1));
    }

    #[test]
    fn test_all_seals_yield_no_effect() {
        // Les effets de sceau sont spécifiés à l'Étape 9 : à l'Étape 2, les
        // quatre bras rendent vide et aucun pas de source Seal n'existe.
        for seal in [DieSeal::Gold, DieSeal::Red, DieSeal::Blue, DieSeal::Purple] {
            assert!(seal_effects(DieId(0), seal).is_empty(), "sceau {seal:?}");
        }

        let mut die = de(0, 6);
        die.seal = Some(DieSeal::Gold);
        let dice = [die];
        let hand = main_de(&[0], &[]);

        let effects = pass_a(
            &hand,
            &dice,
            &HandLevels::default(),
            &blind_nu(),
            &inventaire(&[Some(RelicId::SixFire)]),
        );

        assert!(
            !effects
                .iter()
                .any(|e| matches!(e.source, StepSource::Seal { .. }))
        );

        // Vérifier l'absence de source `Seal` ne suffit pas : un bras qui
        // inventerait un effet en le rangeant sous la source du dé y
        // échapperait. Le journal d'un dé scellé est donc comparé à celui du
        // même dé sans sceau, et les deux doivent coïncider exactement.
        let nu = [de(0, 6)];
        let sans_sceau = pass_a(
            &hand,
            &nu,
            &HandLevels::default(),
            &blind_nu(),
            &inventaire(&[Some(RelicId::SixFire)]),
        );
        assert_eq!(effects, sans_sceau);
    }

    #[test]
    fn test_left_effects_empty_for_first_slot() {
        let dice = [de(0, 6)];
        let hand = main_de(&[0], &[]);
        let relics = inventaire(&[Some(RelicId::SixFire)]);
        let mut journal = Journal::new();

        pass_a_with(
            &hand,
            &dice,
            &HandLevels::default(),
            &blind_nu(),
            &relics,
            &mut observateur(&mut journal),
        );

        let vus = sur(&journal, Hook::OnScoringDie);
        assert_eq!(vus.len(), 1);
        assert_eq!(vus[0].slot, 0);
        assert!(vus[0].left.is_empty());
    }

    #[test]
    fn test_sterile_slot_resets_left_effects() {
        // Un slot stérile réinitialise le canal, et il y a **trois** façons de
        // l'être : absent, désactivé, ou présent mais improductif. Le troisième
        // cas est le seul que distingue une garde `is_empty()` posée avant la
        // réassignation ; sans lui, cette faute passe inaperçue. Règle partagée
        // au caractère près avec TASK-24.
        let dice = [de(0, 6)];
        let hand = main_de(&[0], &[]);
        let levels = HandLevels::default();
        let blind = blind_nu();

        for (cas, voisin) in [
            ("absent", None),
            ("désactivé", Some(RelicId::SixFire)),
            ("improductif", Some(RelicId::MagicPair)),
        ] {
            let mut relics = inventaire(&[Some(RelicId::SixFire), voisin, Some(RelicId::SixFire)]);
            if cas == "désactivé"
                && let Some(inst) = relics.slots[1].as_mut()
            {
                inst.state = RelicState::Disabled;
            }

            let mut journal = Journal::new();
            pass_a_with(
                &hand,
                &dice,
                &levels,
                &blind,
                &relics,
                &mut observateur(&mut journal),
            );

            let vus = sur(&journal, Hook::OnScoringDie);
            let vu = vus
                .iter()
                .find(|vu| vu.slot == 2)
                .unwrap_or_else(|| panic!("le slot 2 n'a pas été interrogé, cas {cas}"));
            assert!(
                vu.left.is_empty(),
                "voisin {cas} : le canal n'a pas été réinitialisé, {:?}",
                vu.left
            );
        }
    }

    #[test]
    fn test_left_effects_reset_between_dice() {
        // Deux 6, deux reliques adjacentes : au second dé, le slot 0 doit
        // repartir d'un canal vide.
        let dice = [de(0, 6), de(1, 6)];
        let hand = main_de(&[0, 1], &[]);
        let relics = inventaire(&[Some(RelicId::SixFire), Some(RelicId::SixFire)]);
        let mut journal = Journal::new();

        pass_a_with(
            &hand,
            &dice,
            &HandLevels::default(),
            &blind_nu(),
            &relics,
            &mut observateur(&mut journal),
        );

        let vus = sur(&journal, Hook::OnScoringDie);
        assert_eq!(vus.len(), 4);
        assert!(vus[0].left.is_empty(), "dé 1, slot 0");
        assert_eq!(vus[1].left.len(), 1, "dé 1, slot 1 voit son voisin");
        assert_eq!(vus[2].slot, 0);
        assert!(
            vus[2].left.is_empty(),
            "dé 2, slot 0 : le canal repart à vide"
        );
    }

    #[test]
    fn test_discarded_six_produces_nothing() {
        let dice = [de(0, 2), de(1, 6)];
        let hand = main_de(&[0], &[1]);
        let relics = inventaire(&[Some(RelicId::SixFire)]);

        let effects = pass_a(&hand, &dice, &HandLevels::default(), &blind_nu(), &relics);
        let suite = apres_la_base(&effects);

        assert_eq!(suite.len(), 1);
        assert_eq!(suite[0].action, ScoreAction::AddChips(2));
        assert!(!suite.iter().any(|e| e.action == ScoreAction::AddChips(10)));
    }

    use crate::blind::BlindContext;
    use crate::hands::{HandLevels, YahtzeeHand};
    use crate::scoring::{ScoreAction, StepSource};
    use smallvec::smallvec;

    // ---- Étape 3 : les reliques déclenchées une fois pour la main ----

    /// Effets de relique du journal, dans l'ordre, sans les effets de dé.
    fn effets_de_relique(effects: &[ScoreEffect]) -> Vec<ScoreAction> {
        effects
            .iter()
            .filter(|e| matches!(e.source, StepSource::Relic { .. }))
            .map(|e| e.action)
            .collect()
    }

    /// Main pleine sans aucun 6 : *Feu de Six* reste muet, donc seuls les
    /// déclenchements de l'étape 3 produisent des effets de relique.
    fn main_pleine() -> ([Die; 5], HandMatch) {
        (
            [de(0, 5), de(1, 5), de(2, 5), de(3, 2), de(4, 2)],
            main_de(&[0, 1, 2, 3, 4], &[]),
        )
    }

    #[test]
    fn test_relic_order_is_inventory_order() {
        let (dice, hand) = main_pleine();
        let relics = inventaire(&[Some(RelicId::MagicPair), Some(RelicId::BrokenGlass)]);

        let effects = pass_a(&hand, &dice, &HandLevels::default(), &blind_nu(), &relics);

        // L'ordre est celui de l'inventaire, jamais un regroupement par type
        // d'action : c'est ce qui fait qu'un réordonnancement change le score.
        assert_eq!(
            effets_de_relique(&effects),
            vec![ScoreAction::AddMult(400), ScoreAction::MultiplyMult(150)]
        );
        let queue = &effects[effects.len() - 2..];
        assert_eq!(queue[0].action, ScoreAction::AddMult(400));
        assert_eq!(queue[1].action, ScoreAction::MultiplyMult(150));
    }

    #[test]
    fn test_relic_reversed_order_reverses_effects() {
        let (dice, hand) = main_pleine();
        let relics = inventaire(&[Some(RelicId::BrokenGlass), Some(RelicId::MagicPair)]);

        let effects = pass_a(&hand, &dice, &HandLevels::default(), &blind_nu(), &relics);

        assert_eq!(
            effets_de_relique(&effects),
            vec![ScoreAction::MultiplyMult(150), ScoreAction::AddMult(400)]
        );
    }

    #[test]
    fn test_disabled_relic_produces_nothing() {
        let (dice, hand) = main_pleine();
        let mut relics = inventaire(&[Some(RelicId::MagicPair), Some(RelicId::BrokenGlass)]);
        if let Some(inst) = relics.slots[0].as_mut() {
            inst.state = RelicState::Disabled;
        }

        let effects = pass_a(&hand, &dice, &HandLevels::default(), &blind_nu(), &relics);

        // L'instance reste dans l'inventaire : le pipeline ne la retire pas et
        // ne la remplace pas par un slot vide, il la saute.
        assert_eq!(relics.slots.len(), 2);
        assert!(relics.slots[0].is_some());
        assert_eq!(
            effets_de_relique(&effects),
            vec![ScoreAction::MultiplyMult(150)]
        );
    }

    #[test]
    fn test_left_effects_slot_zero_is_empty() {
        let (dice, hand) = main_pleine();
        let relics = inventaire(&[Some(RelicId::MagicPair), Some(RelicId::BrokenGlass)]);
        let mut journal = Journal::new();

        pass_a_with(
            &hand,
            &dice,
            &HandLevels::default(),
            &blind_nu(),
            &relics,
            &mut observateur(&mut journal),
        );

        let vus = sur(&journal, Hook::OnHandScored);
        assert_eq!(vus.len(), 2, "un balayage unique pour la main");
        assert_eq!(vus[0].slot, 0);
        assert!(vus[0].left.is_empty());
        // `OnHandScored` se déclenche une fois pour la main : aucun dé n'est
        // désigné, et confondre les deux déclencheurs se verrait ici.
        assert!(vus.iter().all(|vu| vu.die.is_none()));
    }

    #[test]
    fn test_left_effects_slot_one_is_slot_zero() {
        let (dice, hand) = main_pleine();
        let relics = inventaire(&[Some(RelicId::MagicPair), Some(RelicId::BrokenGlass)]);
        let mut journal = Journal::new();

        pass_a_with(
            &hand,
            &dice,
            &HandLevels::default(),
            &blind_nu(),
            &relics,
            &mut observateur(&mut journal),
        );

        let vus = sur(&journal, Hook::OnHandScored);
        assert_eq!(vus[1].slot, 1);
        assert_eq!(vus[1].left.len(), 1);
        assert_eq!(vus[1].left[0].action, ScoreAction::AddMult(400));
        assert_eq!(
            vus[1].left[0].source,
            StepSource::Relic {
                uid: 1,
                def: RelicId::MagicPair
            }
        );
    }

    #[test]
    fn test_empty_inventory_produces_no_relic_effect() {
        let (dice, hand) = main_pleine();

        for slots in [vec![], vec![None, None]] {
            let relics = RelicInventory { slots };

            let effects = pass_a(&hand, &dice, &HandLevels::default(), &blind_nu(), &relics);

            assert!(effets_de_relique(&effects).is_empty());
        }
    }

    #[test]
    fn test_base_emits_chips_then_mult() {
        let levels = HandLevels::default();
        let blind = BlindContext {
            modifiers: smallvec![],
        };

        let effects = base_effects(YahtzeeHand::FullHouse, &levels, &blind);

        // L'ordre Chips puis Mult donne le même score final que l'inverse, mais
        // un journal différent : il est fixé par l'exemple normatif.
        assert_eq!(effects.len(), 2);
        assert_eq!(
            effects[0].source,
            StepSource::HandBase {
                hand: YahtzeeHand::FullHouse
            }
        );
        assert_eq!(effects[0].action, ScoreAction::AddChips(30));
        assert_eq!(
            effects[1].source,
            StepSource::HandBase {
                hand: YahtzeeHand::FullHouse
            }
        );
        assert_eq!(effects[1].action, ScoreAction::AddMult(400));
    }
}
