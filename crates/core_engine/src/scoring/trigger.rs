//! Point de déclenchement des reliques : Hook, TriggerCtx.
//!
//! `TriggerCtx` est une vue **strictement en lecture seule**. Il ne porte
//! aucune référence mutable, ni aucun conteneur à mutabilité intérieure, et
//! c'est sa raison d'être : re-déclencher la relique voisine pendant une
//! itération mutable sur l'inventaire réclamait un second emprunt exclusif
//! d'un autre élément du même slice, ce qui ne compile pas. En lecture seule,
//! le problème n'existe plus. Les deux champs ajoutés à l'Étape 5 —
//! `roll_index` et `rerolls_left` — suivent la même règle : des copies, jamais
//! des emprunts exclusifs. Cette phrase évite délibérément d'écrire le motif
//! qu'une garde du volet 1 interdit dans ce fichier : une garde large se
//! contourne par le sens, jamais par une exception de chemin.

use crate::blind::BlindContext;
use crate::dice::{Die, DieId};
use crate::evaluator::HandMatch;
use crate::hands::HandLevels;
use crate::relics::RelicState;
use crate::scoring::ScoreEffect;

/// Le moment où une relique est consultée.
///
/// **L'Étape 2 ne consomme que `OnScoringDie` et `OnHandScored`.** Les deux
/// autres sont déclarées et non câblées : `OnRoll` reviendra à
/// `roll_modifier_for` (Étape 5), `OnRoundEnd` à `gold_for` et `advance_state`
/// (Étapes 5 et 6). Les câbler ici produirait des effets fantômes dans le
/// journal et casserait l'exemple résolu à neuf pas.
///
/// **`OnRoundEnd` est une fin de *blind*, pas une fin de main.** La distinction
/// n'est pas rhétorique et elle ne se lit pas dans le code : `RunPhase::RoundEnd`
/// est entrée après **chaque** main — c'est là que `resolve_round_outcome`
/// arbitre entre main suivante, boutique et défaite —, alors que ce hook ne se
/// déclenche qu'une fois la blind battue. Confondre les deux quadruple la
/// cadence : *Tirelire en Terre* encaisserait quatre fois et son plafond
/// deviendrait inopérant, et une relique périssable perdrait ses charges quatre
/// fois plus vite. Le vocabulaire du corpus dit « manche » pour les deux ;
/// c'est ce hook qui tranche, et les systèmes de `game_state::relics` portent
/// des noms qui disent lequel des deux ils servent.
///
/// `OnScoringDie` nomme un **moment**. Ce n'est pas le type dont il contient
/// le nom, proscrit par le glossaire parce qu'il entrerait en collision avec un
/// composant du moteur de rendu ; la garde du CI est ancrée aux limites de mot
/// pour distinguer les deux.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Hook {
    OnRoll,
    OnScoringDie,
    OnHandScored,
    OnRoundEnd,
}

/// Tout ce qu'une relique voit au moment d'être consultée, et rien de plus.
///
/// Onze champs, dont aucun n'ouvre un droit d'écriture. Le type ne donne
/// notamment **pas** accès au `ScoreContext` en cours : la passe A ignore le
/// score courant, faute de quoi l'ordre de production et l'ordre d'application
/// cesseraient d'être séparables et le journal ne serait plus rejouable.
///
/// `state` est pris par valeur, ce qui interdit par construction de faire
/// avancer l'état d'une relique depuis la passe A. `left_effects` est la
/// tranche produite par la relique du slot immédiatement à gauche, vide au
/// slot 0 ; c'est le seul canal dont *Miroir Double* (Étape 5) aura besoin.
/// `base_chips` et `base_mult` sont les valeurs postérieures à la résolution
/// de la base (niveaux puis modificateurs de blind), pour que les reliques
/// conditionnelles puissent raisonner dessus.
///
/// Aucun dérivé de Bevy ni de `serde` : les durées de vie `'a` rendent le type
/// ni sérialisable ni réfléchissable, et le sérialiser n'aurait pas de sens
/// puisque c'est une vue et non un état.
#[derive(Debug, Clone, Copy)]
pub struct TriggerCtx<'a> {
    pub hand: &'a HandMatch,
    pub dice: &'a [Die],
    pub hand_levels: &'a HandLevels,
    pub blind: &'a BlindContext,
    pub uid: u32,
    pub slot: u8,
    pub state: RelicState,
    pub die: Option<(DieId, u8)>,
    pub base_chips: u64,
    pub base_mult: i64,
    pub left_effects: &'a [ScoreEffect],
    /// Rang du lancer dans la manche. **Zéro est le premier lancer**, un la
    /// première relance. C'est la seule convention : *Dé Fantôme* n'a pas
    /// d'autre garde que `roll_index == 0`, et une convention partant de un la
    /// rendrait silencieusement inerte.
    pub roll_index: u8,
    /// Relances **non encore consommées** au moment du déclenchement. Simple
    /// lecture de l'état de la main : ce contexte n'en est pas propriétaire et
    /// ne le décrémente jamais.
    pub rerolls_left: u8,
}

/// Ce qu'une relique peut changer au lancer, rendu par le hook `OnRoll`.
///
/// **`reroll_delta` est signé, et doit le rester.** Il entre au **dernier
/// maillon** de la chaîne de l'ADR-007 — `base(gobelet) → mise → blind →
/// relique` — et y est appliqué en saturation. *Gobelet Abandonné*, qui part de
/// zéro relance, plus *Obsidienne Instable*, qui en retire une, doit rendre
/// **zéro** et non 255. Un `u8` ne saurait pas porter ce retrait.
///
/// **`force_values` adresse les dés par identité, jamais par position.** Le
/// nombre de dés varie d'un gobelet à l'autre, et un boss en retire un **en
/// cours de manche** : tout index capturé avant serait faux après.
///
/// `Reflect` seul sous la feature Bevy, et aucun `serde` : c'est une valeur de
/// retour consommée dans la frame, jamais persistée. Le `Default` dérivé **est**
/// la valeur neutre, dont TASK-56 se sert pour ses bras muets.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RollModifier {
    pub reroll_delta: i8,
    pub force_values: Vec<(DieId, u8)>,
}

impl<'a> TriggerCtx<'a> {
    /// Vue du contexte pour un dé comptabilisé donné. Onze champs interdisent
    /// un constructeur ; le pipeline pose le littéral une fois, puis dérive.
    #[must_use]
    pub fn on_scoring_die(self, die_id: DieId, value: u8) -> Self {
        Self {
            die: Some((die_id, value)),
            ..self
        }
    }

    /// Vue du contexte pour la figure entière : aucun dé particulier n'est en
    /// cause, donc `die` est remis à `None`.
    #[must_use]
    pub fn on_hand_scored(self) -> Self {
        Self { die: None, ..self }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::blind::BlindContext;
    use crate::dice::{Die, DieId};
    use crate::evaluator::HandMatch;
    use crate::hands::{HandLevels, YahtzeeHand};
    use crate::relics::{RelicId, RelicState};
    use crate::scoring::{ScoreAction, ScoreEffect, StepSource};

    fn hand_match() -> HandMatch {
        HandMatch {
            hand: YahtzeeHand::FullHouse,
            scoring_dice: vec![DieId(0), DieId(1), DieId(2), DieId(3), DieId(4)],
            discarded_dice: Vec::new(),
            potential_score: 30,
        }
    }

    fn dice() -> Vec<Die> {
        (0..5).map(|index| Die::new(DieId(index), 6)).collect()
    }

    fn blind() -> BlindContext {
        BlindContext::de_test(None)
    }

    fn effects() -> Vec<ScoreEffect> {
        vec![
            ScoreEffect {
                source: StepSource::Relic {
                    uid: 1,
                    def: RelicId::SixFire,
                },
                action: ScoreAction::AddChips(52),
            },
            ScoreEffect {
                source: StepSource::Relic {
                    uid: 1,
                    def: RelicId::SixFire,
                },
                action: ScoreAction::MultiplyMult(150),
            },
        ]
    }

    /// Contexte de référence des tests de contrat. `die` y vaut délibérément
    /// `Some` : parti de `None`, `test_die_is_none_on_hand_scored` passerait
    /// même si `on_hand_scored` rendait `self` sans rien changer.
    fn base<'a>(
        hand: &'a HandMatch,
        dice: &'a [Die],
        hand_levels: &'a HandLevels,
        blind: &'a BlindContext,
        left_effects: &'a [ScoreEffect],
    ) -> TriggerCtx<'a> {
        TriggerCtx {
            hand,
            dice,
            hand_levels,
            blind,
            uid: 7,
            slot: 3,
            state: RelicState::Counter(2),
            die: Some((DieId(9), 2)),
            base_chips: 30,
            base_mult: 400,
            left_effects,
            roll_index: 0,
            rerolls_left: 2,
        }
    }

    /// Les dix champs autres que `die`. Les cinq références sont comparées par
    /// **identité** et non par valeur : un dériveur qui substituerait une autre
    /// `HandMatch` égale passerait une comparaison de valeurs.
    fn assert_only_die_changed(before: &TriggerCtx<'_>, after: &TriggerCtx<'_>) {
        assert!(core::ptr::eq(before.hand, after.hand));
        assert!(core::ptr::eq(before.dice, after.dice));
        assert!(core::ptr::eq(before.hand_levels, after.hand_levels));
        assert!(core::ptr::eq(before.blind, after.blind));
        assert!(core::ptr::eq(before.left_effects, after.left_effects));
        assert_eq!(before.uid, after.uid);
        assert_eq!(before.slot, after.slot);
        assert_eq!(before.state, after.state);
        assert_eq!(before.base_chips, after.base_chips);
        assert_eq!(before.base_mult, after.base_mult);
    }

    #[test]
    fn test_roll_index_zero_is_first_roll() {
        // **La convention entière de `roll_index`.** Zéro est le premier lancer,
        // un la première relance. *Dé Fantôme* n'a pas d'autre garde que
        // `roll_index == 0` : une convention partant de un la rendrait
        // silencieusement inerte, et aucune erreur de compilation ne le dirait.
        let hand = hand_match();
        let dice = dice();
        let hand_levels = HandLevels::default();
        let blind = blind();
        let effets = effects();

        let premier = base(&hand, &dice, &hand_levels, &blind, &effets);
        let relance = TriggerCtx {
            roll_index: 1,
            ..premier
        };

        assert_eq!(premier.roll_index, 0);
        assert!(premier.roll_index == 0, "le premier lancer n'est pas zéro");
        assert!(
            relance.roll_index != 0,
            "une relance passe pour un premier lancer"
        );
        assert_eq!(
            premier.rerolls_left, relance.rerolls_left,
            "les deux contextes ne diffèrent que par l'index de lancer"
        );
    }

    #[test]
    fn test_roll_modifier_default_is_neutral() {
        // TASK-56 s'en sert pour ses bras neutres : un défaut non neutre
        // donnerait une relance de plus ou de moins à chaque relique muette.
        let neutre = RollModifier::default();
        assert_eq!(neutre.reroll_delta, 0);
        assert!(neutre.force_values.is_empty());
    }

    #[test]
    fn test_trigger_ctx_exposes_thirteen_fields() {
        let hand = hand_match();
        let dice = dice();
        let hand_levels = HandLevels::default();
        let blind = blind();
        let effets = effects();
        let ctx = base(&hand, &dice, &hand_levels, &blind, &effets);

        // Les onze de TASK-20, dans leur ordre, puis les deux ajoutés en queue.
        assert!(core::ptr::eq(ctx.hand, &hand));
        assert!(core::ptr::eq(ctx.dice, dice.as_slice()));
        assert!(core::ptr::eq(ctx.hand_levels, &hand_levels));
        assert!(core::ptr::eq(ctx.blind, &blind));
        assert_eq!(ctx.uid, 7);
        assert_eq!(ctx.slot, 3);
        assert_eq!(ctx.state, RelicState::Counter(2));
        assert_eq!(ctx.die, Some((DieId(9), 2)));
        assert_eq!(ctx.base_chips, 30);
        assert_eq!(ctx.base_mult, 400);
        assert!(core::ptr::eq(ctx.left_effects, effets.as_slice()));
        assert_eq!(ctx.roll_index, 0);
        assert_eq!(ctx.rerolls_left, 2);
    }

    #[test]
    fn test_derivers_carry_the_two_new_fields() {
        // **Le test que le corpus voulait, réécrit pour pouvoir échouer.** Il
        // demandait de vérifier que `rerolls_left` est en lecture seule — ce
        // que le typage garantit déjà, et qu'aucune assertion ne peut donc
        // mettre en défaut. La propriété qui, elle, peut casser : les deux
        // dériveurs propagent les deux nouveaux champs. Un `Self { die, .. }`
        // écrit sans `..self`, ou un champ remis à zéro, les perdrait en
        // silence — et *Dé Fantôme* comme *Tirelire en Terre* lisent leur
        // contexte **après** dérivation.
        let hand = hand_match();
        let dice = dice();
        let hand_levels = HandLevels::default();
        let blind = blind();
        let effets = effects();

        let ctx = TriggerCtx {
            roll_index: 3,
            rerolls_left: 5,
            ..base(&hand, &dice, &hand_levels, &blind, &effets)
        };

        for derive in [ctx.on_scoring_die(DieId(1), 6), ctx.on_hand_scored()] {
            assert_eq!(
                derive.roll_index, 3,
                "index de lancer perdu à la dérivation"
            );
            assert_eq!(derive.rerolls_left, 5, "relances restantes perdues");
        }
    }

    #[test]
    fn test_left_effects_empty_at_slot_zero() {
        // Ce test ne garde rien du comportement du moteur : il constate que le
        // littéral se construit sur une tranche vide, et qu'une tranche vide
        // est vide. La règle réelle, « le slot 0 n'a pas de voisine à
        // gauche », est tenue par le pipeline (TASK-21 et TASK-22), pas par ce
        // type, qui se contente de la transporter.
        let hand = hand_match();
        let dice = dice();
        let hand_levels = HandLevels::default();
        let blind = blind();

        let ctx = TriggerCtx {
            hand: &hand,
            dice: &dice,
            hand_levels: &hand_levels,
            blind: &blind,
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

        assert!(ctx.left_effects.is_empty());
        assert_eq!(ctx.slot, 0);
    }

    #[test]
    fn test_die_is_some_on_scoring_die() {
        fn exige_copy<T: Copy>() {}
        exige_copy::<TriggerCtx<'_>>();

        let hand = hand_match();
        let dice = dice();
        let hand_levels = HandLevels::default();
        let blind = blind();
        let effets = effects();
        let ctx = base(&hand, &dice, &hand_levels, &blind, &effets);

        let derive = ctx.on_scoring_die(DieId(3), 6);

        assert_eq!(derive.die, Some((DieId(3), 6)));
        assert_only_die_changed(&ctx, &derive);
    }

    #[test]
    fn test_die_is_none_on_hand_scored() {
        let hand = hand_match();
        let dice = dice();
        let hand_levels = HandLevels::default();
        let blind = blind();
        let effets = effects();
        let ctx = base(&hand, &dice, &hand_levels, &blind, &effets);

        // Le contexte de départ porte `Some` : c'est ce qui distingue un
        // dériveur qui remet `die` à `None` d'un dériveur inerte.
        assert!(ctx.die.is_some());

        let derive = ctx.on_hand_scored();

        assert_eq!(derive.die, None);
        assert_only_die_changed(&ctx, &derive);
    }

    #[test]
    fn test_left_effects_slice_is_readable() {
        // Comme le premier test, celui-ci ne peut pas échouer sur une faute du
        // moteur : il constate que la tranche traverse le littéral sans être
        // copiée ni réordonnée. Ce qu'il documente, c'est le canal dont
        // *Miroir Double* aura besoin à l'Étape 5.
        let hand = hand_match();
        let dice = dice();
        let hand_levels = HandLevels::default();
        let blind = blind();
        let effets = effects();
        let ctx = base(&hand, &dice, &hand_levels, &blind, &effets);

        assert_eq!(ctx.left_effects.len(), 2);
        assert_eq!(ctx.left_effects, effets.as_slice());
        assert_eq!(ctx.left_effects[0].action, ScoreAction::AddChips(52));
        assert_eq!(ctx.left_effects[1].action, ScoreAction::MultiplyMult(150));
    }

    #[test]
    fn test_hook_variants_are_distinct_and_roundtrip() {
        // `Hash` ne peut pas être exercé par un conteneur : la DoD interdit les
        // tables de hachage dans ce fichier. La borne de trait est le seul
        // énoncé possible, et elle tombe dès qu'un dérivé disparaît.
        fn exige<T: Copy + Eq + core::hash::Hash>() {}
        exige::<Hook>();

        let hooks = [
            Hook::OnRoll,
            Hook::OnScoringDie,
            Hook::OnHandScored,
            Hook::OnRoundEnd,
        ];

        for (position, left) in hooks.iter().enumerate() {
            for right in &hooks[position + 1..] {
                assert_ne!(left, right);
            }

            let json = serde_json::to_string(left).expect("sérialisation");
            let back: Hook = serde_json::from_str(&json).expect("désérialisation");
            assert_eq!(*left, back);
        }
    }
}
