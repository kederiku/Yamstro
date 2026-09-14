//! Consommables : `ConsumableId`.
//!
//! # Pourquoi ici, et non sous `shop/`
//!
//! Un consommable est un objet que le joueur **détient** ; la boutique en vend,
//! comme elle vend des reliques. Le dépôt place chaque catalogue chez lui :
//! `RelicId` dans `relics/`, `CupId` dans `cups/`, `BlindType` dans `blinds/`.
//! Le déclarer sous `shop/` obligerait l'inventaire de consommables de
//! l'Étape 9 à étendre un type de boutique.
//!
//! # Une seule variante, et c'est délibéré
//!
//! Le corpus n'autorise qu'un consommable à cette étape (ADR-008). Les **sept**
//! autres runes — Mutation, Cristal, Échange, Immolation, Harmonie, Surcharge,
//! Alchimie — arrivent à l'Étape 9, **ici même**, sans que ce type soit
//! redéclaré ailleurs.

#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;

use crate::relics::RelicRarity;

/// Les consommables du jeu.
///
/// La *Rune du Destin* retire la moitié du score cible : c'est la carte la plus
/// puissante du jeu, et sa rareté, son prix et son unicité par étalage sont
/// tenus ailleurs (TASK-76 et TASK-77).
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ConsumableId {
    RuneOfFate,
}

impl ConsumableId {
    /// Le catalogue entier, dans l'ordre de déclaration.
    pub const ALL: [ConsumableId; 1] = [ConsumableId::RuneOfFate];
}

/// La rareté d'un consommable.
///
/// **Elle vit ici, avec l'identité, et non dans la tarification.** Symétrique
/// de `relics::rarity_of` : la boutique lit une rareté, elle n'en décide pas.
/// Un `match` sur l'identifiant écrit dans le barème y aurait créé un second
/// point de vérité, qui survivrait à un recalibrage de la table des prix.
///
/// C'est aussi là que les **sept autres runes** de l'Étape 9 déclareront la
/// leur, sans qu'une ligne de la tarification soit rouverte.
///
/// **Fonction totale**, comme sa jumelle : un `match` exhaustif, sans bras
/// attrape-tout. Un consommable ajouté sans rareté est une erreur de
/// compilation, jamais un prix nul silencieux.
pub fn rarity_of(id: ConsumableId) -> RelicRarity {
    match id {
        // La carte la plus puissante du jeu : elle retire la moitié du score
        // cible. Sa rareté est ce qui lui donne son prix de huit (ADR-008).
        ConsumableId::RuneOfFate => RelicRarity::Rare,
    }
}

/// Les consommables détenus par le joueur.
///
/// **Symétrique de `RelicInventory`, et minimal.** Il naît ici parce que
/// `ConsumableId` y vit, et parce que la boutique en vend dès l'Étape 6 : sans
/// lui, un quart des articles de l'étalage serait incliquable. L'usage, la
/// consommation et les sept autres runes sont l'affaire de l'Étape 9, qui
/// l'étendra **sur place**.
///
/// `slots` est dimensionné depuis `RunConfig.consumable_capacity`, jamais d'un
/// littéral, comme l'inventaire de reliques l'est depuis `relic_capacity`
/// (ADR-007). **Aucun `Default` n'est dérivé** : il donnerait zéro slot, ce qui
/// n'est jamais la capacité voulue et que rien ne signalerait.
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::resource::Resource, bevy_reflect::Reflect),
    reflect(Component)
)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConsumableInventory {
    pub slots: Vec<Option<ConsumableId>>,
}

impl ConsumableInventory {
    /// Inventaire de `capacity` slots vides. **Seule fabrique.**
    #[must_use]
    pub fn new(capacity: u8) -> Self {
        Self {
            slots: vec![None; usize::from(capacity)],
        }
    }

    /// Place `id` dans le premier slot libre. Rend son rang, `None` si plein.
    pub fn add(&mut self, id: ConsumableId) -> Option<usize> {
        let rang = self.slots.iter().position(Option::is_none)?;
        self.slots[rang] = Some(id);
        Some(rang)
    }

    /// Le nombre de slots **occupés**, non la taille du tableau.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    /// Vrai quand aucun consommable n'est détenu, même si des slots existent.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
