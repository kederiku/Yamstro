//! Gobelets : CupDeck, CupId.

pub mod definitions;

/// Identité d'un gobelet. Le catalogue complet est arrêté à l'Étape 9 ; ces
/// cinq variantes sont celles dont l'Étape 1 a besoin.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CupId {
    Standard,
    Abandoned,
    Polyhedron,
    Cheater,
    Fortune,
}

/// Configuration de départ d'une run. C'est la seule source des valeurs de
/// gameplay : rien dans le moteur ne les présuppose (ADR-007).
///
/// `starting_gold` et `sides` ne remontent pas dans `RunConfig` : le premier
/// est écrit une fois dans l'or de la session, le second dimensionne les faces
/// de la main active.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CupDeck {
    pub id: CupId,
    pub starting_gold: u32,
    /// Nombre de faces de chaque dé, dans l'ordre. Sa longueur vaut
    /// `dice_count`. L'Étape 9 y emploiera un `SmallVec` ; ici c'est un `Vec`,
    /// le document source interdisant `SmallVec` dans un type réfléchi.
    pub sides: Vec<u8>,
    pub dice_count: u8,
    pub base_rerolls: u8,
    pub relic_capacity: u8,
    pub consumable_capacity: u8,
    pub max_interest: u32,
    pub hands_per_blind: u8,
}
