//! Définitions des reliques : rareté maintenant, comportements à TASK-56.
//!
//! Symétrique de `cups/definitions`, qui porte le catalogue des gobelets : ce
//! module est le seul endroit du moteur où les paramètres d'une relique seront
//! écrits en dur (ADR-007). Il ne déclare **aucun sous-module** tant que les
//! douze fichiers n'existent pas.

use super::{RelicId, RelicRarity};

/// Rareté d'une relique. **Fonction totale, par exhaustivité du `match`.**
///
/// Aucun `Option`, aucune valeur par défaut, **aucun bras `_ =>`** : une
/// relique dont la rareté n'est pas déclarée doit produire une erreur `E0004`,
/// pas une `Common` silencieuse. C'est le seul contrôle qui tienne encore à
/// l'échelle des soixante reliques de l'Étape 9.
///
/// **La composition du catalogue n'est pas la loi de tirage.** Quatre Communes,
/// quatre Peu communes et quatre Rares décrivent les archétypes couverts ; les
/// taux de la boutique — 65 / 25 / 9 / 1 — sont un livrable de l'Étape 6, qui
/// lira cette fonction sur les entrées de `CATALOG`. Rien n'est pondéré ici, et
/// aucune entrée n'est dupliquée pour approcher un taux.
pub fn rarity_of(def: RelicId) -> RelicRarity {
    match def {
        RelicId::CrackedDie
        | RelicId::PolishedStone
        | RelicId::TripletMaster
        | RelicId::ClayPiggyBank => RelicRarity::Common,

        RelicId::FullHouseArchitect
        | RelicId::StellarAlignment
        | RelicId::PyramidOfSixes
        | RelicId::GhostDie => RelicRarity::Uncommon,

        RelicId::Pendulum
        | RelicId::UnstableObsidian
        | RelicId::DivineYahtzee
        | RelicId::DoubleMirror => RelicRarity::Rare,

        // Les fixtures ont leur propre rareté, sans quoi `cargo test` ne
        // compile pas. *Verre Brisé* est **Rare** et non Commune : elle émet un
        // `MultiplyMult(150)`, et une Commune qui multiplie ferait échouer
        // l'invariant de budget de rareté sur sa propre matrice.
        #[cfg(test)]
        RelicId::SixFire | RelicId::MagicPair => RelicRarity::Common,
        #[cfg(test)]
        RelicId::BrokenGlass => RelicRarity::Rare,
    }
}
