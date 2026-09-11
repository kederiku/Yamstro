//! Les deux passes du calcul de score.
//!
//! La **passe A** produit une liste ordonnée d'effets sans jamais toucher au
//! score ; la **passe B** la replie sur un `ScoreContext` et rend le journal.
//! Cette séparation est ce qui rend le journal rejouable : l'ordre de
//! production et l'ordre d'application restent distincts et vérifiables.

use smallvec::SmallVec;

use crate::blind::BlindContext;
use crate::dice::{Die, DieId, DieModifier, DieSeal};
use crate::evaluator::HandMatch;
use crate::hands::{HandLevels, YahtzeeHand};
use crate::relics::effects::effects_for;
use crate::relics::{RelicInventory, RelicState};
use crate::scoring::levels::resolved_base;
use crate::scoring::{
    Hook, ScoreAction, ScoreContext, ScoreEffect, ScoreStep, ScoringReport, StepSource, TriggerCtx,
};

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
        if !inst.participe() {
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

/// Valeurs d'attente des deux champs de lancer, **sans propriétaire**.
///
/// **Cesser de les dater.** Elles ont porté TASK-61, puis TASK-66, puis
/// TASK-67, sans être levées une seule fois : chaque ticket les a trouvées hors
/// de son périmètre, et les a repoussées d'un numéro. Un quatrième numéro ne
/// changerait rien.
///
/// La raison est structurelle. Ces constantes alimentent le prototype de la
/// passe de **score**, et `ScoringPipeline::resolve` n'a aucun paramètre de
/// lancer. Les lever voudrait dire en ajouter deux à sa signature, pour un
/// besoin qui **n'existe pas** : la seule relique qui lit `roll_index` le lit
/// dans `roll_modifier_for`, qui ne traverse jamais le pipeline.
///
/// **La condition qui leur donnera un propriétaire** : une relique dont
/// `effects_for` — et non `roll_modifier_for` — lirait `roll_index` ou
/// `rerolls_left`. Ce jour-là, et pas avant, la signature de `resolve` s'élargit
/// et ces deux constantes disparaissent. C'est cette condition qu'il faut
/// surveiller, pas un numéro de ticket.
///
/// **Le choix des valeurs inverse le mode de défaillance.** Un `roll_index` à
/// zéro rendrait **vraie** la garde entière de *Dé Fantôme* — `roll_index == 0`
/// — et ferait déclencher la relique à chaque main : une erreur de score
/// silencieuse. À `u8::MAX`, la garde est fausse, et un câblage oublié donne une
/// relique qui ne part jamais. `rerolls_left` vaut zéro pour la raison
/// symétrique : *Tirelire en Terre* accumule cette valeur, et zéro n'accumule
/// rien.
const ROLL_INDEX_NON_CABLE: u8 = u8::MAX;
const REROLLS_LEFT_NON_CABLE: u8 = 0;

/// Passe A complète telle qu'elle existe à ce stade : la base de la figure,
/// puis les dés comptabilisés et le déclencheur `OnScoringDie`.
///
/// La passe A est **pure** : elle ne touche aucun contexte de score, ne fait
/// avancer aucun état de relique, et ne fait que produire une liste ordonnée.
fn pass_a(
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
        roll_index: ROLL_INDEX_NON_CABLE,
        rerolls_left: REROLLS_LEFT_NON_CABLE,
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

/// Passe B : replie les effets de la passe A et rend le journal.
///
/// **C'est la seule fonction du moteur qui mute un `ScoreContext`.** Elle
/// émet exactement un `ScoreStep` par `ScoreEffect`, dans l'ordre reçu :
/// aucun effet n'est fusionné, aucun palier n'est ajouté « pour la
/// lisibilité », aucun n'est réordonné. Chaque pas porte l'état **après**
/// application, jamais avant.
///
/// **Le journal est rejouable.** Repartir d'un contexte neuf et appliquer les
/// actions des pas dans l'ordre reproduit `final_score` exactement. C'est ce
/// qui autorise l'animation de l'Étape 4 à dépiler le journal palier par
/// palier sans jamais recalculer : elle rejoue ce qui a été commis.
///
/// Le contexte part de `{ chips: 0, mult: 0 }`, si bien que le premier pas
/// d'un journal rend un score nul tant que le Mult n'est pas posé. Le poser à
/// cent « pour respecter le plancher » casserait l'exemple normatif : ce
/// plancher appartient aux producteurs d'effets, et un blind qui ramène tout
/// le Mult à un a besoin de descendre.
///
/// L'arithmétique n'est pas réimplémentée ici : `add_chips`, `add_mult`,
/// `multiply_mult` et `final_score` sont normatifs dans `context.rs`, et
/// `score_after` se **recalcule** à chaque pas plutôt que de s'accumuler. Une
/// accumulation serait fausse dès la première multiplication, le score étant
/// un produit réévalué et non une somme de contributions.
fn pass_b(effects: &[ScoreEffect]) -> ScoringReport {
    let mut ctx = ScoreContext::default();
    let mut steps = Vec::with_capacity(effects.len());

    for effect in effects {
        match effect.action {
            ScoreAction::AddChips(chips) => ctx.add_chips(chips),
            ScoreAction::AddMult(mult) => ctx.add_mult(mult),
            ScoreAction::MultiplyMult(percent) => ctx.multiply_mult(percent),
        }

        steps.push(ScoreStep {
            source: effect.source,
            action: effect.action,
            chips_after: ctx.chips,
            mult_after: ctx.mult,
            score_after: ctx.final_score(),
        });
    }

    ScoringReport {
        final_score: ctx.final_score(),
        chips: ctx.chips,
        mult: ctx.mult,
        steps,
    }
}

/// Point d'entrée du calcul de score.
///
/// Structure sans état : elle n'existe que pour donner un nom au pipeline, et
/// toute sa logique tient dans `resolve`.
#[derive(Debug, Clone, Copy)]
pub struct ScoringPipeline;

impl ScoringPipeline {
    /// Résout une figure et rend le score accompagné du journal qui le
    /// justifie.
    ///
    /// **`resolve` calcule et ne commet rien.** Elle ne prend aucune source
    /// d'aléa, n'écrit dans aucun état de manche et ne fait avancer aucun état
    /// de relique : une relique à compteur ressort avec le compteur qu'elle
    /// avait. Deux appels sur les mêmes entrées rendent deux rapports égaux,
    /// journal compris. Commettre le score appartient à l'Étape 4, à la fin du
    /// dépilement de la file d'animation ; le faire aussi ici produirait un
    /// double comptage systématique.
    ///
    /// Le déroulé est celui des deux passes : production ordonnée des effets,
    /// puis repliement. Rien n'est trié, dédupliqué, ni filtré entre les deux —
    /// un effet nul reste un palier, que l'Étape 4 anime comme les autres.
    pub fn resolve(
        hand: &HandMatch,
        dice: &[Die],
        hand_levels: &HandLevels,
        relics: &RelicInventory,
        ctx: &BlindContext,
    ) -> ScoringReport {
        let effects = pass_a(hand, dice, hand_levels, ctx, relics);
        pass_b(&effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blind::BlindModifier;
    use crate::dice::{Die, DieId, DieModifier, DieSeal};
    use crate::evaluator::HandMatch;
    use crate::relics::{RelicId, RelicInstance, RelicInventory, RelicState};

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
        BlindContext::de_test(None)
    }

    fn inventaire(slots: &[Option<RelicId>]) -> RelicInventory {
        let mut inventaire = RelicInventory::new(0);
        inventaire.slots = slots
            .iter()
            .enumerate()
            .map(|(index, def)| {
                def.map(|def| RelicInstance {
                    uid: index as u32 + 1,
                    def,
                    state: RelicState::None,
                })
            })
            .collect();
        inventaire
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
            let mut relics = RelicInventory::new(0);
            relics.slots = slots;

            let effects = pass_a(&hand, &dice, &HandLevels::default(), &blind_nu(), &relics);

            assert!(effets_de_relique(&effects).is_empty());
        }
    }

    // ---- Passe B : repliement des effets et journal ----

    /// Les neuf effets de l'exemple résolu du corpus, écrits à la main.
    ///
    /// **Fixture partagée avec TASK-26.** Elle est littérale et non produite
    /// par la passe A : c'est ce qui permet à
    /// `test_nine_steps_of_the_worked_example` de comparer les deux et de voir
    /// une divergence entre le corpus et le moteur, au lieu de la masquer.
    fn neuf_effets_de_lexemple() -> Vec<ScoreEffect> {
        let base = StepSource::HandBase {
            hand: YahtzeeHand::FullHouse,
        };
        let mut effects = vec![
            ScoreEffect {
                source: base,
                action: ScoreAction::AddChips(30),
            },
            ScoreEffect {
                source: base,
                action: ScoreAction::AddMult(400),
            },
        ];
        for (id, value) in [(0_u32, 5_u8), (1, 5), (2, 5), (3, 2), (4, 2)] {
            effects.push(ScoreEffect {
                source: StepSource::Die {
                    die_id: DieId(id),
                    value,
                },
                action: ScoreAction::AddChips(u64::from(value)),
            });
        }
        effects.push(ScoreEffect {
            source: StepSource::Relic {
                uid: 1,
                def: RelicId::MagicPair,
            },
            action: ScoreAction::AddMult(400),
        });
        effects.push(ScoreEffect {
            source: StepSource::Relic {
                uid: 2,
                def: RelicId::BrokenGlass,
            },
            action: ScoreAction::MultiplyMult(150),
        });
        effects
    }

    /// Les cinq colonnes de la table du § 2.3, dans l'ordre.
    const NEUF_PAS: [(u64, i64, u64); 9] = [
        (30, 0, 0),
        (30, 400, 120),
        (35, 400, 140),
        (40, 400, 160),
        (45, 400, 180),
        (47, 400, 188),
        (49, 400, 196),
        (49, 800, 392),
        (49, 1200, 588),
    ];

    #[test]
    fn test_steps_are_consistent() {
        // La passe A est **pure** : aucun `ScoreContext` n'entre dans sa
        // signature ni n'en sort. La garde était un `rg` sur ce fichier, qui
        // cesse de garder quoi que ce soit maintenant que la passe B y vit ;
        // la borne, elle, tombe si un contexte s'y invite.
        fn exige_pure<
            T: Fn(&HandMatch, &[Die], &HandLevels, &BlindContext, &RelicInventory) -> Vec<ScoreEffect>,
        >(
            _: T,
        ) {
        }
        exige_pure(pass_a);

        fn exige_repli<T: Fn(&[ScoreEffect]) -> ScoringReport>(_: T) {}
        exige_repli(pass_b);

        let effects = neuf_effets_de_lexemple();
        let report = pass_b(&effects);

        // Chaque pas porte l'état **après** son action, jamais avant.
        let mut rejeu = ScoreContext::default();
        for (position, step) in report.steps.iter().enumerate() {
            match step.action {
                ScoreAction::AddChips(n) => rejeu.add_chips(n),
                ScoreAction::AddMult(n) => rejeu.add_mult(n),
                ScoreAction::MultiplyMult(p) => rejeu.multiply_mult(p),
            }
            assert_eq!(step.chips_after, rejeu.chips, "pas {}", position + 1);
            assert_eq!(step.mult_after, rejeu.mult, "pas {}", position + 1);
            assert_eq!(
                step.score_after,
                rejeu.final_score(),
                "pas {}",
                position + 1
            );
        }

        // Rejouer le journal depuis un contexte neuf reproduit le score commis :
        // c'est ce qui autorise l'Étape 4 à animer le journal sans jamais
        // recalculer.
        assert_eq!(rejeu.final_score(), report.final_score);
        assert_eq!(rejeu.chips, report.chips);
        assert_eq!(rejeu.mult, report.mult);
    }

    #[test]
    fn test_nine_steps_of_the_worked_example() {
        let attendus = neuf_effets_de_lexemple();

        // Le moteur produit-il l'exemple du corpus ? Sans cette comparaison, le
        // repliement pourrait être juste sur une liste que la passe A ne
        // produit jamais.
        let (dice, hand) = main_pleine();
        let relics = inventaire(&[Some(RelicId::MagicPair), Some(RelicId::BrokenGlass)]);
        let produits = pass_a(&hand, &dice, &HandLevels::default(), &blind_nu(), &relics);
        assert_eq!(produits, attendus);

        let report = pass_b(&attendus);

        assert_eq!(report.steps.len(), 9);
        for (position, (chips, mult, score)) in NEUF_PAS.iter().enumerate() {
            let step = &report.steps[position];
            assert_eq!(
                step.source,
                attendus[position].source,
                "pas {}",
                position + 1
            );
            assert_eq!(
                step.action,
                attendus[position].action,
                "pas {}",
                position + 1
            );
            assert_eq!(step.chips_after, *chips, "pas {}", position + 1);
            assert_eq!(step.mult_after, *mult, "pas {}", position + 1);
            assert_eq!(step.score_after, *score, "pas {}", position + 1);
        }

        // Le premier pas vaut zéro : le Mult n'est pas encore posé et
        // `30 × 0 = 0`. Poser un plancher pour que « ça ait l'air correct »
        // casserait l'exemple et l'affichage de l'Étape 4.
        assert_eq!(report.steps[0].score_after, 0);
        assert_eq!(report.final_score, 588);
        assert_eq!(report.chips, 49);
        assert_eq!(report.mult, 1200);
    }

    #[test]
    fn test_one_step_per_effect() {
        let effects = neuf_effets_de_lexemple();

        for longueur in [1, 3, 7, effects.len()] {
            let tranche = &effects[..longueur];
            let report = pass_b(tranche);

            assert_eq!(report.steps.len(), longueur);
            // Un pas par effet **et dans le même ordre** : compter ne suffit
            // pas, une permutation ou une substitution de source passerait.
            for (step, effect) in report.steps.iter().zip(tranche) {
                assert_eq!(step.source, effect.source);
                assert_eq!(step.action, effect.action);
            }
        }
    }

    #[test]
    fn test_empty_effects_yield_empty_report() {
        let report = pass_b(&[]);

        assert!(report.steps.is_empty());
        assert_eq!(report.final_score, 0);
        assert_eq!(report.chips, 0);
        assert_eq!(report.mult, 0);
    }

    // ---- Orchestration : les deux passes derrière une signature publique ----

    /// Main d'une figure quelconque, `scoring_dice` et écartés maîtrisés.
    fn main_figure(hand: YahtzeeHand, scoring: &[u32], discarded: &[u32]) -> HandMatch {
        HandMatch {
            hand,
            scoring_dice: scoring.iter().map(|id| DieId(*id)).collect(),
            discarded_dice: discarded.iter().map(|id| DieId(*id)).collect(),
            potential_score: 0,
        }
    }

    #[test]
    fn test_nominal_three_of_a_kind_no_relic() {
        // Brelan de 4 : base (10, 200) de la table normative, plus trois faces
        // à 4. Les deux dés écartés ne versent rien.
        let dice = [de(0, 4), de(1, 4), de(2, 4), de(3, 2), de(4, 3)];
        let hand = main_figure(YahtzeeHand::ThreeOfAKind, &[0, 1, 2], &[3, 4]);
        let relics = inventaire(&[]);

        let report =
            ScoringPipeline::resolve(&hand, &dice, &HandLevels::default(), &relics, &blind_nu());

        assert_eq!(report.chips, 22);
        assert_eq!(report.mult, 200);
        assert_eq!(report.final_score, 44);
    }

    #[test]
    fn test_fixture_order_changes_score() {
        // Test central de l'Étape 2 : réordonner l'inventaire change le score.
        // C'est la preuve mécanique d'ADR-005, et le seul garde-fou contre un
        // regroupement des effets par type d'action.
        //
        // **Renommé à l'audit d'Étape 5.** Il portait le nom imposé du § 3 de
        // TASK-59, qui désigne le test d'intégration sur les reliques de
        // production — celui-ci travaille sur les fixtures. Deux homonymes dans
        // deux cibles : `cargo test <nom>` n'en désignait aucun, et l'un des
        // deux pouvait pourrir sans que personne le voie.
        let (dice, hand) = main_pleine();
        let levels = HandLevels::default();
        let blind = blind_nu();

        let dans_lordre = ScoringPipeline::resolve(
            &hand,
            &dice,
            &levels,
            &inventaire(&[Some(RelicId::MagicPair), Some(RelicId::BrokenGlass)]),
            &blind,
        );
        let inverse = ScoringPipeline::resolve(
            &hand,
            &dice,
            &levels,
            &inventaire(&[Some(RelicId::BrokenGlass), Some(RelicId::MagicPair)]),
            &blind,
        );

        // 400 → 800 → 1200 d'un côté, 400 → 600 → 1000 de l'autre.
        assert_eq!(dans_lordre.final_score, 588);
        assert_eq!(dans_lordre.mult, 1200);
        assert_eq!(inverse.final_score, 490);
        assert_eq!(inverse.mult, 1000);
        assert_ne!(dans_lordre.final_score, inverse.final_score);
    }

    #[test]
    fn test_resolve_is_deterministic() {
        let (mut dice, hand) = main_pleine();
        // Un modificateur nul : le pipeline ne filtre pas les effets sans
        // conséquence sur le score, un palier à zéro restant un palier que
        // l'Étape 4 anime comme les autres.
        dice[0].modifiers = vec![DieModifier::BonusChips(0)];
        let relics = inventaire(&[Some(RelicId::MagicPair), Some(RelicId::BrokenGlass)]);
        let levels = HandLevels::default();
        let blind = blind_nu();

        let premier = ScoringPipeline::resolve(&hand, &dice, &levels, &relics, &blind);
        let second = ScoringPipeline::resolve(&hand, &dice, &levels, &relics, &blind);

        assert!(
            premier
                .steps
                .iter()
                .any(|step| step.action == ScoreAction::AddChips(0)),
            "le palier nul a été filtré"
        );

        // Les rapports **entiers** sont comparés, journal compris : deux scores
        // égaux obtenus par deux chemins différents passeraient une simple
        // comparaison de nombres.
        assert_eq!(premier.final_score, second.final_score);
        assert_eq!(premier, second);
    }

    #[test]
    fn test_mult_floor() {
        // Les deux premiers pas sont les seuls de source `HandBase` : ils sont
        // l'étape 1. Le premier porte un Mult nul par construction, la base
        // n'étant pas encore posée ; le plancher vaut à partir du second.
        //
        // Le cas `HalveBaseScores` sur une figure à mult 100 est le seul de
        // l'étape où le plancher se joue vraiment : `resolved_base` calcule
        // `(100 + 1) / 2 = 50` avant de remonter par `.max(100)`. Sans ce
        // relèvement, ce test tombe.
        let dice = [de(0, 4), de(1, 4), de(2, 4), de(3, 2), de(4, 3)];
        let relics = inventaire(&[Some(RelicId::MagicPair), Some(RelicId::BrokenGlass)]);
        let blinds = [
            BlindContext::de_test(None),
            BlindContext::de_test(Some(BlindModifier::HalveBaseScores)),
        ];

        for blind in &blinds {
            for figure in YahtzeeHand::ALL {
                let hand = main_figure(figure, &[0, 1, 2], &[3, 4]);
                let report =
                    ScoringPipeline::resolve(&hand, &dice, &HandLevels::default(), &relics, blind);

                assert_eq!(
                    report.steps[0].source,
                    StepSource::HandBase { hand: figure }
                );
                for (position, step) in report.steps.iter().enumerate().skip(1) {
                    assert!(
                        step.mult_after >= 100,
                        "figure {figure:?}, pas {} : mult {} sous le plancher",
                        position + 1,
                        step.mult_after
                    );
                }
            }
        }
    }

    #[test]
    fn test_resolve_does_not_mutate_inputs() {
        // La pureté est tenue par le type : `resolve` ne reçoit que des
        // emprunts partagés, et aucune de ses entrées n'a de mutabilité
        // intérieure. La sonde de borne l'énonce, ce qu'une comparaison de
        // clones ne peut pas faire.
        fn exige_pure<
            T: Fn(&HandMatch, &[Die], &HandLevels, &RelicInventory, &BlindContext) -> ScoringReport,
        >(
            _: T,
        ) {
        }
        exige_pure(ScoringPipeline::resolve);

        // Les champs `current_score`, `hands_remaining` et `used_hands` que le
        // corpus voulait voir comparés ici n'existent pas : ils appartiennent
        // au contexte de manche de l'Étape 3, pas au `BlindContext` minimal de
        // l'Étape 2. La comparaison porte sur ce qui existe.
        let (dice, hand) = main_pleine();
        let relics = inventaire(&[Some(RelicId::MagicPair), None, Some(RelicId::BrokenGlass)]);
        let blind = blind_nu();
        let relics_avant = relics.clone();
        let blind_avant = blind.clone();

        ScoringPipeline::resolve(&hand, &dice, &HandLevels::default(), &relics, &blind);
        ScoringPipeline::resolve(&hand, &dice, &HandLevels::default(), &relics, &blind);

        assert_eq!(relics, relics_avant);
        assert_eq!(blind, blind_avant);
        // Aucun état de relique n'a avancé : un compteur reste où il était.
        for (avant, apres) in relics_avant.slots.iter().zip(&relics.slots) {
            assert_eq!(avant.map(|inst| inst.state), apres.map(|inst| inst.state));
        }
    }

    #[test]
    fn test_base_emits_chips_then_mult() {
        let levels = HandLevels::default();
        let blind = BlindContext::de_test(None);

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
