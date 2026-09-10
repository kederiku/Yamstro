//! Types atomiques du journal de score : ScoreAction, StepSource, ScoreEffect.

use crate::dice::{DieId, DieSeal};
use crate::hands::YahtzeeHand;
use crate::relics::RelicId;

/// Les trois seules mutations qu'un effet peut appliquer au score.
///
/// **Attention aux unités, elles diffèrent d'une variante à l'autre.**
/// `AddMult` prend des centièmes de Mult : `+4 Mult` s'écrit `AddMult(400)`.
/// `MultiplyMult` prend un pourcentage : `×1,50` s'écrit `MultiplyMult(150)`.
///
/// La formule `Score = Chips × Mult` est invariante et aucune variante ne la
/// remplace (ADR-004). Un effet de design énoncé « ×3 le score total » se
/// réécrit en `MultiplyMult(300)` **avant** d'entrer dans le moteur : le moteur
/// n'a pas de type pour un score total mutable, et n'en aura pas.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScoreAction {
    AddChips(u64),
    AddMult(i64),
    MultiplyMult(u32),
}

/// À quoi un effet est imputable dans le journal.
///
/// Le journal transporte une **identité**, jamais un libellé : le nom affiché
/// d'une relique vient de l'internationalisation (Étape 9), qui le dérive de
/// `def`. Un texte stocké ici ferait tomber `Copy`, donc l'égalité bon marché
/// dont dépend la comparaison de deux rapports entiers à TASK-26.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StepSource {
    HandBase { hand: YahtzeeHand },
    Die { die_id: DieId, value: u8 },
    Relic { uid: u32, def: RelicId },
    Seal { die_id: DieId, seal: DieSeal },
}

/// Produit de la passe A : une mutation et son imputation, sans mutation.
///
/// Le `Copy` n'est pas décoratif, il est structurel. C'est lui qui permet à la
/// passe A de rendre ses effets sans allocation, et à la tranche immuable des
/// effets voisins de se relire sans réclamer un second emprunt mutable. Il
/// impose que `StepSource` et `ScoreAction` restent `Copy`, donc que `RelicId`
/// reste unit-only et qu'aucun champ à allocation n'apparaisse ici : un champ
/// qui semble en exiger une n'a rien à faire dans le journal.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScoreEffect {
    pub source: StepSource,
    pub action: ScoreAction,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::{DieId, DieSeal};
    use crate::hands::YahtzeeHand;
    use crate::relics::RelicId;

    #[test]
    fn test_score_effect_is_copy() {
        // `Copy` porte toute la passe A : sans lui, plus de SmallVec sans
        // allocation ni tranche relue par Miroir Double. La contrainte
        // s'énonce donc directement, sur les trois types.
        fn exige_copy<T: Copy>() {}
        exige_copy::<ScoreAction>();
        exige_copy::<StepSource>();
        exige_copy::<ScoreEffect>();

        let effect = ScoreEffect {
            source: StepSource::HandBase {
                hand: YahtzeeHand::FullHouse,
            },
            action: ScoreAction::AddMult(400),
        };

        // Vraie liaison, et non `let _ = effect;` : le motif joker ne lie rien
        // et ne déplace donc rien, ce qui rendrait la sonde inerte.
        let copie = effect;
        assert_eq!(effect, copie);
        assert_eq!(effect.action, ScoreAction::AddMult(400));
    }

    #[test]
    fn test_step_source_equality_on_all_variants() {
        let hand_base = StepSource::HandBase {
            hand: YahtzeeHand::Yahtzee,
        };
        let die = StepSource::Die {
            die_id: DieId(3),
            value: 6,
        };
        let relic = StepSource::Relic {
            uid: 1,
            def: RelicId::SixFire,
        };
        let seal = StepSource::Seal {
            die_id: DieId(3),
            seal: DieSeal::Gold,
        };

        let toutes = [hand_base, die, relic, seal];
        for (position, left) in toutes.iter().enumerate() {
            assert_eq!(*left, toutes[position]);
            for right in &toutes[position + 1..] {
                assert_ne!(left, right);
            }
        }

        // Deux copies d'une même définition sont deux sources distinctes.
        let autre_exemplaire = StepSource::Relic {
            uid: 2,
            def: RelicId::SixFire,
        };
        assert_ne!(relic, autre_exemplaire);
    }

    #[test]
    fn test_effect_types_serde_roundtrip() {
        let actions = [
            ScoreAction::AddChips(52),
            ScoreAction::AddMult(-125),
            ScoreAction::MultiplyMult(150),
        ];
        for action in actions {
            let json = serde_json::to_string(&action).expect("sérialisation");
            let back: ScoreAction = serde_json::from_str(&json).expect("désérialisation");
            assert_eq!(action, back);
        }

        let sources = [
            StepSource::HandBase {
                hand: YahtzeeHand::LargeStraight,
            },
            StepSource::Die {
                die_id: DieId(7),
                value: 8,
            },
            StepSource::Relic {
                uid: 42,
                def: RelicId::MagicPair,
            },
            StepSource::Seal {
                die_id: DieId(7),
                seal: DieSeal::Purple,
            },
        ];
        for source in sources {
            let json = serde_json::to_string(&source).expect("sérialisation");
            let back: StepSource = serde_json::from_str(&json).expect("désérialisation");
            assert_eq!(source, back);
        }

        let effect = ScoreEffect {
            source: StepSource::Relic {
                uid: 9,
                def: RelicId::BrokenGlass,
            },
            action: ScoreAction::MultiplyMult(300),
        };
        let json = serde_json::to_string(&effect).expect("sérialisation");
        let back: ScoreEffect = serde_json::from_str(&json).expect("désérialisation");
        assert_eq!(effect, back);
    }
}
