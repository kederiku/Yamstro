//! Définitions des reliques : rareté, comportements et grille de valeurs.
//!
//! Symétrique de `cups/definitions`, qui porte le catalogue des gobelets : ce
//! module est le seul endroit du moteur où les paramètres d'une relique sont
//! écrits en dur (ADR-007).
//!
//! # Budget de puissance
//!
//! Unité de compte : **1 Mult ≡ 10 Chips**. Un `AddChips(c)` vaut `c` unités ;
//! un `AddMult(m)`, `m` étant en centièmes, vaut `m / 10` unités. Main de
//! référence : cinq dés comptabilisés, faces uniformes sur 1–6, figure de
//! niveau 1 — d'où deux dés et demi impairs, autant de pairs, et un six sur
//! six.
//!
//! | Rareté | Budget | Formes autorisées | Part du pool |
//! | :-- | --: | :-- | --: |
//! | Commune | 40 unités | `AddChips`, `AddMult`, or | 65 % |
//! | Peu commune | 80 unités | `AddChips`, `AddMult`, or, manipulation de dés | 25 % |
//! | Rare | ×1,5 Mult | multiplication, effets structurels | 9 % |
//! | Légendaire | ×2,0 Mult | multiplication, règles inédites | 1 % |
//!
//! **La multiplication du Mult est réservée aux raretés `Rare` et
//! `Legendary`.** Une relique conditionnelle, de fréquence `f`, peut dépasser
//! son budget d'un facteur `1/f`, **plafonné à trois**. Ces deux règles ne sont
//! pas que des phrases : `test_rarity_budget_invariant` les éprouve sur un
//! produit cartésien de contextes, et il devra passer tel quel sur les soixante
//! reliques de l'Étape 9.
//!
//! ## Budget consommé, relique par relique
//!
//! | # | Relique | Rareté | Action émise | Budget consommé |
//! | :-- | :-- | :-- | :-- | :-- |
//! | 1 | Le Dé Fêlé | Commune | `AddMult(100)` | 2,5 × 1 Mult ≡ 25 u / 40 → **63 %** |
//! | 2 | La Pierre Polie | Commune | `AddChips(10)` | 2,5 × 10 Chips ≡ 25 u / 40 → **63 %** |
//! | 3 | Maître du Brelan | Commune | `AddMult(600)` | brut 60 u ; `f ≈ 1/3` ⇒ plafond 40 × 3 = 120 u → **50 %** |
//! | 4 | Architecte du Full | Peu commune | `AddChips(40)`, `AddMult(500)` | brut 40 + 50 = 90 u ; `f ≈ 1/6` ⇒ plafond 80 × 3 = 240 u → **38 %** |
//! | 5 | L'Alignement Stellaire | Peu commune | `AddMult(800)` | brut 80 u ; `f ≈ 1/6` ⇒ plafond 240 u → **33 %** |
//! | 6 | La Pyramide de Six | Peu commune | `AddChips(15)` | 0,83 × 15 ≡ 12,5 u / 80 → **16 %** ; 45 u (**56 %**) dès trois 6 comptabilisés, cas d'un build `Sixes` |
//! | 7 | Le Balancier | Rare | multiplication par 1,5 | `f = 1/2` ⇒ plafond ×1,5 × 2 = ×3,0 → **50 %** |
//! | 8 | Obsidienne Instable | Rare | multiplication par 2, `reroll_delta: -1` | ×2,0 / ×1,5 → **133 %**, ramené sous 100 % par le malus de relance |
//! | 9 | Yams Divin | Rare | multiplication par 3 | `f ≈ 1/20` ⇒ plafond ×1,5 × 3 = ×4,5 → **67 %** |
//! | 10 | Tirelire en Terre | Commune | or de fin de blind | ≈ +$2,5 par blind, borné à +$5 — hors échelle Chips/Mult |
//! | 11 | Dé Fantôme | Peu commune | `force_values` | ≈ 1,5 u en Chips faciaux (**2 %**) ; valeur réelle **à mesurer au harnais de l'Étape 6 bis** |
//! | 12 | Miroir Double | Rare | ré-émission de `ctx.left_effects` | gabarit = celui de sa voisine de gauche ; pire cas assumé : *Obsidienne* à gauche |
//!
//! ## Les deux lignes qui sortent du calcul simple
//!
//! **Obsidienne Instable est à 133 % du budget Rare, et c'est un arbitrage
//! assumé.** Deux rapporté à un et demi donne 133 % ; le malus `reroll_delta:
//! -1` retire une relance par manche, soit environ un quart de probabilité
//! d'atteindre une figure en moins, ce qui ramène la valeur nette sous 100 %.
//! Les deux moitiés forment un tout : ne ramène pas la multiplication à 1,5 et
//! ne retire pas le malus pour justifier le 2,0.
//!
//! **Dé Fantôme affiche 2 %, et ce chiffre est trompeur.** La probabilité
//! qu'aucun dé n'affiche 1 vaut environ 40 %, et relever le plus faible à six
//! ajoute une unité et demie en Chips faciaux. Mais la valeur réelle de cette
//! relique est **la probabilité de figure déplacée** — une Suite manquée qui se
//! complète, un Carré qui devient Yams —, et cela **se mesure, cela ne se
//! calcule pas**. N'invente aucun coefficient de conversion pour remonter les
//! 2 % : le tableau porte la valeur nominale et la mention « à mesurer », et
//! c'est la forme correcte.

pub mod clay_piggy_bank;
pub mod cracked_die;
pub mod divine_yahtzee;
pub mod double_mirror;
pub mod full_house_architect;
pub mod ghost_die;
pub mod pendulum;
pub mod polished_stone;
pub mod pyramid_of_sixes;
pub mod stellar_alignment;
pub mod triplet_master;
pub mod unstable_obsidian;

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
