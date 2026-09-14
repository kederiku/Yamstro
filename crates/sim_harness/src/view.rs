//! Vues en lecture seule des politiques, et la décision qu'elles rendent.
//!
//! **Une vue ne mute rien.** Les deux structures n'exposent que des emprunts
//! partagés et aucune méthode qui écrive : une politique reçoit une vue et rend
//! une décision que le harnais applique. C'est le type, et non une convention
//! de relecture, qui garantit qu'une politique ne consomme jamais un flux du
//! générateur de run — aucune vue n'en porte, ni ne porte de `&mut` quoi que ce
//! soit.
//!
//! Les deux doc-tests ci-dessous le montrent, et le positif est là pour la même
//! raison que partout ailleurs dans ce dépôt : sans son jumeau, un
//! `compile_fail` passe aussi bien sur une faute de frappe.
//!
//! ```
//! # use core_engine::config::RunConfig;
//! # use core_engine::cups::CupId;
//! # use core_engine::cups::definitions::cup;
//! # use core_engine::relics::RelicInventory;
//! # use sim_harness::view::ShopView;
//! # use core_engine::shop::ShopInventory;
//! let config = RunConfig::from_cup(&cup(CupId::Standard));
//! let mut stock = RelicInventory::new(config.relic_capacity);
//! let etalage = ShopInventory::default();
//! let vue = ShopView { inventory: &etalage, gold: 10, relics: &stock, config: &config };
//! let _lecture = vue.relics.len();
//! # let _ = &mut stock;
//! ```
//!
//! ```compile_fail
//! # use core_engine::config::RunConfig;
//! # use core_engine::cups::CupId;
//! # use core_engine::cups::definitions::cup;
//! # use core_engine::relics::RelicInventory;
//! # use core_engine::relics::RelicId;
//! # use sim_harness::view::ShopView;
//! # use core_engine::shop::ShopInventory;
//! let config = RunConfig::from_cup(&cup(CupId::Standard));
//! let stock = RelicInventory::new(config.relic_capacity);
//! let etalage = ShopInventory::default();
//! let vue = ShopView { inventory: &etalage, gold: 10, relics: &stock, config: &config };
//! vue.relics.add_relic(RelicId::PolishedStone);
//! ```

use core_engine::blinds::BlindContext;
use core_engine::config::RunConfig;
use core_engine::dice::{Die, DieId};
use core_engine::evaluator::HandMatch;
use core_engine::hands::{HandLevels, YahtzeeHand};
use core_engine::relics::RelicInventory;
use core_engine::shop::ShopInventory;
use smallvec::SmallVec;

/// Ce qu'une politique de main voit.
///
/// **Elle porte la manche et ne porte pas la grille des figures consommées.**
/// Le document source lui donne les deux — or la manche la porte déjà. Deux
/// chemins vers la même donnée, dans une vue en lecture seule, sont une
/// invitation à ce qu'ils divergent le jour où l'un sera reconstruit à partir
/// de l'autre ; et le jour où ils divergent, la grille ment sans qu'aucun test
/// ne le voie. Une politique écrit `view.blind.used_hands.contains(hand)`.
///
/// La même référence donne la cible, le score courant et les mains restantes,
/// ce qui est la seconde raison de ne pas dupliquer la grille.
pub struct HandView<'a> {
    pub dice: &'a [Die],
    pub matches: &'a [HandMatch],
    pub blind: &'a BlindContext,
    pub rerolls_left: u8,
    pub hand_levels: &'a HandLevels,
    pub relics: &'a RelicInventory,
}

/// Ce qu'une politique d'achat voit.
///
/// L'étalage se lit dans l'inventaire de boutique, un point c'est tout : la vue
/// ne recopie ni prix, ni rareté, ni description, qui se lisent par les
/// fonctions du moteur. La configuration porte la capacité de reliques et le
/// plafond d'intérêts, que la politique lit là et jamais en littéral.
pub struct ShopView<'a> {
    pub inventory: &'a ShopInventory,
    pub gold: u32,
    pub relics: &'a RelicInventory,
    pub config: &'a RunConfig,
}

/// La liste des dés **à conserver**, adressés par identifiant.
///
/// **Le nom vient du document source et ment : ce n'est pas un masque de
/// bits.** Écris-le ici, sinon le prochain lecteur « corrigera » la structure
/// vers ce que le nom annonce. Un entier masqué serait parfaitement légal tant
/// que la main tient en huit dés — et il casserait à l'Étape 9 sur *La Meule*,
/// qui retire un dé **en cours de manche** : les positions se décalent, les
/// identifiants non. Le harnais verrouillerait les mauvais dés, sans erreur ni
/// panique. La même machinerie sert *Le Pacte de Sang* et la *Rune
/// d'Immolation*.
///
/// **Le champ tuple est privé, donc l'API ci-dessous est obligatoire, pas
/// décorative** : la politique gloutonne construit le masque depuis les dés
/// marquants d'une figure, et la boucle le lit pour décider quoi relancer. Sans
/// constructeur ni accesseur, ni l'une ni l'autre ne le peut, et ce qui suivrait
/// est prévisible — une seconde déclaration du type ailleurs, qui divergerait au
/// premier changement de forme. C'est l'API qui s'ouvre, pas la représentation.
///
/// La capacité inline est un dimensionnement, **pas un plafond** : le type
/// accepte davantage en débordant sur le tas, et rien ici ne suppose six dés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockMask(SmallVec<[DieId; 6]>);

impl LockMask {
    /// Accepte n'importe quel itérable, ce qui laisse une politique écrire
    /// `LockMask::new(candidat.scoring_dice.iter().copied())` sans allocation
    /// intermédiaire.
    #[must_use]
    pub fn new(ids: impl IntoIterator<Item = DieId>) -> Self {
        Self(ids.into_iter().collect())
    }

    #[must_use]
    pub fn contains(&self, id: DieId) -> bool {
        self.0.contains(&id)
    }

    /// Rend les identifiants **dans l'ordre d'insertion**, et cet ordre est
    /// stable : le test de déterminisme de la politique gloutonne compare des
    /// masques entiers, ordre compris.
    pub fn iter(&self) -> impl Iterator<Item = DieId> + '_ {
        self.0.iter().copied()
    }

    /// La relance totale — aucun dé conservé —, lue sans passer par un compte.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Ce qu'une politique de main rend.
///
/// **Elle est déclarée là où le masque l'est**, faute de quoi les deux
/// évoluent séparément : le jour où la forme du masque change — et elle
/// changera à l'Étape 9 — le type qui le porte serait dans un autre fichier,
/// sous la garde d'un autre ticket. Les séparer revient à couper une paire.
///
/// Le module des politiques la ré-exporte ; ce ticket déclare le type et rien
/// d'autre : le trait qui la rend et la boucle qui l'applique sont deux tickets
/// plus loin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandDecision {
    Reroll(LockMask),
    Submit(YahtzeeHand),
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::blinds::{BlindDefinition, BlindType};
    use core_engine::config::RunConfig;
    use core_engine::cups::CupId;
    use core_engine::cups::definitions::cup;
    use core_engine::hands::HandGrid;
    use core_engine::pool::DicePool;

    fn manche(used: HandGrid) -> BlindContext {
        BlindContext {
            blind: BlindDefinition {
                kind: BlindType::Boss,
                target_score: 600,
                reward: 5,
                modifier: None,
            },
            target_score: 600,
            current_score: 120,
            hands_remaining: 3,
            used_hands: used,
        }
    }

    #[test]
    fn test_blind_context_is_built_by_literal() {
        let contexte = manche(HandGrid::default());

        assert_eq!(contexte.blind.kind, BlindType::Boss);
        assert_eq!(contexte.target_score, 600);
        assert_eq!(contexte.current_score, 120);
        assert_eq!(contexte.hands_remaining, 3);
        assert!(contexte.used_hands.is_empty());
    }

    #[test]
    fn test_hand_view_reads_used_hands_through_blind() {
        let mut grille = HandGrid::default();
        grille.mark(YahtzeeHand::FullHouse);
        let contexte = manche(grille);

        let config = RunConfig::from_cup(&cup(CupId::Standard));
        let stock = RelicInventory::new(config.relic_capacity);
        let niveaux = HandLevels::default();
        let vue = HandView {
            dice: &[],
            matches: &[],
            blind: &contexte,
            rerolls_left: 2,
            hand_levels: &niveaux,
            relics: &stock,
        };

        // **Un seul chemin vers la grille**, et il traverse la manche.
        assert!(vue.blind.used_hands.contains(YahtzeeHand::FullHouse));
        assert!(!vue.blind.used_hands.contains(YahtzeeHand::Yahtzee));
        // Et la même référence donne les trois autres champs dont la politique
        // de l'Étape 6 bis a besoin.
        assert_eq!(vue.blind.target_score, 600);
        assert_eq!(vue.blind.current_score, 120);
        assert_eq!(vue.blind.hands_remaining, 3);
    }

    #[test]
    fn test_available_hands_are_iterated_through_all() {
        let mut grille = HandGrid::default();
        for hand in [
            YahtzeeHand::Aces,
            YahtzeeHand::FullHouse,
            YahtzeeHand::Chance,
        ] {
            grille.mark(hand);
        }

        let restantes: Vec<YahtzeeHand> = YahtzeeHand::ALL
            .into_iter()
            .filter(|hand| !grille.contains(*hand))
            .collect();

        assert_eq!(restantes.len(), 10);
        assert_eq!(
            restantes,
            YahtzeeHand::ALL
                .into_iter()
                .filter(|hand| !matches!(
                    hand,
                    YahtzeeHand::Aces | YahtzeeHand::FullHouse | YahtzeeHand::Chance
                ))
                .collect::<Vec<_>>(),
            "l'ordre est celui de la déclaration"
        );
    }

    #[test]
    fn test_lock_mask_addresses_by_die_id() {
        let deck = cup(CupId::Standard);
        let config = RunConfig::from_cup(&deck);
        let mut pool = DicePool::new(&config, &deck.sides);
        assert_eq!(pool.len(), 5);

        let avant: Vec<DieId> = pool.dice().iter().map(|die| die.id).collect();
        let retire = avant[1];
        assert!(pool.remove(retire).is_some());
        pool.push(Die::new(DieId(0), 6));

        // Deux survivants, dont un qui a changé de position par le retrait.
        let garde = (avant[0], avant[4]);
        let masque = LockMask::new([garde.0, garde.1]);

        assert!(masque.contains(garde.0));
        assert!(masque.contains(garde.1));
        assert!(!masque.contains(retire), "le dé retiré n'est pas conservé");
        assert!(!masque.is_empty());
        assert_eq!(masque.iter().collect::<Vec<_>>(), vec![garde.0, garde.1]);

        // **Retrouvés par identifiant, jamais par position.** Le retrait a
        // décalé les positions : le second survivant était en cinquième place,
        // il est en quatrième.
        for id in masque.iter() {
            let die = pool.dice().iter().find(|die| die.id == id);
            assert!(die.is_some(), "{id:?} introuvable après le retrait");
        }
        assert_eq!(
            pool.dice().iter().position(|die| die.id == garde.1),
            Some(3),
            "la position a bougé, l'identifiant non"
        );

        // Le même masque, porté par la décision.
        assert_eq!(
            HandDecision::Reroll(masque.clone()),
            HandDecision::Reroll(LockMask::new([garde.0, garde.1]))
        );
        assert_ne!(
            HandDecision::Reroll(masque),
            HandDecision::Reroll(LockMask::new([garde.1, garde.0])),
            "l'ordre d'insertion est stable et fait partie de l'égalité"
        );

        // La relance totale se lit sans compter.
        assert!(LockMask::new([]).is_empty());
    }
}
