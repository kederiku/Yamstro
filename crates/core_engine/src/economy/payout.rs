//! Le gain de fin de manche, **décomposé**.
//!
//! # Pourquoi quatre termes et non un forfait
//!
//! Le bonus de mains non consommées n'est pas une décoration. Avec quatre mains
//! par blind pour **treize cases consommables**, battre une manche en deux
//! mains rapporte deux dollars de plus qu'en quatre — mais chaque main dépensée
//! est aussi **une case brûlée** et un score engrangé. C'est la moitié de la
//! tension économique du jeu, et elle n'existe que parce que la grille est
//! consommable (ADR-001). Le réduire à une récompense forfaitaire supprimerait
//! l'arbitrage sans rien casser de visible.
//!
//! # Deux cinq de nature opposée
//!
//! La tranche est une **règle du jeu** : un dollar d'intérêt par tranche de
//! cinq en banque. Le plafond est un **paramètre de run**, lu dans la
//! configuration du gobelet, qu'une relique de l'Étape 9 portera à dix.
//!
//! **Les confondre ne se voit pas.** Les cinq gobelets valent tous un plafond
//! de cinq : dans une partie réelle aujourd'hui, plafonner par la constante ou
//! par la configuration donne exactement la même chose. Seul un test sur une
//! configuration construite à la main les sépare, et sans lui la relique de
//! l'Étape 9 naîtrait silencieusement inerte.
//!
//! # Ce que cette fonction ne fait pas
//!
//! Elle ne crédite rien. L'or a une seule source de vérité, portée par la
//! session de run, et c'est l'appelant qui l'y ajoute en saturation. Elle
//! n'agrège pas non plus les déclencheurs de fin de blind : l'or des reliques
//! lui arrive tout calculé, faute de quoi elle perdrait sa pureté et
//! deviendrait intestable sans inventaire.

use crate::blinds::BlindDefinition;
use crate::config::RunConfig;

/// Tranche d'or qui rapporte un dollar d'intérêt.
///
/// **Règle du jeu, pas paramètre de run.** Le plafond, lui, vient de la
/// configuration : voir l'en-tête du module.
pub const INTEREST_TRANCHE: u32 = 5;

/// Le gain d'une fin de manche, terme par terme.
///
/// **Aucun champ de plus.** Ni identifiant de blind, ni ante, ni drapeau
/// d'application : un tel drapeau inviterait au double comptage que l'ADR-010
/// supprime en ne laissant qu'un seul écrivain.
///
/// Pas de serde non plus : c'est une valeur calculée et consommée dans la
/// frame, jamais persistée — même arbitrage que pour le modificateur de lancer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Payout {
    pub blind_reward: u32,
    pub unused_hands: u32,
    pub interest: u32,
    pub relic_gold: u32,
    pub total: u32,
}

/// Le gain d'une manche battue.
///
/// **Fonction libre, et pure.** Elle ne lit aucune ressource, n'écrit nulle
/// part, ne tire aucun aléa, et ne connaît que ses cinq paramètres.
///
/// La récompense se **lit** dans la définition, elle ne se recalcule pas depuis
/// le rang : le Stake 2 de l'Étape 10 forcera cette valeur à zéro sur les
/// Petites Mises, et un calcul local le contredirait sans erreur de
/// compilation.
#[must_use]
pub fn calculate_payout(
    blind: &BlindDefinition,
    hands_remaining: u8,
    gold: u32,
    config: &RunConfig,
    relic_gold: u32,
) -> Payout {
    let interest = (gold / INTEREST_TRANCHE).min(config.max_interest);
    let unused_hands = u32::from(hands_remaining);

    Payout {
        blind_reward: blind.reward,
        unused_hands,
        interest,
        relic_gold,
        total: blind
            .reward
            .saturating_add(unused_hands)
            .saturating_add(interest)
            .saturating_add(relic_gold),
    }
}
