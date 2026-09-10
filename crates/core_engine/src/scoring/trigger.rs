//! Point de déclenchement des reliques : Hook, TriggerCtx.
//!
//! `TriggerCtx` est une vue **strictement en lecture seule**. Il ne porte
//! aucune référence mutable, ni aucun conteneur à mutabilité intérieure, et
//! c'est sa raison d'être : re-déclencher la relique voisine pendant une
//! itération mutable sur l'inventaire réclamait un second emprunt exclusif
//! d'un autre élément du même slice, ce qui ne compile pas. En lecture seule,
//! le problème n'existe plus.

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
    use smallvec::SmallVec;

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
        BlindContext {
            modifiers: SmallVec::new(),
        }
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
        let effects = effects();
        let ctx = base(&hand, &dice, &hand_levels, &blind, &effects);

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
        let effects = effects();
        let ctx = base(&hand, &dice, &hand_levels, &blind, &effects);

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
        let effects = effects();
        let ctx = base(&hand, &dice, &hand_levels, &blind, &effects);

        assert_eq!(ctx.left_effects.len(), 2);
        assert_eq!(ctx.left_effects, effects.as_slice());
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
