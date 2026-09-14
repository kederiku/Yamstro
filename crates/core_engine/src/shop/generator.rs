//! Génération de l'étalage : pondération, repli, et rien d'autre.
//!
//! # Deux distributions, à ne pas confondre
//!
//! Le catalogue contient quatre Communes, quatre Peu communes et quatre Rares :
//! c'est un quota d'**archétypes couverts**. Le **taux de tirage** est
//! 65/25/9/1. Aucune des deux ne se déduit de l'autre, et aucune entrée n'est
//! dupliquée « pour approcher soixante-cinq pour cent ».
//!
//! # Arithmétique entière, et flux unique
//!
//! Cumul sur `0..1_000`, aucun flottant, aucune bibliothèque de distribution
//! pondérée. Le tirage porte sur le flux de la boutique **et lui seul** : un
//! seul appel sur celui des dés décalerait la séquence des lancers et casserait
//! la reproductibilité d'une graine partagée.
//!
//! Aucune table associative non plus : son ordre d'itération n'est pas garanti,
//! et deux runs de même graine rendraient deux étalages.

use rand::{Rng, RngExt};

use crate::consumables::ConsumableId;
use crate::dice::DieModifier;
use crate::hands::YahtzeeHand;
use crate::relics::{CATALOG, RelicId, RelicRarity, rarity_of};
use crate::shop::{INITIAL_REROLL_COST, ShopInventory, ShopItem};

/// Les raretés, dans l'ordre de déclaration de leurs variantes.
pub const RARITY_ORDER: [RelicRarity; 4] = [
    RelicRarity::Common,
    RelicRarity::Uncommon,
    RelicRarity::Rare,
    RelicRarity::Legendary,
];

/// Les poids de tirage, en pour-mille, dans l'ordre de `RARITY_ORDER`.
///
/// **Table unique du projet.** Elle n'est déclarée nulle part ailleurs : ni
/// dans la tarification, ni dans la crate de boutique, ni recopiée dans un test
/// « pour vérifier ». Une table à soixante-dix pour cent de Communes a circulé
/// dans un document plus ancien ; elle est fausse, et n'a pas été conciliée.
pub const RARITY_WEIGHTS_PERMILLE: [u32; 4] = [650, 250, 90, 10];

/// Le total des poids. Le tirage porte sur `0..TOTAL_PERMILLE`.
const TOTAL_PERMILLE: u32 = 1_000;

/// Les modificateurs de dé que la boutique propose.
///
/// **Le corpus ne les chiffre pas.** Il nomme la variante `DieMod` et s'arrête
/// là, alors que `DieModifier` porte une grandeur. Ces valeurs sont donc
/// **provisoires**, posées ici en un seul endroit pour que le harnais de
/// l'Étape 6 bis les calibre sans les chercher. Le mult est en centièmes.
const DIE_MODS_OFFERTS: [DieModifier; 3] = [
    DieModifier::BonusChips(30),
    DieModifier::BonusChips(50),
    DieModifier::BonusMult(100),
];

/// Le vivier d'une rareté, replié vers le palier inférieur s'il est vide.
///
/// **Le repli ne consomme aucun tirage**, et c'est tout son intérêt : une
/// boucle de retirage, même bornée, décalerait le flux dès qu'une Légendaire
/// sort, et deux graines identiques rendraient deux étalages.
///
/// Aucune Légendaire n'existe à cette étape, donc `Legendary` se replie sur
/// `Rare`. Le jour où l'Étape 9 en ajoute une, ce repli cesse de s'appliquer
/// sans qu'une ligne change ici.
///
/// Le vivier se construit en filtrant le catalogue, une tranche **ordonnée** :
/// son ordre est donc stable d'une exécution à l'autre. Le catalogue n'est ni
/// trié, ni repondéré ; son ordre sert de référence aux tickets aval.
///
/// Rend une tranche **vide** si aucun palier n'est peuplé, ce qui ne peut pas
/// arriver avec les douze reliques mais s'écrit sans panique.
pub fn relic_pool(rarity: RelicRarity) -> Vec<RelicId> {
    let depart = RARITY_ORDER.iter().position(|r| *r == rarity).unwrap_or(0);

    for palier in (0..=depart).rev() {
        let vivier: Vec<RelicId> = CATALOG
            .iter()
            .copied()
            .filter(|def| rarity_of(*def) == RARITY_ORDER[palier])
            .collect();
        if !vivier.is_empty() {
            return vivier;
        }
    }

    Vec::new()
}

/// La rareté d'un seuil, par cumul entier sur `0..TOTAL_PERMILLE`.
///
/// **Séparée du tirage, et c'est ce qui la rend éprouvable.** Les frontières
/// exactes — 649 est Commune, 650 est Peu commune — se vérifient valeur par
/// valeur ; mêlée au tirage, la même règle ne se mesurerait que par des
/// fréquences, et un décalage d'un pour-mille passerait inaperçu.
pub fn rarity_for_permille(seuil: u32) -> RelicRarity {
    let mut cumul = 0;

    for (index, poids) in RARITY_WEIGHTS_PERMILLE.iter().enumerate() {
        cumul += poids;
        if seuil < cumul {
            return RARITY_ORDER[index];
        }
    }

    // Inatteignable, les poids sommant au total du tirage ; la dernière rareté
    // plutôt qu'une panique.
    RARITY_ORDER[RARITY_ORDER.len() - 1]
}

/// La rareté tirée : **un** seuil, et la correspondance ci-dessus.
// La borne `Rng + RngExt` est redondante, `RngExt` ayant `Rng` pour supertrait,
// mais elle est imposée verbatim par le corpus. L'attribut lève le refus de
// clippy sans modifier la signature, comme pour `Die::roll` depuis TASK-02.
#[allow(clippy::implied_bounds_in_impls)]
fn tirer_rarete(rng: &mut (impl Rng + RngExt)) -> RelicRarity {
    rarity_for_permille(rng.random_range(0..TOTAL_PERMILLE))
}

/// Une entrée d'une tranche, tirée uniformément. `None` sur tranche vide :
/// `random_range` panique sur un intervalle vide.
///
/// La borne est `Clone` et non `Copy` : `DieModifier` ne l'est pas, et l'y
/// ajouter pour la commodité d'un générateur toucherait un type de l'Étape 1.
// La borne `Rng + RngExt` est redondante, `RngExt` ayant `Rng` pour supertrait,
// mais elle est imposée verbatim par le corpus. L'attribut lève le refus de
// clippy sans modifier la signature, comme pour `Die::roll` depuis TASK-02.
#[allow(clippy::implied_bounds_in_impls)]
fn tirer_dans<T: Clone>(tranche: &[T], rng: &mut (impl Rng + RngExt)) -> Option<T> {
    if tranche.is_empty() {
        return None;
    }
    tranche.get(rng.random_range(0..tranche.len())).cloned()
}

/// L'étalage d'une visite : deux reliques, un parchemin de grille, puis un dé
/// modifié ou un consommable.
///
/// **Nombre de tirages constant par article** : une rareté puis un index pour
/// une relique, un index pour le parchemin, une branche puis un index pour le
/// quatrième. Le repli n'en ajoute aucun.
///
/// Les deux reliques sont tirées **indépendamment** : un doublon dans le même
/// étalage est légal à cette étape. L'exclure demanderait soit un retirage, soit
/// un vivier amputé qui fausserait les poids. À arbitrer par l'Étape 6 bis.
///
/// Le parchemin se tire dans les treize figures, sans regarder les niveaux
/// courants : le générateur reste **pur et sans session**, ce qui est ce qui le
/// rend reproductible. Il peut donc proposer une figure déjà au plafond, ce qui
/// demande neuf parchemins sur la même case et ne se produit pas en huit antes.
#[allow(clippy::implied_bounds_in_impls)]
pub fn generate_shop(rng: &mut (impl Rng + RngExt)) -> ShopInventory {
    let mut items = Vec::with_capacity(4);

    for _ in 0..2 {
        let vivier = relic_pool(tirer_rarete(rng));
        if let Some(def) = tirer_dans(&vivier, rng) {
            items.push(ShopItem::RelicCard(def));
        }
    }

    if let Some(figure) = tirer_dans(&YahtzeeHand::ALL, rng) {
        items.push(ShopItem::GridUpgrade(figure));
    }

    // La branche d'abord, l'article ensuite : deux tirages, quel que soit le
    // côté choisi.
    let consommable = rng.random_range(0..2) == 0;
    if consommable {
        if let Some(id) = tirer_dans(&ConsumableId::ALL, rng) {
            items.push(ShopItem::Consumable(id));
        }
    } else if let Some(modificateur) = tirer_dans(&DIE_MODS_OFFERTS, rng) {
        items.push(ShopItem::DieMod(modificateur));
    }

    ShopInventory {
        items,
        reroll_cost: INITIAL_REROLL_COST,
    }
}
