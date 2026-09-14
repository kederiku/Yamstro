//! Comportement des reliques : `effects_for`, et les trois fixtures de test.
//!
//! **Arbitrage sur l'emplacement des fixtures.** Les variantes de fixture de
//! `RelicId` et leurs bras sont sous `#[cfg(test)]`, or un item `#[cfg(test)]`
//! de `src/` est invisible depuis `tests/*.rs` : une cible d'intégration se lie
//! à la crate compilée sans `cfg(test)`. Tout test qui nomme une fixture est
//! donc un test unitaire de ce fichier, jamais un test d'intégration.
//!
//! **La feature de développement `test-fixtures` est écartée**, sans
//! dérogation : elle exigerait une dépendance de développement de la crate sur
//! elle-même, rendrait les fixtures compilables hors `cfg(test)`, et créerait
//! un piège durable avec `--all-features`.
//!
//! # Nidification du parcours, normative
//!
//! `OnScoringDie` : pour chaque dé comptabilisé **dans l'ordre**, puis pour
//! chaque slot **de gauche à droite**. `OnHandScored` : **un seul passage** sur
//! les slots, de gauche à droite.
//!
//! **Aucun tri par type d'action** (ADR-005). Regrouper les additions avant les
//! multiplications changerait le score : deux reliques dans un ordre rendent
//! 264, dans l'autre 198.
//!
//! **Un slot vide ou désactivé ne produit rien et transmet une tranche vide** à
//! son voisin de droite. C'est ce qui rend les boss de désactivation
//! déterministes **sans branche particulière** : le voisin ne teste pas si sa
//! gauche est désactivée, il lit une tranche vide.

use smallvec::SmallVec;

use crate::dice::Die;
use crate::relics::{RelicId, RelicState, definitions};
use crate::scoring::{Hook, RollModifier, ScoreEffect, TriggerCtx};

// Ces deux types ne sont nommés que par les bras de fixture, tous sous
// `#[cfg(test)]`. L'Étape 5 retirera ce `cfg` en même temps qu'elle versera les
// douze reliques de production.
#[cfg(test)]
use crate::hands::YahtzeeHand;
#[cfg(test)]
use crate::scoring::{ScoreAction, StepSource};

/// Ce qu'une relique produit à un déclencheur donné, et rien d'autre.
///
/// Fonction libre et statique : c'est elle qui tient le rôle qu'un objet à
/// répartition dynamique tiendrait ailleurs (ADR-002). Ajouter une relique,
/// c'est ajouter une variante à `RelicId` et un bras ici ; le compilateur
/// signale alors celles qu'on a oubliées, ce qu'une table de fonctions ne fait
/// pas.
///
/// **Pure.** Elle ne mute rien, ne lit aucune source d'aléa ni aucune horloge,
/// et ne fait avancer aucun `RelicState` : cet avancement appartient à
/// `advance_state` (Étape 5), seul écrivain de l'état (ADR-010). Une relique
/// interrogée sur un déclencheur qu'elle n'écoute pas rend une liste vide,
/// jamais une panique.
///
/// Hors `cfg(test)`, `RelicId` est **inhabité** : les trois fixtures sont ses
/// seules variantes et le `match` se réduit à zéro bras. C'est voulu. Le
/// remplacer par un retour vide inconditionnel masquerait la non-exhaustivité
/// dès que l'Étape 5 versera le catalogue.
pub fn effects_for(def: RelicId, hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]> {
    // Hors test, aucun bras ne subsiste, donc rien ne lit le contexte. Le
    // `match` consomme `def` et `hook` de lui-même ; seul ce paramètre reste à
    // signaler comme délibérément inutilisé.
    #[cfg(not(test))]
    let _ = ctx;

    match (def, hook) {
        // *Feu de Six* : dix Chips par dé comptabilisé montrant un 6. Le
        // contexte porte une `Option` et rien n'interdit un appel sans dé, d'où
        // la lecture par motif.
        #[cfg(test)]
        (RelicId::SixFire, Hook::OnScoringDie) => {
            let mut effects = SmallVec::new();
            if let Some((_, value)) = ctx.die
                && value == 6
            {
                effects.push(ScoreEffect {
                    source: StepSource::Relic { uid: ctx.uid, def },
                    action: ScoreAction::AddChips(10),
                });
            }
            effects
        }

        // *Paire Magique* : quatre Mult sur les deux figures qui reposent sur
        // une répétition partielle.
        #[cfg(test)]
        (RelicId::MagicPair, Hook::OnHandScored) => {
            let mut effects = SmallVec::new();
            if matches!(
                ctx.hand.hand,
                YahtzeeHand::ThreeOfAKind | YahtzeeHand::FullHouse
            ) {
                effects.push(ScoreEffect {
                    source: StepSource::Relic { uid: ctx.uid, def },
                    action: ScoreAction::AddMult(400),
                });
            }
            effects
        }

        // *Verre Brisé* : un Mult et demi, sans condition.
        #[cfg(test)]
        (RelicId::BrokenGlass, Hook::OnHandScored) => {
            let mut effects = SmallVec::new();
            effects.push(ScoreEffect {
                source: StepSource::Relic { uid: ctx.uid, def },
                action: ScoreAction::MultiplyMult(150),
            });
            effects
        }

        // **Les douze reliques de production n'ont pas encore de comportement,
        // et cela s'écrit ici, nommément.** Rendre un `SmallVec` vide n'est pas
        // un comportement, c'est son absence : TASK-56 remplacera ces noms un à
        // un. Ce qui compte est qu'ils soient **énumérés** : un joker `_ =>`
        // ferait compiler une treizième relique oubliée, et à soixante
        // reliques le compilateur est le seul contrôle qui tienne encore.
        // C'est l'argument que le corpus emploie pour interdire le `_ =>` dans
        // `rarity_of` ; il vaut ici mot pour mot.
        // Délégation : les valeurs de score vivent dans le module de la
        // relique, jamais ici. Le `match` reste une table lisible sur douze
        // bras, et le hook est filtré par la relique elle-même.
        (RelicId::CrackedDie, _) => definitions::cracked_die::effects(hook, ctx),
        (RelicId::PolishedStone, _) => definitions::polished_stone::effects(hook, ctx),
        (RelicId::PyramidOfSixes, _) => definitions::pyramid_of_sixes::effects(hook, ctx),
        (RelicId::TripletMaster, _) => definitions::triplet_master::effects(hook, ctx),
        (RelicId::FullHouseArchitect, _) => definitions::full_house_architect::effects(hook, ctx),
        (RelicId::StellarAlignment, _) => definitions::stellar_alignment::effects(hook, ctx),
        (RelicId::Pendulum, _) => definitions::pendulum::effects(hook, ctx),
        (RelicId::UnstableObsidian, _) => definitions::unstable_obsidian::effects(hook, ctx),
        (RelicId::DivineYahtzee, _) => definitions::divine_yahtzee::effects(hook, ctx),

        (RelicId::DoubleMirror, _) => definitions::double_mirror::effects(hook, ctx),

        (RelicId::ClayPiggyBank | RelicId::GhostDie, _) => SmallVec::new(),

        // Les trois fixtures sont énumérées de même : le joker ne porte que sur
        // le déclencheur.
        #[cfg(test)]
        (RelicId::SixFire | RelicId::MagicPair | RelicId::BrokenGlass, _) => SmallVec::new(),
    }
}

/// Ce qu'une relique change au lancer. **Deux la changent, dix non.**
///
/// `hook` n'entre pas dans la signature : le hook est `OnRoll` par
/// construction. Il n'existe donc pas de « neutralité sur les autres hooks » à
/// éprouver ici — la question ne se pose qu'à `effects_for`, qui reçoit le
/// hook.
///
/// # Pourquoi pas un `TriggerCtx`
///
/// **Il n'en existe aucun au moment du lancer.** `TriggerCtx` exige un
/// `&HandMatch`, et le montage de manche réinitialise le contexte de main :
/// à la première manche d'une run, aucune figure n'a jamais été évaluée. Le
/// fabriquer en sentinelle rejouerait le piège que `ScoringStepQueue::default`
/// documente.
///
/// La signature porte donc exactement ce que les deux reliques concernées
/// lisent : les dés et le rang du lancer. Elle était épinglée par une garde de
/// CI écrite quand la fonction n'avait **aucun appelant** ; le premier appelant
/// réel l'a contredite.
///
/// La valeur neutre est le `Default` dérivé, jamais un littéral réécrit à la
/// main.
pub fn roll_modifier_for(def: RelicId, dice: &[Die], roll_index: u8) -> RollModifier {
    match def {
        RelicId::UnstableObsidian => definitions::unstable_obsidian::roll_modifier(),
        RelicId::GhostDie => definitions::ghost_die::roll_modifier(dice, roll_index),

        // Définitif : ces dix ne toucheront jamais au lancer. Plus rien n'est
        // étiqueté dans cette fonction, les deux reliques concernées étant
        // livrées ; le groupe reste énuméré pour que l'ajout d'une treizième
        // variante fasse échouer la compilation ici aussi.
        RelicId::CrackedDie
        | RelicId::PolishedStone
        | RelicId::TripletMaster
        | RelicId::FullHouseArchitect
        | RelicId::StellarAlignment
        | RelicId::PyramidOfSixes
        | RelicId::Pendulum
        | RelicId::DivineYahtzee
        | RelicId::ClayPiggyBank
        | RelicId::DoubleMirror => RollModifier::default(),

        // Définitif : aucune fixture ne gagnera de comportement au lancer.
        #[cfg(test)]
        RelicId::SixFire | RelicId::MagicPair | RelicId::BrokenGlass => RollModifier::default(),
    }
}

/// L'or qu'une relique rapporte. **Squelette : toutes à zéro.**
pub fn gold_for(def: RelicId, ctx: &TriggerCtx) -> u32 {
    match def {
        RelicId::ClayPiggyBank => definitions::clay_piggy_bank::gold(ctx),

        // Définitif : ces reliques ne rapportent pas d'or.
        RelicId::CrackedDie
        | RelicId::PolishedStone
        | RelicId::TripletMaster
        | RelicId::FullHouseArchitect
        | RelicId::StellarAlignment
        | RelicId::PyramidOfSixes
        | RelicId::Pendulum
        | RelicId::UnstableObsidian
        | RelicId::DivineYahtzee
        | RelicId::GhostDie
        | RelicId::DoubleMirror => 0,

        // **Or indépendant de l'état, et c'est tout son objet.** Toute relique
        // de production lit son propre `RelicState` pour calculer son or, si
        // bien qu'éteinte elle rend zéro d'elle-même : le filtre sur `Disabled`
        // de `round_end_gold` et le bras de la relique concourent alors au même
        // silence, et le filtre n'est gardé par rien. Cette fixture les sépare.
        #[cfg(test)]
        RelicId::BrokenGlass => 3,
        #[cfg(test)]
        RelicId::SixFire | RelicId::MagicPair => 0,
    }
}

/// Fait avancer l'état d'une relique. **Squelette : identité partout.**
///
/// **Seul écrivain de `RelicState`** (ADR-010) : `effects_for` reste pure et ne
/// fait avancer aucun état. L'état est pris **par valeur** et le nouvel état
/// rendu ; cette fonction n'écrit pas dans l'inventaire, l'appelant s'en charge.
///
/// Elle n'est appelée **ni depuis le commit du score**, dont le corps est
/// normatif et reste minimal, ni depuis le pipeline : l'Étape 5 lui donnera deux
/// systèmes propres, ordonnés autour du commit sans le modifier (TASK-62).
pub fn advance_state(def: RelicId, hook: Hook, ctx: &TriggerCtx, state: RelicState) -> RelicState {
    match def {
        RelicId::ClayPiggyBank => definitions::clay_piggy_bank::advance(hook, ctx, state),

        // Définitif : ces reliques sont sans mémoire.
        RelicId::CrackedDie
        | RelicId::PolishedStone
        | RelicId::TripletMaster
        | RelicId::FullHouseArchitect
        | RelicId::StellarAlignment
        | RelicId::PyramidOfSixes
        | RelicId::Pendulum
        | RelicId::UnstableObsidian
        | RelicId::DivineYahtzee
        | RelicId::GhostDie
        | RelicId::DoubleMirror => state,

        #[cfg(test)]
        RelicId::SixFire | RelicId::MagicPair | RelicId::BrokenGlass => state,
    }
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::blinds::BlindContext;
    use crate::dice::{Die, DieId};
    use crate::evaluator::HandMatch;
    use crate::hands::{HandLevels, YahtzeeHand};
    use crate::relics::definitions::rarity_of;
    use crate::relics::{CATALOG, RelicId, RelicRarity, RelicState};
    use crate::scoring::{Hook, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

    const UID: u32 = 7;

    /// Porte les valeurs que `TriggerCtx` emprunte. Sans ce propriétaire, les
    /// références du contexte ne survivraient pas à l'expression qui les crée.
    struct Fixture {
        hand: HandMatch,
        dice: Vec<Die>,
        hand_levels: HandLevels,
        blind: BlindContext,
        left_effects: Vec<ScoreEffect>,
    }

    impl Fixture {
        fn new(hand: YahtzeeHand) -> Self {
            Self {
                hand: HandMatch {
                    hand,
                    scoring_dice: vec![DieId(0), DieId(1), DieId(2)],
                    discarded_dice: vec![DieId(3), DieId(4)],
                    potential_score: 30,
                },
                dice: (0..5).map(|index| Die::new(DieId(index), 6)).collect(),
                hand_levels: HandLevels::default(),
                blind: BlindContext::de_test(None),
                left_effects: Vec::new(),
            }
        }

        fn ctx(&self, die: Option<(DieId, u8)>) -> TriggerCtx<'_> {
            TriggerCtx {
                hand: &self.hand,
                dice: &self.dice,
                hand_levels: &self.hand_levels,
                blind: &self.blind,
                uid: UID,
                slot: 3,
                state: RelicState::Counter(2),
                die,
                base_chips: 30,
                base_mult: 400,
                left_effects: &self.left_effects,
                roll_index: 0,
                rerolls_left: 0,
            }
        }
    }

    fn source(def: RelicId) -> StepSource {
        StepSource::Relic { uid: UID, def }
    }

    #[test]
    fn test_fixtures_absent_from_catalog() {
        // Étape 6 : étendre au tirage de boutique.
        //
        // `CATALOG` porte douze entrées depuis TASK-53 : l'assertion est
        // devenue **substantielle**, là où elle était trivialement vraie sur un
        // catalogue vide. Elle est restée une boucle `contains` plutôt qu'un
        // `assert!(CATALOG.is_empty())` précisément pour survivre à ce
        // remplissage. L'invariant ne repose pas sur ce test mais sur le fait
        // que le pool de boutique **dérive** de `CATALOG`.
        for fixture in [RelicId::SixFire, RelicId::MagicPair, RelicId::BrokenGlass] {
            assert!(!CATALOG.contains(&fixture));
        }
    }

    #[test]
    fn test_six_fire_ignores_non_six() {
        // La valeur 8 n'est pas un détail : les gobelets polyédriques de
        // TASK-09 posent des dés à huit faces, et un seuil écrit `>= 6` au lieu
        // de `== 6` déclencherait dessus. Sans ce cas, les deux formulations
        // sont indiscernables.
        let fixture = Fixture::new(YahtzeeHand::ThreeOfAKind);

        for value in [1, 3, 5, 7, 8] {
            let ctx = fixture.ctx(Some((DieId(0), value)));

            let effects = effects_for(RelicId::SixFire, Hook::OnScoringDie, &ctx);

            assert!(effects.is_empty(), "valeur {value}");
        }
    }

    #[test]
    fn test_six_fire_fires_on_six() {
        let fixture = Fixture::new(YahtzeeHand::ThreeOfAKind);
        let ctx = fixture.ctx(Some((DieId(0), 6)));

        let effects = effects_for(RelicId::SixFire, Hook::OnScoringDie, &ctx);

        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].action, ScoreAction::AddChips(10));
        assert_eq!(effects[0].source, source(RelicId::SixFire));
    }

    #[test]
    fn test_six_fire_without_die_does_not_panic() {
        // Rien n'interdit structurellement un appel sans dé : le contexte
        // porte une `Option`, et la lecture passe par un motif, jamais par un
        // dépliage qui paniquerait.
        let fixture = Fixture::new(YahtzeeHand::ThreeOfAKind);
        let ctx = fixture.ctx(None);

        let effects = effects_for(RelicId::SixFire, Hook::OnScoringDie, &ctx);

        assert!(effects.is_empty());
    }

    #[test]
    fn test_magic_pair_silent_on_large_straight() {
        let fixture = Fixture::new(YahtzeeHand::LargeStraight);
        let ctx = fixture.ctx(None);

        let effects = effects_for(RelicId::MagicPair, Hook::OnHandScored, &ctx);

        assert!(effects.is_empty());
    }

    #[test]
    fn test_magic_pair_fires_on_three_of_a_kind_and_full_house() {
        for hand in [YahtzeeHand::ThreeOfAKind, YahtzeeHand::FullHouse] {
            let fixture = Fixture::new(hand);
            let ctx = fixture.ctx(None);

            let effects = effects_for(RelicId::MagicPair, Hook::OnHandScored, &ctx);

            assert_eq!(effects.len(), 1, "figure {hand:?}");
            assert_eq!(
                effects[0].action,
                ScoreAction::AddMult(400),
                "figure {hand:?}"
            );
            assert_eq!(effects[0].source, source(RelicId::MagicPair));
        }
    }

    #[test]
    fn test_broken_glass_always_one_effect() {
        for hand in YahtzeeHand::ALL {
            let fixture = Fixture::new(hand);
            let ctx = fixture.ctx(None);

            let effects = effects_for(RelicId::BrokenGlass, Hook::OnHandScored, &ctx);

            assert_eq!(effects.len(), 1, "figure {hand:?}");
            assert_eq!(
                effects[0].action,
                ScoreAction::MultiplyMult(150),
                "figure {hand:?}"
            );
        }
    }

    #[test]
    fn test_wrong_hook_yields_empty() {
        let fixture = Fixture::new(YahtzeeHand::FullHouse);
        let ctx = fixture.ctx(Some((DieId(0), 6)));

        for (def, hook) in [
            (RelicId::SixFire, Hook::OnHandScored),
            (RelicId::MagicPair, Hook::OnScoringDie),
            (RelicId::BrokenGlass, Hook::OnScoringDie),
        ] {
            assert!(effects_for(def, hook, &ctx).is_empty(), "{def:?} {hook:?}");
        }

        // **La boucle sur le catalogue a été retirée à TASK-57.** Elle
        // affirmait que toute relique du catalogue est muette sur tout hook,
        // ce qui était vrai du squelette et cesse de l'être à la première
        // relique implémentée. La propriété résiduelle — les reliques encore
        // neutres le restent — est portée par `test_skeleton_effects_are_empty`
        // dans `tests/relics.rs`, avec **une seule** liste à maintenir. Ce test
        // retrouve donc son sujet : les trois fixtures et leurs hooks.
    }

    #[test]
    fn test_effects_for_is_pure() {
        // Ce test ne peut pas échouer sur une faute d'écriture : `ctx` est une
        // référence partagée, `state` y est pris par valeur, et la fonction n'a
        // ni source d'aléa ni état global. « Deux appels rendent deux résultats
        // égaux » est une garantie du langage, pas une propriété du code.
        // Sa seule dent est la sonde de borne ci-dessous, qui tombe si la
        // signature change ; la garde utile contre une lecture du hasard est le
        // `rg` de la DoD.
        fn exige_pur<T: Fn(RelicId, Hook, &TriggerCtx) -> SmallVec<[ScoreEffect; 2]>>(_: T) {}
        exige_pur(effects_for);

        let fixture = Fixture::new(YahtzeeHand::FullHouse);
        let ctx = fixture.ctx(Some((DieId(0), 6)));

        let premier = effects_for(RelicId::SixFire, Hook::OnScoringDie, &ctx);
        let second = effects_for(RelicId::SixFire, Hook::OnScoringDie, &ctx);

        assert_eq!(premier, second);
        assert_eq!(ctx.state, RelicState::Counter(2));
    }

    /// **L'invariant de budget ne voit pas les fixtures, et c'est ici qu'il
    /// faut le compléter.**
    ///
    /// `test_rarity_budget_invariant` vit dans `tests/relics.rs`, une cible
    /// d'intégration liée à la crate compilée **sans `cfg(test)`** : les trois
    /// fixtures n'y existent pas et n'entrent jamais dans sa matrice. Sans ce
    /// test-ci, le commentaire de `rarity_of` — « une Commune qui multiplie
    /// ferait échouer l'invariant de budget » — décrirait une garde qui
    /// n'existe nulle part.
    #[test]
    fn test_fixture_rarities_obey_the_multiply_rule() {
        assert_eq!(rarity_of(RelicId::BrokenGlass), RelicRarity::Rare);

        for def in [RelicId::SixFire, RelicId::MagicPair, RelicId::BrokenGlass] {
            for figure in YahtzeeHand::ALL {
                let fixture = Fixture::new(figure);
                for hook in [
                    Hook::OnRoll,
                    Hook::OnScoringDie,
                    Hook::OnHandScored,
                    Hook::OnRoundEnd,
                ] {
                    for valeur in 1u8..=8 {
                        let ctx = fixture.ctx(Some((DieId(0), valeur)));
                        let multiplie = effects_for(def, hook, &ctx)
                            .iter()
                            .any(|effet| matches!(effet.action, ScoreAction::MultiplyMult(_)));
                        assert!(
                            !multiplie
                                || matches!(
                                    rarity_of(def),
                                    RelicRarity::Rare | RelicRarity::Legendary
                                ),
                            "{def:?} multiplie sans être Rare ni Légendaire ({figure:?}, {hook:?})"
                        );
                    }
                }
            }
        }
    }
}
