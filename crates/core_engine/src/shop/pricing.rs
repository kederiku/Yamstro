//! Prix, revente et coût de relance. **Arithmétique entière, et rien d'autre.**
//!
//! Ces trois fonctions sont **pures et totales** : elles ne connaissent que
//! leurs arguments, ni la session, ni l'or, ni l'inventaire du joueur. Le
//! débit, le refus d'achat et les capacités sont l'affaire de la crate d'états.
//!
//! # Les valeurs sont provisoires, et c'est écrit
//!
//! Le corpus ne fixe qu'un prix, celui de la *Rune du Destin*. La table en est
//! l'extension minimale : comme la cible de base et la croissance par ante, ce
//! sont des **points de départ à calibrer** par le harnais de l'Étape 6 bis,
//! jamais un équilibrage acquis.

use crate::consumables;
use crate::relics::{RelicRarity, rarity_of};
use crate::shop::ShopItem;

/// Ce que chaque relance supplémentaire d'une même visite ajoute au coût.
pub const REROLL_COST_STEP: u32 = 1;

/// Le prix d'un palier de rareté.
///
/// **Un `match` exhaustif, pas un tableau indexé.** Une cinquième rareté doit
/// être une erreur de compilation ici comme dans `rarity_of`, et un tableau
/// aligné sur un ordre déclaré ailleurs rendrait une valeur pour elle sans rien
/// dire. La table de tirage, elle, vit dans le générateur et nulle part
/// ailleurs : ce fichier lit des raretés, jamais des poids.
fn price_of_rarity(rarity: RelicRarity) -> u32 {
    match rarity {
        RelicRarity::Common => 4,
        RelicRarity::Uncommon => 6,
        RelicRarity::Rare => 8,
        RelicRarity::Legendary => 10,
    }
}

/// Le prix d'un article de l'étalage.
///
/// **Fonction totale**, sans bras attrape-tout : une variante ajoutée à
/// `ShopItem` doit produire un `E0004`, jamais un prix nul silencieux.
///
/// Le prix de la *Rune du Destin* **tombe du palier Rare**. Le coder en dur
/// créerait un second point de vérité, qui survivrait à un recalibrage de la
/// table et donnerait deux prix pour une même rareté.
///
/// Le parchemin de grille et le dé modifié n'ont pas de rareté propre : ils
/// **empruntent** un palier, ils n'introduisent pas un second barème. Les trois
/// magnitudes de dé offertes coûtent donc le même prix, écart assumé et porté
/// au registre de l'Étape 6 bis avec les magnitudes elles-mêmes.
///
/// Elle reste une fonction de l'**article**, jamais une valeur figée au moment
/// de la génération : c'est ce qui permettra à l'Étape 9 de brancher ses écarts
/// par-dessus, l'écart de prix d'un gobelet et le parchemin gratuit du sceau
/// bleu, sans rouvrir ce fichier.
///
/// # Revendre un objet déjà possédé
///
/// La revente prend un **prix**, pas un article : ce que la crate d'états
/// revendra est une relique de l'inventaire, qui porte un identifiant et non un
/// article d'étalage. Elle reconstruira donc l'article — `ShopItem::RelicCard`
/// de l'identifiant possédé — puis appellera cette fonction. **C'est le chemin
/// prévu** ; établir un barème par identifiant serait le second point de vérité
/// que tout ce fichier évite.
pub fn price_of(item: &ShopItem) -> u32 {
    match item {
        ShopItem::RelicCard(def) => price_of_rarity(rarity_of(*def)),
        ShopItem::Consumable(id) => price_of_rarity(consumables::rarity_of(*id)),
        ShopItem::GridUpgrade(_) => price_of_rarity(RelicRarity::Common),
        ShopItem::DieMod(_) => price_of_rarity(RelicRarity::Uncommon),
    }
}

/// La reprise d'un article payé `price`.
///
/// **La moitié, tronquée vers zéro, plancher à un.** On tronque, on n'arrondit
/// pas au plus proche : un article à sept se revend trois. Sans le plancher,
/// `1 / 2` rendrait zéro et vendre coûterait de l'or au joueur.
///
/// **Un article gratuit se revend zéro**, et non un. Le plancher protège une
/// revente réelle contre le zéro ; appliqué à un article gratuit, il
/// fabriquerait de l'or à partir de rien dès que le sceau bleu de l'Étape 9
/// offrira un parchemin à zéro. La clause tient en une ligne, et elle ne se
/// rattrape pas plus tard.
///
/// La revente n'est jamais une plus-value : le résultat est toujours inférieur
/// ou égal au prix.
pub fn sell_value(price: u32) -> u32 {
    if price == 0 {
        return 0;
    }
    (price / 2).max(1)
}

/// Le coût de la relance suivante, dans la **même visite**.
///
/// La remise à zéro est **par visite**, et elle est obtenue par construction :
/// le générateur pose le coût initial à chaque appel, et il n'est appelé qu'à
/// l'entrée en boutique. Aucun code ne remet quoi que ce soit à zéro.
///
/// # Contrat pour le chemin de relance
///
/// Une relance **remplace les articles** et applique cette fonction. Elle ne
/// rappelle **pas** le générateur sur l'inventaire existant : cela replacerait
/// le coût initial et rendrait les relances gratuites à perpétuité. Le piège
/// n'est atteignable que depuis ce chemin, donc depuis la crate d'états ; il ne
/// peut pas être gardé ici.
///
/// Saturation plutôt que débordement : cinquante relances donnent cinquante
/// cinq, et le plafond reste le plafond.
pub fn bump_reroll_cost(cost: u32) -> u32 {
    cost.saturating_add(REROLL_COST_STEP)
}
