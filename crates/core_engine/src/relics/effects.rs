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

use smallvec::SmallVec;

use crate::relics::RelicId;
use crate::scoring::{Hook, ScoreEffect, TriggerCtx};

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

        // Les trois `def` sont énumérés explicitement : une quatrième variante
        // de `RelicId` ne serait couverte par rien et le compilateur le dirait.
        // Le joker ne porte que sur le déclencheur.
        #[cfg(test)]
        (RelicId::SixFire | RelicId::MagicPair | RelicId::BrokenGlass, _) => SmallVec::new(),
    }
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::blind::BlindContext;
    use crate::dice::{Die, DieId};
    use crate::evaluator::HandMatch;
    use crate::hands::{HandLevels, YahtzeeHand};
    use crate::relics::{CATALOG, RelicId, RelicState};
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
                blind: BlindContext {
                    modifiers: SmallVec::new(),
                },
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
        // `CATALOG` vaut `&[]` à cette étape : l'assertion est trivialement
        // vraie aujourd'hui. Elle est écrite en boucle `contains` et non en
        // `assert!(CATALOG.is_empty())` pour rester valide quand l'Étape 5 y
        // versera douze entrées. L'invariant ne repose pas sur ce test mais
        // sur le fait que le pool de boutique **dérive** de `CATALOG`.
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

        // Vide à cette étape, `CATALOG` étant `&[]` : cette boucle ne fait
        // aucune itération et n'affirme donc rien aujourd'hui. Elle devient
        // substantielle à l'Étape 5, quand les douze reliques de production y
        // entreront. Les trois couples ci-dessus, eux, mordent réellement.
        for def in CATALOG {
            for hook in [
                Hook::OnRoll,
                Hook::OnScoringDie,
                Hook::OnHandScored,
                Hook::OnRoundEnd,
            ] {
                assert!(effects_for(*def, hook, &ctx).is_empty());
            }
        }
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
}
