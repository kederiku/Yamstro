//! L'inventaire de reliques : sa forme, sa fabrique et ses trois mutateurs.
//!
//! Séparé de `relics/mod.rs` à TASK-54, l'arborescence du corpus lui donnant son
//! propre fichier. `RelicInventory` reste ré-exporté par `relics`, de sorte
//! qu'aucun chemin d'import ne bouge.

#[cfg(feature = "bevy")]
use bevy_ecs::reflect::ReflectComponent;

use super::{RelicId, RelicInstance, RelicState};

/// Unique source de vérité de l'inventaire. L'ordre des slots **est** l'ordre
/// d'application des effets (ADR-005) : il n'est jamais retrié, par quoi que ce
/// soit. `slots` est dimensionné par l'appelant depuis
/// `RunConfig.relic_capacity` (ADR-007), jamais depuis un littéral.
#[cfg_attr(
    feature = "bevy",
    derive(bevy_ecs::resource::Resource, bevy_reflect::Reflect),
    reflect(Component)
)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RelicInventory {
    pub slots: Vec<Option<RelicInstance>>,
    /// Compteur **monotone** d'identifiants. Jamais décrémenté, jamais remis à
    /// zéro, et un identifiant libéré n'est jamais réattribué.
    pub next_uid: u32,
}

impl RelicInventory {
    /// Inventaire de `capacity` slots vides. **Seule fabrique.**
    ///
    /// Le point d'appel s'écrit `RelicInventory::new(config.relic_capacity)` et
    /// rien d'autre : la capacité vient de la configuration du gobelet
    /// (ADR-007), jamais d'un littéral. **Aucun `Default` n'est dérivé** — il
    /// donnerait un inventaire à zéro slot, ce qui n'est jamais la capacité
    /// voulue, et rien ne le signalerait.
    #[must_use]
    pub fn new(capacity: u8) -> Self {
        Self {
            slots: vec![None; usize::from(capacity)],
            next_uid: 0,
        }
    }

    /// Place `def` dans le premier slot libre. Rend le `uid` attribué, `None`
    /// si l'inventaire est plein.
    ///
    /// **Un refus ne consomme aucun identifiant** : `next_uid` reste inchangé
    /// quand il n'y a pas de place. C'est la seule écriture de ce compteur dans
    /// tout le projet.
    pub fn add_relic(&mut self, def: RelicId) -> Option<u32> {
        let libre = self.slots.iter_mut().find(|slot| slot.is_none())?;
        let uid = self.next_uid;
        *libre = Some(RelicInstance {
            uid,
            def,
            state: RelicState::None,
        });
        self.next_uid = self.next_uid.saturating_add(1);
        Some(uid)
    }

    /// Retire et rend la relique du slot `slot`. `None` si le slot est vide ou
    /// hors bornes.
    ///
    /// **Le slot est vidé, pas supprimé** : `Vec::remove` raccourcirait
    /// l'inventaire et changerait sa capacité, alors que la capacité vient du
    /// gobelet. L'identifiant libéré n'est **jamais** réattribué, `next_uid`
    /// étant monotone : c'est ce qui garde deux copies d'une même définition
    /// distinguables dans le journal de score et à l'écran.
    pub fn remove_relic(&mut self, slot: u8) -> Option<RelicInstance> {
        self.slots.get_mut(usize::from(slot))?.take()
    }

    /// Déplace la relique de `from` vers `to`, les slots intermédiaires
    /// glissant d'un cran.
    ///
    /// **Décalage, jamais échange.** `[A,B,C,D,E]` et `reorder(0, 2)` donnent
    /// `[B,C,A,D,E]`, pas `[C,B,A,D,E]` : c'est la sémantique du
    /// glisser-déposer, et c'est aussi un autre score, l'ordre des slots étant
    /// l'ordre d'application (ADR-005).
    ///
    /// L'opération porte sur `slots` **entier, slots vides compris**, de sorte
    /// que la longueur reste invariante. Hors bornes, elle ne fait rien et ne
    /// panique pas : `Vec::remove` et `Vec::insert` paniqueraient, et le
    /// glisser-déposer lâche des index arbitraires.
    pub fn reorder(&mut self, from: u8, to: u8) {
        let (from, to) = (usize::from(from), usize::from(to));
        if from >= self.slots.len() || to >= self.slots.len() || from == to {
            return;
        }
        let deplacee = self.slots.remove(from);
        self.slots.insert(to, deplacee);
    }

    /// Slots occupés, de gauche à droite, numéro de slot compris. Les `None`
    /// sont sautés ; rien d'autre ne l'est, et rien n'est trié.
    ///
    /// **Invariant :** `slots.len() <= 256`, faute de quoi le numéro de slot ne
    /// tient pas dans un `u8` et deux slots distincts se présenteraient sous le
    /// même numéro. `RunConfig.relic_capacity` étant un `u8`, la configuration
    /// ne peut pas l'enfreindre ; un `Vec` construit à la main ou relu d'une
    /// sauvegarde le peut, d'où l'assertion.
    pub fn iter_slots(&self) -> impl Iterator<Item = (u8, &RelicInstance)> + '_ {
        debug_assert!(
            self.slots.len() <= usize::from(u8::MAX) + 1,
            "inventaire de {} slots : le numéro de slot déborde le u8",
            self.slots.len()
        );
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|relic| (index as u8, relic)))
    }

    /// Nombre de reliques possédées, c'est-à-dire de slots occupés. C'est la
    /// forme qu'attend la boutique : `relics.len() < config.relic_capacity`.
    pub fn len(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    /// Vrai quand aucune relique n'est possédée, même si des slots existent.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Nombre de slots, occupés ou non. À ne pas confondre avec `len()` :
    /// les intervertir inverse la condition d'achat.
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }
}
