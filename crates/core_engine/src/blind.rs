//! Contexte de blind, forme minimale.

#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;
use smallvec::SmallVec;

/// Modificateur porté par un blind. **Deux variantes seulement à cette étape** :
/// celles qui touchent la base de la figure. L'Étape 6 est propriétaire de
/// l'énumération complète et y ajoutera les treize autres, sans déplacer ce
/// fichier.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BlindModifier {
    HalveBaseScores,
    AllMultToOne,
}

/// Ce que le pipeline de score lit d'un blind, et rien de plus.
///
/// **C'est un arbitrage, pas une évidence.** `BlindContext` est un livrable de
/// l'Étape 3 et `BlindModifier` de l'Étape 6 ; les introduire ici sous forme
/// minimale est la seule façon de rendre l'Étape 2 implémentable sans
/// dépendance circulaire entre étapes. Les deux étapes propriétaires
/// l'enrichiront **là où il est**, sans le déplacer.
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::resource::Resource, bevy_reflect::Reflect),
    reflect(Component)
)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlindContext {
    pub modifiers: SmallVec<[BlindModifier; 2]>,
}

impl BlindContext {
    /// Unique lecture du pipeline. L'Étape 3 la réimplémentera sur sa propre
    /// représentation sans toucher aux appelants, ce qui suppose que personne
    /// n'atteigne le champ directement.
    pub fn has_modifier(&self, m: BlindModifier) -> bool {
        self.modifiers.contains(&m)
    }
}
