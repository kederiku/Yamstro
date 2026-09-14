//! Les deux contrats de décision.
//!
//! **C'est la boucle qui définit le contrat qu'elle consomme**, jamais
//! l'inverse : les traits vivent ici, les politiques les implémentent, et le
//! module ré-exporte les deux types de décision pour qu'une politique écrive
//! `use crate::policy::HandDecision;` sans savoir où ils sont déclarés.

pub mod greedy;
pub mod grid_aware;
pub mod random;

pub use crate::view::{HandDecision, ShopAction};

use crate::view::{HandView, ShopView};
use rand_chacha::ChaCha8Rng;
use smallvec::SmallVec;

/// Ce qui décide d'une main.
///
/// `name` rend un identifiant **stable d'une version à l'autre** : c'est lui
/// qui remplira la colonne de politique du tableau de sortie.
///
/// Le générateur passé n'est **jamais** un flux de la run : une politique qui
/// en consommerait un ferait diverger la partie elle-même selon la stratégie
/// employée, et deux campagnes de même graine ne seraient plus comparables.
pub trait Policy {
    fn name(&self) -> &'static str;
    fn decide(&mut self, view: &HandView<'_>, rng: &mut ChaCha8Rng) -> HandDecision;
}

/// Ce qui décide d'une visite en boutique. Même discipline : elle choisit, la
/// boucle applique.
pub trait ShopPolicy {
    fn name(&self) -> &'static str;
    fn decide(&mut self, view: &ShopView<'_>, rng: &mut ChaCha8Rng) -> SmallVec<[ShopAction; 4]>;
}
