//! Boutique : l'étalage et ce qu'il contient.
//!
//! **Aucun système Bevy ici.** La génération est une fonction pure ; son site
//! d'appel, l'achat et les capacités sont l'affaire de la crate d'états.
//!
//! Les **prix** n'y sont pas non plus : ce module ne pose que le coût de
//! relance initial, que la tarification lira sans le redéclarer.

// L'import paraît inutile et ne l'est pas : `reflect(Component)` se déploie en
// une référence à `ReflectComponent`, qui doit être en portée. Même forme que
// dans `relics/inventory.rs` et `dice.rs`.
#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;

pub mod generator;
pub mod pricing;

use crate::consumables::ConsumableId;
use crate::dice::DieModifier;
use crate::hands::YahtzeeHand;
use crate::relics::RelicId;

/// Coût de la première relance d'étalage d'une visite.
///
/// **Déclaré une seule fois.** `generate_shop` le pose à chaque appel, ce qui
/// rend la remise à zéro par visite vraie **par construction** plutôt que par
/// un système qui devrait y penser. La tarification le **lit**.
///
/// Valeur de départ, à calibrer par le harnais de l'Étape 6 bis.
pub const INITIAL_REROLL_COST: u32 = 5;

/// Un article de l'étalage.
///
/// **La variante s'appelle `DieMod`.** `DiceMod` est le nom v1, proscrit par la
/// table de correspondance du glossaire.
///
/// `Reflect` seul sous la feature Bevy : ni `Component`, ni `Resource`. Un
/// article est une donnée, pas une entité.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ShopItem {
    RelicCard(RelicId),
    GridUpgrade(YahtzeeHand),
    DieMod(DieModifier),
    Consumable(ConsumableId),
}

/// L'étalage courant.
///
/// **Deux champs, pas un de plus.** Ni compteur de visites, ni horodatage, ni
/// liste de vendus : un article acheté quitte `items`, et l'or a sa source de
/// vérité dans la session.
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::resource::Resource, bevy_reflect::Reflect),
    reflect(Component)
)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ShopInventory {
    pub items: Vec<ShopItem>,
    pub reroll_cost: u32,
}

impl Default for ShopInventory {
    /// **Écrit à la main, jamais dérivé.** Un `reroll_cost` à zéro offrirait la
    /// première relance d'une run, et rien ne le signalerait.
    fn default() -> Self {
        Self {
            items: Vec::new(),
            reroll_cost: INITIAL_REROLL_COST,
        }
    }
}
