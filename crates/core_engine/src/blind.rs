//! Blind : sa définition, et le contexte de la manche en cours.

#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;

use crate::hands::HandGrid;

/// Modificateur porté par un blind. **Trois variantes seulement à cette étape** :
/// les deux qui touchent la base de la figure, et le plafond de relances que la
/// mise en place d'une manche consomme. L'Étape 6 est propriétaire de
/// l'énumération complète et y ajoutera les douze autres, sans déplacer ce
/// fichier.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BlindModifier {
    HalveBaseScores,
    AllMultToOne,
    /// Plafond de relances du boss L'Étau. **Un plafond, jamais un delta** : il
    /// abaisse le compte quand il est plus bas, ne le relève jamais, et le
    /// maillon relique lui étant postérieur, un bonus de relique le dépasse
    /// légitimement (`core_engine::config::effective_rerolls`).
    MaxRerolls(u8),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct BlindDefinition {
    pub kind: BlindType,
    pub target_score: u64,
    pub reward: u32,
    pub modifier: Option<BlindModifier>,
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
