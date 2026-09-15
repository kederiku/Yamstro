//! `BudgetShopPolicy` : elle remplit la bourse d'articles, les moins chers
//! d'abord.
//!
//! **Elle décide, elle n'applique pas.** La couche d'achat — débit de l'or,
//! refus sur la bourse, refus sur la capacité, retrait de l'article, contrat de
//! relance — vit dans la boucle, qui détient la session. La raison est dans le
//! type : `ShopView` porte `gold` **par valeur**, un entier copié. Un « débit
//! sur `view.gold` » compilerait, n'écrirait que dans la copie, et la campagne
//! mesurerait une économie sans dépenses — sans qu'aucun test ne bronche.
//!
//! Les règles ci-dessous sont donc ce que la politique **lit pour décider**, et
//! la boucle les refait en garde : un achat qu'elle ne peut plus payer est un
//! non-événement côté boucle, jamais une panique.
//!
//! # L'ordre est celui des prix, et il n'y en a qu'un
//!
//! **Le prix est la seule grandeur définie sur les quatre variantes d'article.**
//! La rareté ne l'est pas : le moteur ne la donne que sur une relique et sur un
//! consommable, le palier qu'emprunte un parchemin de grille n'est pas
//! public, et le type de rareté ne s'ordonne pas. Classer par rareté
//! obligerait donc à inventer un palier pour le parchemin, c'est-à-dire le
//! second barème que ce fichier s'interdit ailleurs.
//!
//! La mesure a tranché dans le même sens. Mille runs, Gobelet Classique,
//! Mise 1, sonde attentive à la grille : par rareté décroissante puis par prix,
//! **2 900** manches battues et **780** parchemins ; par prix croissant,
//! **3 174** manches et **1 577** parchemins, à reliques achetées constantes.
//! La cause est arithmétique — la bourse vaut sept pièces à la visite médiane,
//! la relique la plus rare payable l'épuise, et le parchemin le moins cher
//! reste sur l'étalage.
//!
//! # Elle ne thésaurise pas, et ne relance jamais
//!
//! L'or non dépensé rapporte un intérêt par tranche, jusqu'à un plafond que la
//! configuration porte. **Toute règle d'abstention mesurée est inerte ou
//! nuisible** : « ne jamais casser une tranche » fait tomber les manches
//! battues de 3 174 à 1 349 et les reliques achetées de 2 395 à 85 ; « ne
//! jamais en casser deux » ne se déclenche pas une seule fois. Le plafond
//! d'intérêt vaut vingt-cinq pièces et la bourse médiane sept : la zone où la
//! règle mordrait est exactement celle où il faut dépenser. C'est un signal de
//! calibrage, consigné pour le rapport d'équilibrage.
//!
//! **La relance est structurellement inexploitable**, et cela ne se règle pas :
//! la boucle appelle `decide` **une fois par visite**, puis applique toute la
//! liste. Un `RerollShop` porte donc sur un étalage que la politique ne reverra
//! jamais, et tout achat qui le suit désigne un rang qu'elle n'a pas vu. La
//! mesure le confirme — la condition « rien d'achetable » ne s'est pas
//! déclenchée une fois sur 3 174 visites, et relancer dès que l'or reste coûte
//! 28 manches au Gobelet Classique, 294 à celui de Fortune.
//!
//! # Elle ne vend pas
//!
//! Revendre pour libérer un slot achète plus et bat moins : 3 174 manches sans
//! revente contre 3 156 avec, 4 350 contre 4 266 au Gobelet de Fortune, pour
//! 435 reliques de plus. La reprise ne rend que la moitié du prix, et la
//! relique remplacée emporte sa contribution. `Sell` reste une variante que la
//! boucle applique ; la politique de synergie pourra s'en servir sur un motif
//! qui ne se réduit pas à un rang.

use crate::policy::{ShopAction, ShopPolicy};
use crate::view::ShopView;
use core_engine::blinds::{BlindContext, BlindDefinition, BlindType};
use core_engine::dice::{Die, DieId};
use core_engine::evaluator::HandEvaluator;
use core_engine::hands::{HandGrid, HandLevels};
use core_engine::relics::effects::effects_for;
use core_engine::relics::{CATALOG, RelicId, RelicState};
use core_engine::scoring::{Hook, TriggerCtx};
use core_engine::shop::ShopItem;
use core_engine::shop::pricing::price_of;
use rand::RngExt;
use rand_chacha::ChaCha8Rng;
use smallvec::SmallVec;

/// La sonde d'achat de référence. **Sans état** : chaque visite se décide sur
/// la seule vue, et il n'y a rien à retenir d'une visite à l'autre.
#[derive(Debug, Default)]
pub struct BudgetShopPolicy;

impl BudgetShopPolicy {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ShopPolicy for BudgetShopPolicy {
    fn name(&self) -> &'static str {
        "budget"
    }

    /// Les articles payables, les moins chers d'abord, tant que la bourse et
    /// les slots suivent.
    ///
    /// Le générateur n'est pas consommé : la politique est déterministe, et
    /// deux visites au même décor rendent la même liste.
    fn decide(&mut self, view: &ShopView<'_>, _rng: &mut ChaCha8Rng) -> SmallVec<[ShopAction; 4]> {
        retenir(view, |_| false)
    }
}

/// La sélection, **commune aux deux politiques d'achat**.
///
/// `prioritaire` est la **seule** chose qu'une politique ajoute : les articles
/// qu'elle désigne passent devant, et le reste suit l'ordre des prix. Écrire
/// deux sélections au lieu d'une rendrait l'écart entre les deux sondes
/// ininterprétable — toute différence pourrait alors venir du second barème
/// plutôt que du plan.
fn retenir(
    view: &ShopView<'_>,
    prioritaire: impl Fn(&ShopItem) -> bool,
) -> SmallVec<[ShopAction; 4]> {
    // **Le `match` est total.** Ni modificateur de dé ni consommable n'a
    // d'effet à la fin de l'Étape 6 : les acheter dépenserait de l'or contre
    // rien de mesurable, ferait chuter le solde donc les intérêts, et
    // abaisserait le taux de victoire d'un montant qu'aucun test
    // n'attribuerait. Le jour où ils auront un effet, c'est ce bras-là qu'on
    // viendra ouvrir — une omission silencieuse, elle, ne se retrouve pas.
    let mut candidats: Vec<(usize, bool, u32, bool)> = view
        .inventory
        .items
        .iter()
        .enumerate()
        .filter_map(|(rang, item)| match item {
            ShopItem::RelicCard(_) => Some((rang, !prioritaire(item), price_of(item), true)),
            ShopItem::GridUpgrade(_) => Some((rang, !prioritaire(item), price_of(item), false)),
            ShopItem::DieMod(_) | ShopItem::Consumable(_) => None,
        })
        .collect();
    // Tri **stable** : à prix égal, l'ordre de l'étalage départage, et deux
    // campagnes de même graine rendent la même liste. La priorité passe devant
    // le prix, et c'est le seul endroit où un plan s'exprime.
    candidats.sort_by_key(|(_, ordinaire, prix, _)| (*ordinaire, *prix));

    // La capacité vient de la configuration, jamais d'un littéral : le Gobelet
    // de Fortune en donne un slot de plus, et l'Étape 9 en ajoutera d'autres
    // écarts.
    let mut libres = usize::from(view.config.relic_capacity).saturating_sub(view.relics.len());
    let mut bourse = view.gold;
    let mut retenus: Vec<usize> = Vec::new();
    for (rang, _, prix, est_relique) in candidats {
        if est_relique && libres == 0 {
            continue;
        }
        if bourse < prix {
            continue;
        }
        bourse = bourse.saturating_sub(prix);
        if est_relique {
            libres = libres.saturating_sub(1);
        }
        retenus.push(rang);
    }

    // **Rangs décroissants.** L'achat retire l'article de l'étalage et décale
    // les suivants : en ordre croissant, le second achat désignerait un autre
    // article que celui choisi, la ligne de résultat resterait plausible, et la
    // contribution mesurée des reliques serait fausse sans le moindre signal.
    retenus.sort_unstable_by_key(|rang| core::cmp::Reverse(*rang));

    let mut actions: SmallVec<[ShopAction; 4]> = SmallVec::new();
    actions.extend(retenus.into_iter().map(ShopAction::Buy));
    // La visite est finie : ce qui suivrait serait ignoré, et le dire vaut
    // mieux que de laisser la liste s'arrêter d'elle-même.
    actions.push(ShopAction::Leave);
    actions
}

/// Les deux plans que le catalogue de cette étape distingue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Archetype {
    Sixes,
    Straights,
}

/// Les faces des dés témoins. Six, parce que l'un des deux archétypes porte sur
/// cette valeur : un dé qui ne peut pas la montrer ne sonderait rien.
const FACES_DU_TEMOIN: u8 = 6;

/// **Les trois témoins séparent ce qu'ils prétendent séparer, et c'est mesuré.**
///
/// Le témoin d'un archétype doit exclure les figures des *autres* archétypes,
/// sinon il mesure la figure et non la valeur. Une main de cinq six n'est pas
/// « la valeur six » : c'est aussi un Yams, un Carré et un Brelan, et elle
/// classe dans l'archétype des six deux reliques qui n'y appartiennent pas. Une
/// grande suite qui va jusqu'au six **contient un six**, et elle en exclut la
/// seule relique qui y appartienne vraiment.
///
/// Mesuré de bout en bout sur mille runs : avec les témoins confondus, la sonde
/// thématique bat **41 manches de moins** que celle qui n'a pas de plan ; avec
/// ceux-ci, **43 de plus**.
const TEMOIN_SIX: [u8; 5] = [6, 6, 1, 2, 4];
const TEMOIN_SUITE: [u8; 5] = [1, 2, 3, 4, 5];
/// Le contrôle : ni six, ni suite, ni brelan. Il attrape les inconditionnelles
/// — les dés impairs, les dés pairs, la somme paire — qui déclencheraient
/// autrement sur les deux archétypes et paraîtraient thématiques.
const TEMOIN_NEUTRE: [u8; 5] = [1, 1, 2, 2, 4];

/// Les quatre hooks du balayage.
const HOOKS: [Hook; 4] = [
    Hook::OnRoll,
    Hook::OnScoringDie,
    Hook::OnHandScored,
    Hook::OnRoundEnd,
];

fn temoin(valeurs: [u8; 5]) -> Vec<Die> {
    valeurs
        .iter()
        .enumerate()
        .map(|(rang, valeur)| {
            let mut die = Die::new(
                DieId(u32::try_from(rang).unwrap_or_default()),
                FACES_DU_TEMOIN,
            );
            die.current_value = *valeur;
            die
        })
        .collect()
}

/// La manche témoin, **identique pour les trois jeux de dés** et sans
/// modificateur : un modificateur neutraliserait des reliques et les ferait
/// passer pour neutres. Aucune de ses valeurs n'est lue par un bras d'effet ;
/// ce qui compte est qu'elles ne varient pas d'un témoin à l'autre.
fn manche_temoin() -> BlindContext {
    let blind = BlindDefinition {
        kind: BlindType::Small,
        target_score: 1,
        reward: 0,
        modifier: None,
    };
    BlindContext {
        blind,
        target_score: 1,
        current_score: 0,
        hands_remaining: 1,
        used_hands: HandGrid::default(),
    }
}

/// Vrai si la relique émet **au moins un** effet sur ce témoin.
///
/// Toutes les figures rendues par l'évaluateur sont sondées, sans en
/// privilégier aucune : choisir « la » figure du témoin serait l'arbitraire que
/// ce protocole existe pour supprimer.
fn declenche(def: RelicId, dice: &[Die], blind: &BlindContext, levels: &HandLevels) -> bool {
    for figure in &HandEvaluator::evaluate(dice) {
        let base = TriggerCtx {
            hand: figure,
            dice,
            hand_levels: levels,
            blind,
            uid: 0,
            slot: 0,
            // **L'état d'une relique fraîchement acquise**, qui est ce dont
            // une décision d'achat parle.
            //
            // Le ticket justifiait ce choix par le fait que `Disabled`
            // viderait la table. **Mesuré au banc, c'est faux** : aucun bras de
            // la fonction d'effets ne lit cet état — seul l'or de fin de manche
            // le fait —, et la neutralisation d'une relique se joue au niveau
            // de l'inventaire, pas ici. Le mutant qui met `Disabled` est donc
            // équivalent aujourd'hui. Le choix reste le bon pour la raison qui
            // le rendra vrai demain : une relique de l'Étape 9 dont les effets
            // dépendraient de son état se classerait autrement.
            state: RelicState::None,
            die: None,
            base_chips: 0,
            base_mult: 0,
            // **Vide** : non vide, elle ferait écho à une voisine qui n'existe
            // pas, et *Miroir Double* classerait selon ce qu'on lui a soufflé.
            left_effects: &[],
            roll_index: 0,
            rerolls_left: 0,
        };
        for hook in HOOKS {
            // Le `match` n'a pas de bras attrape-tout : un cinquième hook fait
            // échouer la compilation ici plutôt que d'être omis en silence.
            let par_de = match hook {
                Hook::OnScoringDie => true,
                Hook::OnRoll | Hook::OnHandScored | Hook::OnRoundEnd => false,
            };
            if par_de {
                // Sans cette boucle, `ctx.die` reste `None` et aucune relique
                // conditionnée sur la valeur d'un dé ne se déclencherait :
                // l'archétype des six serait vide.
                for id in &figure.scoring_dice {
                    let valeur = dice
                        .iter()
                        .find(|die| die.id == *id)
                        .map_or(FACES_DU_TEMOIN, |die| die.current_value);
                    if !effects_for(def, hook, &base.on_scoring_die(*id, valeur)).is_empty() {
                        return true;
                    }
                }
            } else if !effects_for(def, hook, &base.on_hand_scored()).is_empty() {
                return true;
            }
        }
    }
    false
}

/// Le classement, en **trois cas exclusifs**.
///
/// Le neutre d'abord, puis l'archétype unique, puis rien. Une relique qui sert
/// les deux plans n'en distingue aucun : la classer dans les deux ferait
/// acheter la même chose aux deux tirages, et l'écart entre les deux plans —
/// la seule chose que cette sonde mesure — s'éroderait sans qu'on sache
/// pourquoi.
fn classer(six: bool, suite: bool, neutre: bool) -> Option<Archetype> {
    if neutre || six == suite {
        return None;
    }
    if six {
        Some(Archetype::Sixes)
    } else {
        Some(Archetype::Straights)
    }
}

/// La table, sondée sur **le catalogue** et jamais sur une liste écrite à la
/// main.
///
/// Deux identifiants en dur passeraient tous les tests d'aujourd'hui, et à
/// l'Étape 9 la sonde ne saurait classer aucune des reliques ajoutées : elle
/// achèterait au hasard sous un nom qui promet un plan, et le tableau resterait
/// crédible.
fn table_de_synergie() -> Vec<(RelicId, Option<Archetype>)> {
    let levels = HandLevels::default();
    let blind = manche_temoin();
    let (six, suite, neutre) = (
        temoin(TEMOIN_SIX),
        temoin(TEMOIN_SUITE),
        temoin(TEMOIN_NEUTRE),
    );
    CATALOG
        .iter()
        .copied()
        .map(|def| {
            let neutre = declenche(def, &neutre, &blind, &levels);
            (
                def,
                classer(
                    declenche(def, &six, &blind, &levels),
                    declenche(def, &suite, &blind, &levels),
                    neutre,
                ),
            )
        })
        .collect()
}

/// L'archétype tiré et la table qui va avec.
#[derive(Debug, Clone)]
struct Plan {
    archetype: Archetype,
    table: Vec<(RelicId, Option<Archetype>)>,
}

impl Plan {
    /// **Un seul tirage, sur le flux du harnais.** Un tirage sur un flux de la
    /// run décalerait la séquence des étalages : les deux politiques d'achat ne
    /// verraient plus la même boutique pour la même graine, et l'écart entre
    /// elles — la seule raison d'être de cette sonde — mesurerait la divergence
    /// du flux, pas la valeur d'un plan.
    fn tirer(rng: &mut ChaCha8Rng) -> Self {
        let archetype = if rng.random_range(0..2u32) == 0 {
            Archetype::Sixes
        } else {
            Archetype::Straights
        };
        Self {
            archetype,
            table: table_de_synergie(),
        }
    }

    fn porte(&self, item: &ShopItem) -> bool {
        let ShopItem::RelicCard(def) = item else {
            return false;
        };
        self.table
            .iter()
            .any(|(entree, classe)| entree == def && *classe == Some(self.archetype))
    }
}

/// La sonde à plan. **Une instance par run**, jamais partagée : l'archétype
/// vaut pour un run entier.
#[derive(Debug, Default)]
pub struct SynergyShopPolicy {
    plan: Option<Plan>,
}

impl SynergyShopPolicy {
    #[must_use]
    pub fn new() -> Self {
        Self { plan: None }
    }
}

impl ShopPolicy for SynergyShopPolicy {
    fn name(&self) -> &'static str {
        "synergy"
    }

    /// **Le tirage a lieu au premier appel**, c'est-à-dire à la première entrée
    /// en boutique et non au lancement du run : c'est le seul moment où la
    /// politique reçoit un flux. Un run qui meurt avant sa première manche
    /// battue n'en tire donc aucun — mesuré, 149 runs sur 1 000.
    ///
    /// Un tirage par visite ferait vaciller l'archétype d'une boutique à
    /// l'autre : la politique n'aurait plus de plan, et la sonde ne mesurerait
    /// plus qu'un bruit thématique.
    fn decide(&mut self, view: &ShopView<'_>, rng: &mut ChaCha8Rng) -> SmallVec<[ShopAction; 4]> {
        let plan = self.plan.get_or_insert_with(|| Plan::tirer(rng));
        retenir(view, |item| plan.porte(item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PolicyKind, ShopPolicyKind, SimConfig};
    use crate::policy::grid_aware::GridAwarePolicy;
    use crate::run::simulate_with;
    use core_engine::config::RunConfig;
    use core_engine::consumables::ConsumableId;
    use core_engine::cups::CupId;
    use core_engine::cups::definitions::cup;
    use core_engine::dice::DieModifier;
    use core_engine::economy::INTEREST_TRANCHE;
    use core_engine::hands::YahtzeeHand;
    use core_engine::relics::{RelicId, RelicInventory, rarity_of};
    use core_engine::rng::RunRng;
    use core_engine::shop::ShopInventory;
    use rand_chacha::rand_core::{Rng, SeedableRng};

    fn config_de(id: CupId) -> RunConfig {
        RunConfig::from_cup(&cup(id))
    }

    fn etalage(items: Vec<ShopItem>) -> ShopInventory {
        ShopInventory {
            items,
            ..ShopInventory::default()
        }
    }

    fn flux() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(7)
    }

    /// Les rangs d'achat que rend la politique, dans l'ordre où elle les rend.
    fn rangs(actions: &[ShopAction]) -> Vec<usize> {
        actions
            .iter()
            .filter_map(|action| match action {
                ShopAction::Buy(rang) => Some(*rang),
                _ => None,
            })
            .collect()
    }

    /// Rejoue le retrait que fera la boucle — `items.remove(rang)` — et rend
    /// les articles réellement sortis de l'étalage.
    ///
    /// **C'est la moitié qui discrimine.** Une liste de rangs croissants a
    /// exactement la même forme qu'une liste décroissante ; seul le décalage
    /// dit laquelle achète ce qu'elle a choisi.
    fn articles_sortis(stock: &ShopInventory, actions: &[ShopAction]) -> Vec<ShopItem> {
        let mut items = stock.items.clone();
        let mut sortis = Vec::new();
        for action in actions {
            match action {
                ShopAction::Leave => break,
                ShopAction::Buy(rang) if *rang < items.len() => sortis.push(items.remove(*rang)),
                _ => {}
            }
        }
        sortis
    }

    /// Ce que la politique a décidé, vu de l'extérieur : la sonde enveloppe la
    /// vraie politique et note ce que la boucle lui montre.
    ///
    /// **C'est le seul observateur disponible.** Le résultat d'un run ne porte
    /// ni l'inventaire, ni les niveaux de figure : un test de bout en bout ne
    /// peut les lire qu'à travers la vue de la visite suivante.
    #[derive(Default)]
    struct Mouchard {
        interne: BudgetShopPolicy,
        visites: u32,
        parchemins: u32,
        relances: u32,
        achats_sans_effet: u32,
        hors_bourse: u32,
        capacite_depassee: u32,
        reliques_max: usize,
        reliques_sur_plein: u32,
    }

    impl ShopPolicy for Mouchard {
        fn name(&self) -> &'static str {
            "mouchard"
        }

        fn decide(
            &mut self,
            view: &ShopView<'_>,
            rng: &mut ChaCha8Rng,
        ) -> SmallVec<[ShopAction; 4]> {
            self.visites = self.visites.saturating_add(1);
            self.reliques_max = self.reliques_max.max(view.relics.len());
            if view.relics.len() > usize::from(view.config.relic_capacity) {
                self.capacite_depassee = self.capacite_depassee.saturating_add(1);
            }

            let actions = self.interne.decide(view, rng);

            // **La décision, et non son résultat.** La boucle refuse d'elle-même
            // un achat de relique sur un inventaire plein : l'inventaire ne
            // déborde donc jamais, même si la politique ignore la règle. Ce
            // qu'il faut voir, c'est la proposition — une politique qui la fait
            // a réservé la bourse pour un article qu'elle n'obtiendra pas, et a
            // renoncé à un parchemin qu'elle pouvait payer.
            let plein = view.relics.len() >= usize::from(view.config.relic_capacity);

            let mut bourse = view.gold;
            for action in &actions {
                match action {
                    ShopAction::RerollShop => self.relances = self.relances.saturating_add(1),
                    ShopAction::Buy(rang) => {
                        let Some(item) = view.inventory.items.get(*rang) else {
                            continue;
                        };
                        match item {
                            ShopItem::GridUpgrade(_) => {
                                self.parchemins = self.parchemins.saturating_add(1);
                            }
                            ShopItem::DieMod(_) | ShopItem::Consumable(_) => {
                                self.achats_sans_effet = self.achats_sans_effet.saturating_add(1);
                            }
                            ShopItem::RelicCard(_) => {
                                if plein {
                                    self.reliques_sur_plein =
                                        self.reliques_sur_plein.saturating_add(1);
                                }
                            }
                        }
                        let prix = price_of(item);
                        if bourse < prix {
                            self.hors_bourse = self.hors_bourse.saturating_add(1);
                        }
                        bourse = bourse.saturating_sub(prix);
                    }
                    ShopAction::Sell(_) | ShopAction::Leave => {}
                }
            }
            actions
        }
    }

    /// Mille runs pilotés par la sonde attentive à la grille, la politique
    /// d'achat sous observation.
    fn campagne(id: CupId) -> Mouchard {
        let config = SimConfig {
            runs: 1,
            seed_base: 1,
            cups: vec![id],
            stakes: vec![1],
            policy: PolicyKind::GridAware,
            shop_policy: ShopPolicyKind::Budget,
            threads: 1,
        };
        let mut mouchard = Mouchard::default();
        for index in 0..1_000u64 {
            let mut politique = GridAwarePolicy::default();
            let (_, _) = simulate_with(&config, 1_000 + index, &mut politique, &mut mouchard);
        }
        mouchard
    }

    #[test]
    fn test_budget_buys_the_cheapest_first() {
        // Rare à huit, Commune à quatre, Peu commune à six : l'ordre de
        // l'étalage n'est pas celui des prix.
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::DivineYahtzee),
            ShopItem::RelicCard(RelicId::PolishedStone),
            ShopItem::RelicCard(RelicId::GhostDie),
        ]);
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let vue = ShopView {
            inventory: &stock,
            gold: 10,
            relics: &reliques,
            config: &config,
        };

        let actions = BudgetShopPolicy::new().decide(&vue, &mut flux());

        // Quatre puis six tiennent dans dix ; huit n'entre plus.
        let sortis = articles_sortis(&stock, &actions);
        assert_eq!(
            sortis,
            vec![
                ShopItem::RelicCard(RelicId::GhostDie),
                ShopItem::RelicCard(RelicId::PolishedStone),
            ],
            "la politique n'a pas pris les deux moins chers"
        );
    }

    #[test]
    fn test_multiple_buys_are_ordered_by_decreasing_index() {
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::PolishedStone),
            ShopItem::GridUpgrade(YahtzeeHand::Fives),
            ShopItem::RelicCard(RelicId::TripletMaster),
        ]);
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let vue = ShopView {
            inventory: &stock,
            gold: 100,
            relics: &reliques,
            config: &config,
        };

        let actions = BudgetShopPolicy::new().decide(&vue, &mut flux());
        let rangs = rangs(&actions);
        assert_eq!(rangs.len(), 3, "les trois articles sont payables");
        assert!(
            rangs.windows(2).all(|paire| paire[0] > paire[1]),
            "rangs non décroissants : {rangs:?}"
        );

        // Et la conséquence, rejouée : les trois articles sortis sont bien les
        // trois de l'étalage, aucun décalage n'ayant désigné un voisin.
        let mut sortis = articles_sortis(&stock, &actions);
        sortis.sort_by_key(|item| format!("{item:?}"));
        let mut attendus = stock.items.clone();
        attendus.sort_by_key(|item| format!("{item:?}"));
        assert_eq!(sortis, attendus);
    }

    #[test]
    fn test_budget_policy_never_buys_die_mods_or_consumables() {
        let stock = etalage(vec![
            ShopItem::DieMod(DieModifier::BonusChips(30)),
            ShopItem::Consumable(ConsumableId::RuneOfFate),
        ]);
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let vue = ShopView {
            inventory: &stock,
            gold: 1_000,
            relics: &reliques,
            config: &config,
        };
        assert!(
            rangs(&BudgetShopPolicy::new().decide(&vue, &mut flux())).is_empty(),
            "un article sans effet a été acheté"
        );

        assert_eq!(
            campagne(CupId::Standard).achats_sans_effet,
            0,
            "un article sans effet a été acheté en campagne"
        );
    }

    #[test]
    fn test_budget_policy_never_rerolls() {
        // Une bourse qui paierait dix relances, et rien d'achetable : le seul
        // décor où une politique relancerait.
        let stock = etalage(vec![ShopItem::Consumable(ConsumableId::RuneOfFate)]);
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let vue = ShopView {
            inventory: &stock,
            gold: 1_000,
            relics: &reliques,
            config: &config,
        };
        let actions = BudgetShopPolicy::new().decide(&vue, &mut flux());
        assert!(
            !actions.contains(&ShopAction::RerollShop),
            "une relance a été demandée"
        );

        assert_eq!(campagne(CupId::Standard).relances, 0);
    }

    #[test]
    fn test_budget_does_not_propose_an_unaffordable_buy() {
        let stock = etalage(vec![ShopItem::RelicCard(RelicId::DivineYahtzee)]);
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let prix = price_of(&stock.items[0]);
        let vue = ShopView {
            inventory: &stock,
            gold: prix.saturating_sub(1),
            relics: &reliques,
            config: &config,
        };
        assert!(
            rangs(&BudgetShopPolicy::new().decide(&vue, &mut flux())).is_empty(),
            "un achat a été proposé sans la bourse"
        );

        // La moitié acceptation : à un de plus, il est proposé. Sans elle, une
        // politique qui n'achète jamais rien passerait.
        let vue = ShopView { gold: prix, ..vue };
        assert_eq!(
            rangs(&BudgetShopPolicy::new().decide(&vue, &mut flux())),
            [0]
        );

        assert_eq!(
            campagne(CupId::Standard).hors_bourse,
            0,
            "la somme des achats d'une visite dépasse la bourse"
        );
    }

    #[test]
    fn test_budget_policy_spends_below_the_interest_ceiling() {
        // Sous le plafond d'intérêts, et l'achat casse une tranche : c'est
        // exactement le décor où une règle d'abstention retiendrait l'or.
        let config = config_de(CupId::Standard);
        let plafond = config.max_interest.saturating_mul(INTEREST_TRANCHE);
        let stock = etalage(vec![ShopItem::RelicCard(RelicId::PolishedStone)]);
        let reliques = RelicInventory::new(config.relic_capacity);
        let bourse = INTEREST_TRANCHE.saturating_add(2);
        assert!(bourse < plafond, "le décor ne teste pas ce qu'il annonce");
        let prix = price_of(&stock.items[0]);
        assert!(
            bourse.saturating_sub(prix) / INTEREST_TRANCHE < bourse / INTEREST_TRANCHE,
            "l'achat ne casse aucune tranche : le décor est sans objet"
        );

        let vue = ShopView {
            inventory: &stock,
            gold: bourse,
            relics: &reliques,
            config: &config,
        };
        assert_eq!(
            rangs(&BudgetShopPolicy::new().decide(&vue, &mut flux())),
            [0],
            "la politique thésaurise"
        );
    }

    #[test]
    fn test_budget_policy_buys_grid_upgrades() {
        // Un parchemin et une Rare, la bourse ne payant que l'un des deux.
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::DivineYahtzee),
            ShopItem::GridUpgrade(YahtzeeHand::Fives),
        ]);
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let vue = ShopView {
            inventory: &stock,
            gold: price_of(&stock.items[1]),
            relics: &reliques,
            config: &config,
        };
        assert_eq!(
            articles_sortis(&stock, &BudgetShopPolicy::new().decide(&vue, &mut flux())),
            vec![ShopItem::GridUpgrade(YahtzeeHand::Fives)],
            "le parchemin a été laissé sur l'étalage"
        );

        // **Le total, et non la médiane.** Près de la moitié des runs n'achètent
        // aucun parchemin — la médiane tiendrait à quelques dizaines de runs et
        // basculerait au premier changement de politique de main.
        let mouchard = campagne(CupId::Standard);
        assert!(
            mouchard.parchemins >= 1_000,
            "mille runs n'ont acheté que {} parchemins sur {} visites",
            mouchard.parchemins,
            mouchard.visites
        );
    }

    #[test]
    fn test_relic_capacity_respected() {
        // Aucun littéral de capacité : les deux gobelets la portent, et c'est
        // le sixième slot du Gobelet de Fortune qui discrimine.
        for id in [CupId::Standard, CupId::Fortune] {
            let capacite = usize::from(config_de(id).relic_capacity);
            let mouchard = campagne(id);
            assert_eq!(mouchard.capacite_depassee, 0, "{id:?} : capacité dépassée");
            assert_eq!(
                mouchard.reliques_max, capacite,
                "{id:?} : la campagne n'a jamais rempli l'inventaire, le test ne prouve rien"
            );
            assert_eq!(
                mouchard.reliques_sur_plein, 0,
                "{id:?} : une relique a été proposée sur un inventaire plein"
            );
        }

        // Le cas isolé : inventaire plein, de quoi payer les deux articles, et
        // seul le parchemin est proposé.
        let config = config_de(CupId::Standard);
        let mut reliques = RelicInventory::new(config.relic_capacity);
        for _ in 0..config.relic_capacity {
            reliques.add_relic(RelicId::PolishedStone);
        }
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::PolishedStone),
            ShopItem::GridUpgrade(YahtzeeHand::Fives),
        ]);
        let vue = ShopView {
            inventory: &stock,
            gold: 1_000,
            relics: &reliques,
            config: &config,
        };
        assert_eq!(
            articles_sortis(&stock, &BudgetShopPolicy::new().decide(&vue, &mut flux())),
            vec![ShopItem::GridUpgrade(YahtzeeHand::Fives)],
            "une relique a été proposée sans slot libre"
        );
    }

    /// **Le raccord avec l'aiguillage et avec la colonne du tableau.** Le nom
    /// ne se compare pas à un littéral recopié : il se compare au libellé que
    /// la ligne de commande expose pour la variante, seule source de vérité.
    /// Un nom qui dérive ferait rendre un résultat **sous un mauvais nom**,
    /// sans la moindre erreur à l'exécution.
    #[test]
    fn test_name_matches_the_command_line_value() {
        use clap::ValueEnum;
        let libelle = ShopPolicyKind::Budget
            .to_possible_value()
            .map(|valeur| valeur.get_name().to_owned());
        assert_eq!(libelle.as_deref(), Some(BudgetShopPolicy::new().name()));
    }

    #[test]
    fn test_budget_is_deterministic() {
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::PolishedStone),
            ShopItem::GridUpgrade(YahtzeeHand::Fives),
            ShopItem::RelicCard(RelicId::DivineYahtzee),
        ]);
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let vue = ShopView {
            inventory: &stock,
            gold: 9,
            relics: &reliques,
            config: &config,
        };
        let premiere = BudgetShopPolicy::new().decide(&vue, &mut flux());
        let seconde = BudgetShopPolicy::new().decide(&vue, &mut flux());
        assert_eq!(premiere.as_slice(), seconde.as_slice());
    }

    #[test]
    fn test_shop_policy_does_not_advance_run_rng() {
        let mut run = RunRng::from_seed(31);
        let temoin = run.clone();
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::PolishedStone),
            ShopItem::GridUpgrade(YahtzeeHand::Fives),
        ]);
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let vue = ShopView {
            inventory: &stock,
            gold: 100,
            relics: &reliques,
            config: &config,
        };

        let mut politique = BudgetShopPolicy::new();
        for _ in 0..50 {
            let _ = politique.decide(&vue, &mut flux());
        }

        // **Comparaison de tirages, jamais de structs** : l'égalité d'état
        // interne n'est pas le contrat, ce sont les séquences qui doivent
        // coïncider.
        let (mut apres, mut avant) = (run.clone(), temoin.clone());
        for _ in 0..8 {
            assert_eq!(apres.dice.next_u32(), avant.dice.next_u32());
            assert_eq!(apres.shop.next_u32(), avant.shop.next_u32());
            assert_eq!(apres.boss.next_u32(), avant.boss.next_u32());
            assert_eq!(
                apres.relic_effects.next_u32(),
                avant.relic_effects.next_u32()
            );
        }
        let _ = &mut run;
    }

    // ---------------------------------------------------------------- synergie

    /// Mille runs pilotés par la sonde à plan, l'archétype observé à chaque
    /// visite.
    fn campagne_synergie(id: CupId) -> (Vec<Archetype>, u32, usize) {
        let config = SimConfig {
            runs: 1,
            seed_base: 1,
            cups: vec![id],
            stakes: vec![1],
            policy: PolicyKind::GridAware,
            shop_policy: ShopPolicyKind::Synergy,
            threads: 1,
        };
        let mut archetypes = Vec::new();
        let mut visites = 0u32;
        let mut reliques_max = 0usize;
        for index in 0..1_000u64 {
            let mut politique = GridAwarePolicy::default();
            let mut achat = MouchardSynergie::default();
            let (_, _) = simulate_with(&config, 1_000 + index, &mut politique, &mut achat);
            visites = visites.saturating_add(achat.visites);
            reliques_max = reliques_max.max(achat.reliques_max);
            archetypes.extend(achat.vus);
        }
        (archetypes, visites, reliques_max)
    }

    #[derive(Default)]
    struct MouchardSynergie {
        interne: SynergyShopPolicy,
        vus: Vec<Archetype>,
        visites: u32,
        reliques_max: usize,
        capacite_depassee: u32,
        reliques_sur_plein: u32,
    }

    impl ShopPolicy for MouchardSynergie {
        fn name(&self) -> &'static str {
            "mouchard-synergie"
        }

        fn decide(
            &mut self,
            view: &ShopView<'_>,
            rng: &mut ChaCha8Rng,
        ) -> SmallVec<[ShopAction; 4]> {
            self.visites = self.visites.saturating_add(1);
            self.reliques_max = self.reliques_max.max(view.relics.len());
            let capacite = usize::from(view.config.relic_capacity);
            if view.relics.len() > capacite {
                self.capacite_depassee = self.capacite_depassee.saturating_add(1);
            }
            let plein = view.relics.len() >= capacite;
            let actions = self.interne.decide(view, rng);
            if let Some(plan) = self.interne.plan.as_ref() {
                self.vus.push(plan.archetype);
            }
            for action in &actions {
                if let ShopAction::Buy(rang) = action
                    && plein
                    && matches!(
                        view.inventory.items.get(*rang),
                        Some(ShopItem::RelicCard(_))
                    )
                {
                    self.reliques_sur_plein = self.reliques_sur_plein.saturating_add(1);
                }
            }
            actions
        }
    }

    /// Compte les mots de trente-deux bits qu'une décision consomme, en
    /// cherchant de combien un témoin vierge doit avancer pour rendre le même
    /// mot suivant. **Un tirage se compte en mots de trente-deux bits** : une
    /// lecture de soixante-quatre en consommerait deux, et mesurer dans la
    /// mauvaise unité fait échouer un test dont la propriété est vraie.
    fn mots_consommes(politique: &mut SynergyShopPolicy, flux_courant: &mut ChaCha8Rng) -> u32 {
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let stock = etalage(vec![ShopItem::RelicCard(RelicId::PolishedStone)]);
        let vue = ShopView {
            inventory: &stock,
            gold: 100,
            relics: &reliques,
            config: &config,
        };
        let mut temoin_flux = flux_courant.clone();
        let _ = politique.decide(&vue, flux_courant);
        let suivant = flux_courant.clone().next_u32();
        for mots in 0..=16u32 {
            if temoin_flux.next_u32() == suivant {
                return mots;
            }
        }
        u32::MAX
    }

    #[test]
    fn test_synergy_witnesses_are_not_confounded() {
        // **Le cœur du protocole.** Un témoin d'archétype qui porte la figure
        // d'un autre archétype mesure la figure et non la valeur : mesuré, cinq
        // six classent le Maître du Brelan et le Yams Divin dans l'archétype des
        // six, et une grande suite qui monte au six en exclut la Pyramide.
        let six = temoin(TEMOIN_SIX);
        let figures_six: Vec<YahtzeeHand> = HandEvaluator::evaluate(&six)
            .iter()
            .map(|f| f.hand)
            .collect();
        for interdite in [
            YahtzeeHand::ThreeOfAKind,
            YahtzeeHand::FourOfAKind,
            YahtzeeHand::Yahtzee,
            YahtzeeHand::SmallStraight,
            YahtzeeHand::LargeStraight,
            YahtzeeHand::FullHouse,
        ] {
            assert!(
                !figures_six.contains(&interdite),
                "le témoin des six porte {interdite:?}"
            );
        }
        assert!(
            six.iter().any(|die| die.current_value == FACES_DU_TEMOIN),
            "le témoin des six ne montre aucun six"
        );

        let suite = temoin(TEMOIN_SUITE);
        assert!(
            !suite.iter().any(|die| die.current_value == FACES_DU_TEMOIN),
            "le témoin des suites contient un six"
        );
        let figures_suite: Vec<YahtzeeHand> = HandEvaluator::evaluate(&suite)
            .iter()
            .map(|f| f.hand)
            .collect();
        assert!(
            figures_suite.contains(&YahtzeeHand::LargeStraight),
            "le témoin des suites ne forme pas de suite"
        );

        // Le contrôle ne porte ni l'un ni l'autre.
        let neutre = temoin(TEMOIN_NEUTRE);
        assert!(
            !neutre
                .iter()
                .any(|die| die.current_value == FACES_DU_TEMOIN)
        );
        let figures_neutre: Vec<YahtzeeHand> = HandEvaluator::evaluate(&neutre)
            .iter()
            .map(|f| f.hand)
            .collect();
        for interdite in [
            YahtzeeHand::ThreeOfAKind,
            YahtzeeHand::SmallStraight,
            YahtzeeHand::LargeStraight,
        ] {
            assert!(!figures_neutre.contains(&interdite), "{interdite:?}");
        }
    }

    #[test]
    fn test_synergy_classification_has_three_exclusive_cases() {
        // Les huit combinaisons, sans en oublier une.
        assert_eq!(classer(true, false, false), Some(Archetype::Sixes));
        assert_eq!(classer(false, true, false), Some(Archetype::Straights));
        // Les deux archétypes à la fois : neutre, et non les deux.
        assert_eq!(classer(true, true, false), None);
        assert_eq!(classer(false, false, false), None);
        // Le contrôle l'emporte sur tout le reste.
        for six in [false, true] {
            for suite in [false, true] {
                assert_eq!(classer(six, suite, true), None, "{six} {suite}");
            }
        }
    }

    #[test]
    fn test_synergy_table_is_not_empty() {
        let table = table_de_synergie();
        assert_eq!(
            table.len(),
            CATALOG.len(),
            "la table filtre le catalogue au lieu de l'itérer"
        );
        for archetype in [Archetype::Sixes, Archetype::Straights] {
            assert!(
                table.iter().any(|(_, classe)| *classe == Some(archetype)),
                "aucune relique dans {archetype:?} : un témoin est mal construit, \
                 ce n'est pas un catalogue sans synergie"
            );
        }
        // Et le classement est celui que la prose du corpus annonce.
        let dans = |def: RelicId| {
            table
                .iter()
                .find(|(entree, _)| *entree == def)
                .and_then(|(_, classe)| *classe)
        };
        assert_eq!(dans(RelicId::PyramidOfSixes), Some(Archetype::Sixes));
        assert_eq!(dans(RelicId::StellarAlignment), Some(Archetype::Straights));
        assert_eq!(dans(RelicId::TripletMaster), None);
        assert_eq!(dans(RelicId::DivineYahtzee), None);
    }

    #[test]
    fn test_synergy_probe_sweeps_the_four_hooks() {
        assert_eq!(HOOKS.len(), 4);
        for hook in HOOKS {
            // Sans bras attrape-tout : un cinquième hook fait échouer la
            // compilation de ce test, et l'auteur voit la liste juste au-dessus.
            let _: () = match hook {
                Hook::OnRoll | Hook::OnScoringDie | Hook::OnHandScored | Hook::OnRoundEnd => (),
            };
        }
        for (rang, hook) in HOOKS.iter().enumerate() {
            assert!(
                !HOOKS[..rang].contains(hook),
                "{hook:?} est balayé deux fois"
            );
        }
    }

    #[test]
    fn test_synergy_prefers_its_archetype() {
        // Les deux reliques d'archétype et une Commune moins chère : quel que
        // soit le tirage, la relique du plan passe devant.
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::PolishedStone),
            ShopItem::RelicCard(RelicId::PyramidOfSixes),
            ShopItem::RelicCard(RelicId::StellarAlignment),
        ]);
        let bourse = price_of(&stock.items[1]);
        assert!(
            bourse > price_of(&stock.items[0]),
            "le décor ne discrimine pas : la Commune est aussi chère"
        );
        let vue = ShopView {
            inventory: &stock,
            gold: bourse,
            relics: &reliques,
            config: &config,
        };

        let mut politique = SynergyShopPolicy::new();
        let sortis = articles_sortis(&stock, &politique.decide(&vue, &mut flux()));
        let attendu = match politique.plan.as_ref().map(|plan| plan.archetype) {
            Some(Archetype::Sixes) => RelicId::PyramidOfSixes,
            Some(Archetype::Straights) => RelicId::StellarAlignment,
            None => panic!("aucun archétype tiré"),
        };
        assert_eq!(sortis, vec![ShopItem::RelicCard(attendu)]);

        // Et la sonde sans plan, sur le même étalage, prend la moins chère.
        assert_eq!(
            articles_sortis(&stock, &BudgetShopPolicy::new().decide(&vue, &mut flux())),
            vec![ShopItem::RelicCard(RelicId::PolishedStone)],
            "l'écart entre les deux sondes a disparu"
        );
    }

    #[test]
    fn test_synergy_falls_back_to_budget_order() {
        // Aucune relique d'archétype : les deux listes coïncident, à l'article
        // près.
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::DivineYahtzee),
            ShopItem::GridUpgrade(YahtzeeHand::Fives),
            ShopItem::RelicCard(RelicId::PolishedStone),
        ]);
        let vue = ShopView {
            inventory: &stock,
            gold: 12,
            relics: &reliques,
            config: &config,
        };
        let synergie = SynergyShopPolicy::new().decide(&vue, &mut flux());
        let budget = BudgetShopPolicy::new().decide(&vue, &mut flux());
        assert_eq!(synergie.as_slice(), budget.as_slice());
        assert!(!rangs(&synergie).is_empty(), "le décor n'achète rien");
    }

    #[test]
    fn test_archetype_is_drawn_once_per_run() {
        let mut politique = SynergyShopPolicy::new();
        let mut courant = flux();
        assert_eq!(
            mots_consommes(&mut politique, &mut courant),
            1,
            "le premier appel ne tire pas exactement un mot"
        );
        for visite in 2..=3 {
            assert_eq!(
                mots_consommes(&mut politique, &mut courant),
                0,
                "la visite {visite} a retiré un archétype"
            );
        }
    }

    #[test]
    fn test_synergy_archetype_is_seed_stable() {
        // Deux runs de même graine tirent le même plan, et il ne bouge pas
        // d'une visite à l'autre. **Le nombre de visites est celui du run** :
        // mesuré, la médiane vaut deux et le maximum quatorze.
        let (premiers, visites, _) = campagne_synergie(CupId::Standard);
        let (seconds, _, _) = campagne_synergie(CupId::Standard);
        assert_eq!(premiers, seconds, "deux campagnes de même graine divergent");
        assert_eq!(
            premiers.len(),
            visites as usize,
            "un archétype manque à une visite"
        );
        assert!(
            visites > 1_000,
            "la campagne ne porte pas de run à plusieurs visites"
        );
        // Les deux plans sortent : un tirage figé passerait sans cela.
        for archetype in [Archetype::Sixes, Archetype::Straights] {
            assert!(premiers.contains(&archetype), "{archetype:?} jamais tiré");
        }
    }

    #[test]
    fn test_synergy_respects_capacity() {
        // Inventaire plein d'articles hors archétype, étalage entier dans
        // l'archétype : aucun achat de relique, et aucun littéral de capacité.
        let config = config_de(CupId::Standard);
        let mut reliques = RelicInventory::new(config.relic_capacity);
        for _ in 0..config.relic_capacity {
            reliques.add_relic(RelicId::PolishedStone);
        }
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::PyramidOfSixes),
            ShopItem::RelicCard(RelicId::StellarAlignment),
        ]);
        let vue = ShopView {
            inventory: &stock,
            gold: 1_000,
            relics: &reliques,
            config: &config,
        };
        assert!(
            rangs(&SynergyShopPolicy::new().decide(&vue, &mut flux())).is_empty(),
            "une relique a été proposée sans slot libre"
        );

        for id in [CupId::Standard, CupId::Fortune] {
            let capacite = usize::from(config_de(id).relic_capacity);
            let (_, _, reliques_max) = campagne_synergie(id);
            assert_eq!(reliques_max, capacite, "{id:?}");
        }
    }

    #[test]
    fn test_synergy_uses_sim_rng_only() {
        let run = RunRng::from_seed(53);
        let temoin_run = run.clone();
        let config = config_de(CupId::Standard);
        let reliques = RelicInventory::new(config.relic_capacity);
        let stock = etalage(vec![
            ShopItem::RelicCard(RelicId::PyramidOfSixes),
            ShopItem::GridUpgrade(YahtzeeHand::Fives),
        ]);
        let vue = ShopView {
            inventory: &stock,
            gold: 100,
            relics: &reliques,
            config: &config,
        };

        let mut politique = SynergyShopPolicy::new();
        for _ in 0..50 {
            let _ = politique.decide(&vue, &mut flux());
        }

        let (mut apres, mut avant) = (run.clone(), temoin_run.clone());
        for _ in 0..8 {
            assert_eq!(apres.dice.next_u32(), avant.dice.next_u32());
            assert_eq!(apres.shop.next_u32(), avant.shop.next_u32());
            assert_eq!(apres.boss.next_u32(), avant.boss.next_u32());
            assert_eq!(
                apres.relic_effects.next_u32(),
                avant.relic_effects.next_u32()
            );
        }
    }

    #[test]
    fn test_synergy_name_matches_the_command_line_value() {
        use clap::ValueEnum;
        let libelle = ShopPolicyKind::Synergy
            .to_possible_value()
            .map(|valeur| valeur.get_name().to_owned());
        assert_eq!(libelle.as_deref(), Some(SynergyShopPolicy::new().name()));
        assert_ne!(
            SynergyShopPolicy::new().name(),
            BudgetShopPolicy::new().name()
        );
        let _ = rarity_of(RelicId::PyramidOfSixes);
    }
}
