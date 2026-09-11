//! Blind : sa définition, et le contexte de la manche en cours.

pub mod definitions;
pub mod scaling;

pub use scaling::target_score;

#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;

use smallvec::SmallVec;

use crate::hands::{HandGrid, YahtzeeHand};
use crate::relics::RelicRarity;

/// Contrainte portée par un blind. **Les quinze, déclarées d'un coup.**
///
/// # Pourquoi les quinze maintenant
///
/// Six servent les boss inauguraux de l'Étape 6, `TargetMultiplier` est lue dès
/// la courbe de cible, et deux sont implémentées depuis l'Étape 2. Les autres
/// attendent l'Étape 9 — qui **n'aura donc aucune variante à ajouter**, et tout
/// `match` exhaustif écrit ici restera valide sans être rouvert. Une énumération
/// de contraintes qui grossit d'étape en étape force à rouvrir chaque `match`
/// et à en oublier un.
///
/// # L'ordre est celui du glossaire, pas celui des boss
///
/// C'est la seule liste que trois documents peuvent comparer ligne à ligne. Ne
/// la réordonne pas pour grouper ce qui est implémenté.
///
/// # Ce que disent les marqueurs
///
/// `// Étape 9` signale une variante qu'aucun boss ne porte encore. **Le corpus
/// ne nomme qu'un seul boss de cette étape-là, *Le Mur* (`TargetMultiplier`,
/// ×3,00) ; pour les autres, aucun nom n'est inventé.** Le jour où l'Étape 9 les
/// nommera, c'est ce bloc qui change, pas neuf commentaires identiques.
///
/// # Il n'existe pas de variante `None`
///
/// L'absence de contrainte est le `None` de l'`Option<BlindModifier>` que porte
/// `BlindDefinition`. Deux façons d'exprimer la même chose donneraient deux
/// tests d'égalité divergents et un `match` obligé de traiter les deux.
///
/// # `Copy` a disparu, et c'est `DebuffHands` qui l'emporte
///
/// Deux variantes portent une `SmallVec`. Mesuré : le relevé des sites qui
/// copiaient la valeur en a rendu **un seul**, un helper de test. La signature
/// de `has_modifier` n'a pas bougé, ses appelants construisant une variante
/// unitaire sur place.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BlindModifier {
    /// Étape 9 — boss non encore nommé.
    DisableFace(u8),
    /// Plafond de relances du boss L'Étau. **Un plafond, jamais un delta** : il
    /// abaisse le compte quand il est plus bas, ne le relève jamais, et le
    /// maillon relique lui étant postérieur, un bonus de relique le dépasse
    /// légitimement (`core_engine::config::effective_rerolls`).
    MaxRerolls(u8),
    /// Étape 6 — La Cage.
    DisableRelicSlot(u8),
    /// Figures affaiblies. **Au pluriel, et c'est normatif** : le singulier ne
    /// pouvait pas exprimer *L'Oubli*, qui interdit Chance **et** Yams (C22).
    DebuffHands(SmallVec<[YahtzeeHand; 2]>),
    /// Étape 2 — implémenté par `resolved_base`.
    HalveBaseScores,
    /// Étape 9 — boss non encore nommé.
    HideDice(u8),
    /// Multiplicateur de cible, en pour-mille. *Le Mur* vaut `3_000`.
    TargetMultiplier(u32),
    /// Étape 9 — boss non encore nommé.
    MaxHands(u8),
    /// Étape 9 — boss non encore nommé.
    GrindLowest,
    /// Étape 9 — boss non encore nommé.
    DisableRarity(RelicRarity),
    /// Étape 9 — boss non encore nommé.
    DisableSeals,
    /// Étape 9 — boss non encore nommé.
    DisableRightmostRelic,
    /// Étape 2 — implémenté par `resolved_base`.
    AllMultToOne,
    /// Étape 9 — boss non encore nommé.
    OnlyHands(SmallVec<[YahtzeeHand; 2]>),
    /// Étape 9 — boss non encore nommé.
    GoldWipeAfter(u8),
}

/// Rang d'un blind dans l'ante.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum BlindType {
    #[default]
    Small,
    Big,
    Boss,
}

/// Définition d'un blind : ce que le catalogue en dit, indépendamment de la
/// manche en cours.
///
/// **Forme minimale.** Le catalogue appartient à l'Étape 6, qui enrichira ce
/// type **là où il est** : ni identité de boss, ni définition de boss ici.
///
/// `Default` est dérivé et c'est délibéré. Une définition par défaut est un
/// objet **inerte** — petit blind, cible nulle, aucune récompense, aucun
/// modificateur — qui sert aux montages de test. `BlindContext`, lui, ne
/// dérive pas `Default` : une manche naît d'une courbe, jamais de zéros.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct BlindDefinition {
    pub kind: BlindType,
    pub target_score: u64,
    pub reward: u32,
    pub modifier: Option<BlindModifier>,
}

impl BlindDefinition {
    /// Le slot mis en cage par *La Cage*, `None` sous toute autre manche.
    ///
    /// **Lecture seule, et site unique.** `has_modifier` ne convient pas : la
    /// variante porte une charge utile qu'il faut lire, pas comparer. Passer
    /// par un accesseur garde la règle de TASK-22 — le pipeline n'atteint
    /// jamais le champ directement — et donne un seul endroit à corriger si la
    /// forme du modificateur évolue.
    ///
    /// Il vit sur la **définition** et non sur le contexte : c'est là que le
    /// modificateur réside, et `resolve_rerolls` ne tient qu'une définition.
    /// Les formes de l'Étape 9 y résideront aussi.
    pub fn disabled_relic_slot(&self) -> Option<u8> {
        match self.modifier {
            Some(BlindModifier::DisableRelicSlot(slot)) => Some(slot),
            _ => None,
        }
    }
}

/// État de la manche en cours.
///
/// **C'est un arbitrage, pas une évidence.** Ce type est un livrable de
/// l'Étape 3 et `BlindModifier` de l'Étape 6 ; les introduire dans
/// `core_engine` est la seule façon de rendre l'Étape 2 implémentable sans
/// dépendance circulaire entre étapes. Il n'en existe donc **qu'un seul** :
/// en déclarer un second dans la crate d'états recréerait la duplication que
/// l'audit reprochait à la v1, deux structures homonymes divergeant au premier
/// champ ajouté.
///
/// `target_score` est la cible **effective** de la manche, après la courbe de
/// progression ; `BlindDefinition.target_score` est la cible **nominale** de la
/// définition. Les deux coexistent volontairement.
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::resource::Resource, bevy_reflect::Reflect),
    reflect(Component)
)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlindContext {
    pub blind: BlindDefinition,
    pub target_score: u64,
    pub current_score: u64,
    pub hands_remaining: u8,
    pub used_hands: HandGrid,
}

impl BlindContext {
    /// Unique lecture du pipeline, de signature inchangée depuis l'Étape 2.
    /// Elle portait alors sur une liste de modificateurs ; elle porte
    /// désormais sur celui de la définition. Aucun appelant n'a bougé, le
    /// champ n'ayant jamais été atteint directement.
    pub fn has_modifier(&self, m: BlindModifier) -> bool {
        self.blind.modifier == Some(m)
    }

    /// Le slot mis en cage, délégué à la définition.
    pub fn disabled_relic_slot(&self) -> Option<u8> {
        self.blind.disabled_relic_slot()
    }

    /// Manche inerte pour les montages de test **internes à cette crate**.
    ///
    /// Les huit sites de construction de l'Étape 2 passent par ici : sans ce
    /// point unique, enrichir `BlindDefinition` à l'Étape 6 obligerait à
    /// rouvrir chacun d'eux. Invisible depuis une autre crate, `cfg(test)`
    /// d'une bibliothèque ne franchissant pas la frontière de crate ; les
    /// tests de la crate d'états construisent donc leur contexte par littéral.
    #[cfg(test)]
    pub(crate) fn de_test(modifier: Option<BlindModifier>) -> Self {
        Self {
            blind: BlindDefinition {
                modifier,
                ..BlindDefinition::default()
            },
            target_score: 0,
            current_score: 0,
            hands_remaining: 0,
            used_hands: HandGrid::default(),
        }
    }
}

#[cfg(test)]
mod tests_modificateurs {
    use smallvec::smallvec;

    use super::*;
    use crate::hands::YahtzeeHand;
    use crate::relics::RelicRarity;

    /// Les quinze variantes, **dans l'ordre du glossaire § 2.5**.
    ///
    /// C'est la seule liste que trois documents peuvent comparer ligne à ligne,
    /// et l'ordre du test est celui de la déclaration : les faire diverger
    /// ferait perdre à la comparaison son seul intérêt.
    fn les_quinze() -> Vec<BlindModifier> {
        vec![
            BlindModifier::DisableFace(3),
            BlindModifier::MaxRerolls(1),
            BlindModifier::DisableRelicSlot(0),
            BlindModifier::DebuffHands(smallvec![YahtzeeHand::Chance, YahtzeeHand::Yahtzee]),
            BlindModifier::HalveBaseScores,
            BlindModifier::HideDice(2),
            BlindModifier::TargetMultiplier(3_000),
            BlindModifier::MaxHands(3),
            BlindModifier::GrindLowest,
            BlindModifier::DisableRarity(RelicRarity::Rare),
            BlindModifier::DisableSeals,
            BlindModifier::DisableRightmostRelic,
            BlindModifier::AllMultToOne,
            BlindModifier::OnlyHands(smallvec![YahtzeeHand::FullHouse]),
            BlindModifier::GoldWipeAfter(1),
        ]
    }

    #[test]
    fn test_fifteen_modifier_variants_exist() {
        // **« Un `match` exhaustif compile » n'est pas une assertion.** Si une
        // variante manquait, rien ne compilerait : c'est un échec de build, que
        // le compilateur dit mieux qu'un test. L'absence de bras attrape-tout
        // est une garde de CI, comme pour `effects_for` et `rarity_of`.
        //
        // Ce qui peut échouer à l'exécution, c'est le compte et la distinction.
        let quinze = les_quinze();
        assert_eq!(quinze.len(), 15);

        for (rang, gauche) in quinze.iter().enumerate() {
            for droite in &quinze[rang + 1..] {
                assert_ne!(gauche, droite, "deux variantes se confondent");
            }
        }
    }

    #[test]
    fn test_modifier_serde_roundtrip() {
        for variante in les_quinze() {
            let texte = serde_json::to_string(&variante).expect("sérialisable");
            let relue: BlindModifier = serde_json::from_str(&texte).expect("désérialisable");
            assert_eq!(relue, variante);
        }
    }

    #[test]
    fn test_debuff_hands_carries_two_hands() {
        // Le singulier `DebuffHand(YahtzeeHand)` ne pouvait pas exprimer
        // *L'Oubli*, qui interdit Chance **et** Yams (C22).
        //
        // **Ce test ne garde pas la capacité inline, et ne le peut pas.** Une
        // `SmallVec` déborde sur le tas : `[YahtzeeHand; 1]` porte deux figures
        // aussi bien que `[YahtzeeHand; 2]`, et l'arité est un choix
        // d'allocation, non une capacité. Mesuré — le mutant qui la réduit
        // survit à tous les tests. Seule une garde de CI sur la déclaration la
        // tient.
        let BlindModifier::DebuffHands(figures) =
            BlindModifier::DebuffHands(smallvec![YahtzeeHand::Chance, YahtzeeHand::Yahtzee])
        else {
            panic!("variante attendue");
        };
        assert_eq!(
            figures.as_slice(),
            [YahtzeeHand::Chance, YahtzeeHand::Yahtzee]
        );

        // Une seule figure reste légale : toutes les contraintes ne sont pas
        // doubles.
        let BlindModifier::DebuffHands(une) =
            BlindModifier::DebuffHands(smallvec![YahtzeeHand::Aces])
        else {
            panic!("variante attendue");
        };
        assert_eq!(une.len(), 1);
    }

    #[test]
    fn test_absence_is_option_none() {
        // **L'absence de contrainte est le `None` de l'`Option`**, jamais une
        // variante. Deux façons de dire la même chose donneraient deux tests
        // d'égalité divergents et un `match` qui doit traiter les deux.
        let manche = BlindContext::de_test(None);
        for variante in les_quinze() {
            assert!(
                !manche.has_modifier(variante),
                "un modificateur répond sur une manche nue"
            );
        }
    }

    #[test]
    fn test_blind_definition_shape() {
        for kind in [BlindType::Small, BlindType::Big, BlindType::Boss] {
            let definition = BlindDefinition {
                kind,
                target_score: 300,
                reward: 4,
                modifier: None,
            };
            assert_eq!(definition.kind, kind);
            assert_eq!(definition.target_score, 300);
            assert_eq!(definition.reward, 4);
            assert_eq!(definition.modifier, None);
        }
    }
}
