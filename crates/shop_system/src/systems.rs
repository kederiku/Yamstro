//! Achat, revente et relance. **Trois consommateurs d'intention, rien d'autre.**
//!
//! # Aucune dette, jamais
//!
//! L'or de la session est la seule source de vérité, et il est non signé. Tout
//! débit sature, tout crédit sature, et **une transaction refusée ne modifie
//! rien** : ni l'or, ni l'étalage, ni les inventaires. La vérification précède
//! toujours le débit ; débiter puis rembourser laisserait une fenêtre où le
//! moindre retour anticipé offre de l'or.
//!
//! # Ce que ce module n'achète pas, et pourquoi
//!
//! `ShopItem` a quatre variantes ; trois ont une destination. Une relique entre
//! dans son inventaire, un parchemin monte une figure, un consommable entre
//! dans le sien. **Un modificateur de dé n'a aucune cible spécifiée** : les dés
//! sont des entités de la crate d'états, ils n'existent pas pendant la
//! boutique, et le corpus ne dit pas lequel recevrait le modificateur. Son
//! achat est donc refusé, et le motif est ici plutôt que dans un `todo!`.
//!
//! Trois inconnues du même objet s'accumulent au registre de l'Étape 6 bis :
//! les magnitudes offertes, leur prix unique, et maintenant leur cible.

use bevy::prelude::*;
use core_engine::consumables::ConsumableInventory;
use core_engine::relics::RelicInventory;
use core_engine::shop::generator::generate_shop;
use core_engine::shop::pricing::{bump_reroll_cost, price_of, sell_value};
use core_engine::shop::{ShopInventory, ShopItem};
use game_state::resources::RunSession;

use crate::{PurchaseEvent, RerollEvent, SellEvent, ShopSet};

/// La règle de capacité, **écrite une fois**.
///
/// Strictement inférieur : un inventaire de cinq slots en accepte cinq, pas
/// six. La capacité vient de la configuration du gobelet, jamais d'un littéral,
/// le *Gobelet de Fortune* en donnant six là où le standard en donne cinq.
///
/// **Elle dit la règle ; l'autorité mécanique est la taille du tableau.** Les
/// deux coïncident par construction, les inventaires étant dimensionnés depuis
/// cette même configuration, et le refus final vient d'`add_relic` qui rend
/// `None`. Cette phrase dit laquelle tient si jamais elles divergeaient.
pub fn has_free_slot(occupied: usize, capacity: u8) -> bool {
    occupied < usize::from(capacity)
}

/// Les rangs d'achat retenus pour cette frame, du plus grand au plus petit.
///
/// **Un rang, une intention.** Un double clic sur la même carte est une seule
/// intention : les doublons sont écartés. Et les rangs sont traités du plus
/// grand au plus petit, car un retrait décale ceux qui suivent : l'ordre
/// décroissant garantit qu'un rang en attente reste valide.
fn rangs_a_traiter(messages: &mut MessageReader<PurchaseEvent>) -> Vec<usize> {
    let mut rangs: Vec<usize> = Vec::new();
    for achat in messages.read() {
        if !rangs.contains(&achat.item_index) {
            rangs.push(achat.item_index);
        }
    }
    rangs.sort_unstable_by(|gauche, droite| droite.cmp(gauche));
    rangs
}

/// La place existe-t-elle pour cet article ?
///
/// Rend `false` pour un modificateur de dé : il n'a pas de cible (voir le `//!`).
fn place_disponible(
    item: &ShopItem,
    session: &RunSession,
    relics: &RelicInventory,
    consumables: &ConsumableInventory,
) -> bool {
    match item {
        ShopItem::RelicCard(_) => has_free_slot(relics.len(), session.config.relic_capacity),
        ShopItem::Consumable(_) => {
            has_free_slot(consumables.len(), session.config.consumable_capacity)
        }
        // Un parchemin monte une figure : aucun inventaire, aucune capacité.
        ShopItem::GridUpgrade(_) => true,
        ShopItem::DieMod(_) => false,
    }
}

/// Range l'article acheté. Rend `false` si la destination a refusé, auquel cas
/// l'appelant ne débite rien.
fn ranger(
    item: &ShopItem,
    session: &mut RunSession,
    relics: &mut RelicInventory,
    consumables: &mut ConsumableInventory,
) -> bool {
    match item {
        ShopItem::RelicCard(def) => relics.add_relic(*def).is_some(),
        ShopItem::Consumable(id) => consumables.add(*id).is_some(),
        ShopItem::GridUpgrade(figure) => {
            session.hand_levels.upgrade(*figure);
            true
        }
        ShopItem::DieMod(_) => false,
    }
}

/// L'achat. **L'ordre des vérifications est normatif.**
fn handle_purchases(
    mut messages: MessageReader<PurchaseEvent>,
    mut session: ResMut<RunSession>,
    mut shop: ResMut<ShopInventory>,
    mut relics: ResMut<RelicInventory>,
    mut consumables: ResMut<ConsumableInventory>,
) {
    for rang in rangs_a_traiter(&mut messages) {
        // 1. Rang hors bornes : refus silencieux, jamais une panique.
        let Some(item) = shop.items.get(rang).cloned() else {
            continue;
        };

        // 2. Le prix se lit, il ne se recalcule pas.
        let prix = price_of(&item);

        // 3. L'or, avant tout. Comparaison **stricte** : payer son dernier
        //    dollar est un achat valide.
        if session.gold < prix {
            continue;
        }

        // 4. La place, ensuite.
        if !place_disponible(&item, &session, &relics, &consumables) {
            continue;
        }

        // 5. Alors seulement. Le rangement peut encore refuser, et le débit ne
        //    part qu'une fois la destination acquise.
        if !ranger(&item, &mut session, &mut relics, &mut consumables) {
            continue;
        }
        session.gold = session.gold.saturating_sub(prix);
        shop.items.remove(rang);
    }
}

/// La revente. **Tarifer avant de libérer** : le slot vidé emporte la
/// définition qui sert à en calculer le prix.
fn handle_sales(
    mut messages: MessageReader<SellEvent>,
    mut session: ResMut<RunSession>,
    mut relics: ResMut<RelicInventory>,
) {
    for vente in messages.read() {
        let Some(instance) = relics
            .slots
            .get(usize::from(vente.slot))
            .and_then(|slot| slot.as_ref())
        else {
            continue;
        };

        let reprise = sell_value(price_of(&ShopItem::RelicCard(instance.def)));
        session.gold = session.gold.saturating_add(reprise);
        relics.remove_relic(vente.slot);
    }
}

/// La relance. **Débiter, régénérer, incrémenter, dans cet ordre.**
///
/// Régénérer avant de débiter offrirait un étalage neuf au joueur sans le sou ;
/// incrémenter avant de débiter surfacturerait d'un dollar.
///
/// **Seuls les articles sont remplacés.** Le générateur rend un étalage entier,
/// coût de relance compris et remis à sa valeur initiale : lui affecter la
/// ressource replacerait ce coût, et les relances successives coûteraient
/// toutes le même prix. Ce piège n'est atteignable que d'ici.
fn handle_rerolls(
    mut messages: MessageReader<RerollEvent>,
    mut session: ResMut<RunSession>,
    mut shop: ResMut<ShopInventory>,
) {
    for _ in messages.read() {
        if session.gold < shop.reroll_cost {
            continue;
        }
        session.gold = session.gold.saturating_sub(shop.reroll_cost);
        shop.items = generate_shop(&mut session.rng.shop).items;
        shop.reroll_cost = bump_reroll_cost(shop.reroll_cost);
    }
}

/// Branche les trois systèmes.
///
/// Ils habitent `ShopSet::Interact` : ce sont des **clics**, et l'ensemble
/// suivant resynchronise l'affichage sur ce qu'ils ont produit. Chaînés entre
/// eux, une relance et un achat de la même frame ne voient jamais deux
/// étalages différents.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        (handle_purchases, handle_sales, handle_rerolls)
            .chain()
            .in_set(ShopSet::Interact),
    );
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::state::app::StatesPlugin;
    use core_engine::config::RunConfig;
    use core_engine::consumables::{ConsumableId, ConsumableInventory};
    use core_engine::cups::CupId;
    use core_engine::cups::definitions::cup;
    use core_engine::hands::{HandLevels, YahtzeeHand};
    use core_engine::relics::{RelicId, RelicInventory};
    use core_engine::rng::RunRng;
    use core_engine::shop::pricing::price_of;
    use core_engine::shop::{ShopInventory, ShopItem};
    use game_state::resources::RunSession;
    use game_state::states::{AppState, RunPhase};

    use crate::systems::has_free_slot;
    use crate::{PurchaseEvent, RerollEvent, SellEvent, ShopPlugin};

    /// Une boutique ouverte, sur un gobelet donné, avec l'or voulu.
    fn app_en_boutique(id: CupId, or: u32, articles: Vec<ShopItem>) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, ShopPlugin));
        app.init_state::<AppState>().add_sub_state::<RunPhase>();

        let deck = cup(id);
        let config = RunConfig::from_cup(&deck);
        app.insert_resource(RelicInventory::new(config.relic_capacity));
        app.insert_resource(ConsumableInventory::new(config.consumable_capacity));
        app.insert_resource(RunSession {
            config,
            ante: 1,
            gold: or,
            cup_id: id,
            stake_level: 0,
            hand_levels: HandLevels::default(),
            rng: RunRng::from_seed(1),
        });
        app.insert_resource(ShopInventory {
            items: articles,
            reroll_cost: core_engine::shop::INITIAL_REROLL_COST,
        });

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InRun);
        app.update();
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::Shop);
        app.update();
        app
    }

    fn or(app: &App) -> u32 {
        app.world().resource::<RunSession>().gold
    }
    fn etalage(app: &App) -> Vec<ShopItem> {
        app.world().resource::<ShopInventory>().items.clone()
    }
    fn reliques(app: &App) -> usize {
        app.world().resource::<RelicInventory>().len()
    }

    /// Une relique Commune, donc à quatre dollars.
    fn commune() -> ShopItem {
        ShopItem::RelicCard(RelicId::CrackedDie)
    }
    /// Une relique Rare, donc à huit.
    fn rare() -> ShopItem {
        ShopItem::RelicCard(RelicId::DoubleMirror)
    }

    #[test]
    fn test_purchase_refused_when_insufficient_gold() {
        let mut app = app_en_boutique(CupId::Standard, 4, vec![rare()]);
        assert_eq!(price_of(&rare()), 6 + 2, "l'article vaut bien huit");

        let avant = etalage(&app);
        app.world_mut()
            .write_message(PurchaseEvent { item_index: 0 });
        app.update();

        assert_eq!(or(&app), 4, "l'or a bougé sur un refus");
        assert_eq!(etalage(&app), avant, "l'étalage a bougé sur un refus");
        assert_eq!(reliques(&app), 0, "une relique est entrée sur un refus");
    }

    #[test]
    fn test_relic_capacity_from_config() {
        // **Deux gobelets, pas une mutation.** `RelicInventory::new` dimensionne
        // ses slots une seule fois, et rien ne les redimensionne : changer la
        // configuration en cours de run laisserait le tableau à sa taille.
        let remplir_puis_acheter = |id: CupId| {
            let mut app = app_en_boutique(id, 100, vec![commune()]);
            let capacite = app.world().resource::<RunSession>().config.relic_capacity;
            {
                let mut stock = app.world_mut().resource_mut::<RelicInventory>();
                for _ in 0..5 {
                    stock.add_relic(RelicId::PolishedStone);
                }
            }
            assert_eq!(reliques(&app), 5, "cinq reliques équipées ({id:?})");

            app.world_mut()
                .write_message(PurchaseEvent { item_index: 0 });
            app.update();
            (capacite, reliques(&app))
        };

        assert_eq!(
            remplir_puis_acheter(CupId::Standard),
            (5, 5),
            "plein à cinq"
        );
        assert_eq!(
            remplir_puis_acheter(CupId::Fortune),
            (6, 6),
            "le sixième slot"
        );
    }

    #[test]
    fn test_purchase_debits_exact_price_and_removes_item() {
        let mut app = app_en_boutique(CupId::Standard, 10, vec![commune(), rare()]);
        app.world_mut()
            .write_message(PurchaseEvent { item_index: 1 });
        app.update();

        assert_eq!(or(&app), 2, "dix moins huit, exactement le prix");
        assert_eq!(
            etalage(&app),
            vec![commune()],
            "l'article n'a pas quitté l'étalage"
        );
        assert_eq!(reliques(&app), 1);
    }

    #[test]
    fn test_purchase_at_last_dollar_succeeds() {
        // La comparaison est stricte : `or == prix` passe.
        let mut app = app_en_boutique(CupId::Standard, 8, vec![rare()]);
        app.world_mut()
            .write_message(PurchaseEvent { item_index: 0 });
        app.update();

        assert_eq!(or(&app), 0);
        assert_eq!(reliques(&app), 1);
    }

    #[test]
    fn test_sell_credits_half_and_frees_slot() {
        let mut app = app_en_boutique(CupId::Standard, 0, vec![]);
        {
            let mut stock = app.world_mut().resource_mut::<RelicInventory>();
            stock.add_relic(RelicId::DoubleMirror); // Rare, huit
            stock.add_relic(RelicId::CrackedDie); // Commune, quatre
        }

        app.world_mut().write_message(SellEvent { slot: 0 });
        app.update();
        assert_eq!(or(&app), 4, "huit revendus quatre");
        assert_eq!(reliques(&app), 1);
        assert!(
            app.world().resource::<RelicInventory>().slots[0].is_none(),
            "le slot n'est pas libéré"
        );

        app.world_mut().write_message(SellEvent { slot: 1 });
        app.update();
        assert_eq!(or(&app), 6, "quatre revendus deux");
        assert_eq!(reliques(&app), 0);
    }

    #[test]
    fn test_reroll_debits_then_regenerates() {
        // Or insuffisant : rien ne bouge.
        let mut pauvre = app_en_boutique(CupId::Standard, 4, vec![commune()]);
        let avant = etalage(&pauvre);
        pauvre.world_mut().write_message(RerollEvent);
        pauvre.update();
        assert_eq!(or(&pauvre), 4);
        assert_eq!(etalage(&pauvre), avant, "étalage régénéré sans payer");

        // Or suffisant.
        let mut riche = app_en_boutique(CupId::Standard, 20, vec![commune()]);
        riche.world_mut().write_message(RerollEvent);
        riche.update();
        assert_eq!(or(&riche), 15, "vingt moins cinq");
        assert_eq!(etalage(&riche).len(), 4, "un étalage complet");
        assert_eq!(riche.world().resource::<ShopInventory>().reroll_cost, 6);
    }

    #[test]
    fn test_three_rerolls_cost_five_six_seven() {
        // **La dette laissée par TASK-77.** Le piège n'est atteignable que d'ici :
        // remplacer la ressource entière par ce que rend le générateur replace
        // le coût initial, et les trois relances coûteraient cinq, cinq, cinq.
        // Le test du ticket ne les distingue pas au premier passage.
        let mut app = app_en_boutique(CupId::Standard, 100, vec![commune()]);
        let mut payes = Vec::new();

        for _ in 0..3 {
            let avant = or(&app);
            payes.push(app.world().resource::<ShopInventory>().reroll_cost);
            app.world_mut().write_message(RerollEvent);
            app.update();
            assert_eq!(avant - or(&app), *payes.last().expect("payé"));
        }

        assert_eq!(payes, vec![5, 6, 7]);
        assert_eq!(app.world().resource::<ShopInventory>().reroll_cost, 8);
        assert_eq!(or(&app), 100 - 5 - 6 - 7);
    }

    #[test]
    fn test_double_purchase_event_buys_once() {
        // **Le rang visé n'est pas le dernier, et c'est le point.** Sur le
        // dernier article, le retrait rend le second rang hors bornes et le
        // second achat échoue de lui-même : la déduplication ne servirait à
        // rien, et un doublon non écarté passerait le test. Mesuré.
        let mut app = app_en_boutique(CupId::Standard, 100, vec![commune(), rare(), commune()]);
        app.world_mut()
            .write_message(PurchaseEvent { item_index: 0 });
        app.world_mut()
            .write_message(PurchaseEvent { item_index: 0 });
        app.update();

        assert_eq!(or(&app), 96, "un seul débit");
        assert_eq!(reliques(&app), 1, "une seule relique");
        assert_eq!(
            etalage(&app),
            vec![rare(), commune()],
            "un seul retrait, et c'est le bon"
        );
    }

    #[test]
    fn test_two_purchases_in_one_frame_keep_their_indices() {
        // **Un retrait décale les rangs qui suivent.** Traités dans l'ordre
        // croissant, l'achat du rang zéro ferait glisser le rang un sur
        // l'article suivant : le joueur paierait l'un et recevrait l'autre.
        // L'ordre décroissant garde chaque rang en attente valide.
        let mut app = app_en_boutique(
            CupId::Standard,
            100,
            vec![commune(), rare(), ShopItem::RelicCard(RelicId::Pendulum)],
        );
        app.world_mut()
            .write_message(PurchaseEvent { item_index: 0 });
        app.world_mut()
            .write_message(PurchaseEvent { item_index: 1 });
        app.update();

        assert_eq!(or(&app), 100 - 4 - 8, "les deux prix, et eux seuls");
        assert_eq!(reliques(&app), 2);
        assert_eq!(
            etalage(&app),
            vec![ShopItem::RelicCard(RelicId::Pendulum)],
            "un autre article que les deux visés a été acheté"
        );

        let possedees: Vec<RelicId> = app
            .world()
            .resource::<RelicInventory>()
            .slots
            .iter()
            .flatten()
            .map(|instance| instance.def)
            .collect();
        assert!(possedees.contains(&RelicId::CrackedDie), "le rang zéro");
        assert!(possedees.contains(&RelicId::DoubleMirror), "le rang un");
    }

    #[test]
    fn test_consumable_capacity_from_config() {
        let article = ShopItem::Consumable(ConsumableId::RuneOfFate);
        let mut app = app_en_boutique(CupId::Standard, 100, vec![article.clone()]);

        let capacite = app
            .world()
            .resource::<RunSession>()
            .config
            .consumable_capacity;
        {
            let mut stock = app.world_mut().resource_mut::<ConsumableInventory>();
            for _ in 0..capacite {
                stock.add(ConsumableId::RuneOfFate);
            }
        }
        let plein = app.world().resource::<ConsumableInventory>().len();
        assert_eq!(plein, usize::from(capacite), "inventaire plein");

        let avant = or(&app);
        app.world_mut()
            .write_message(PurchaseEvent { item_index: 0 });
        app.update();

        assert_eq!(or(&app), avant, "acheté malgré l'inventaire plein");
        assert_eq!(app.world().resource::<ConsumableInventory>().len(), plein);
    }

    #[test]
    fn test_grid_upgrade_raises_the_hand_level() {
        let article = ShopItem::GridUpgrade(YahtzeeHand::Chance);
        let mut app = app_en_boutique(CupId::Standard, 100, vec![article]);
        let avant = app
            .world()
            .resource::<RunSession>()
            .hand_levels
            .base_for(YahtzeeHand::Chance);

        app.world_mut()
            .write_message(PurchaseEvent { item_index: 0 });
        app.update();

        let apres = app
            .world()
            .resource::<RunSession>()
            .hand_levels
            .base_for(YahtzeeHand::Chance);
        assert_ne!(apres, avant, "le parchemin n'a pas monté la figure");
        assert_eq!(or(&app), 96, "cent moins quatre");
        assert!(etalage(&app).is_empty());
    }

    #[test]
    fn test_no_transaction_outside_shop() {
        let mut app = app_en_boutique(CupId::Standard, 100, vec![commune()]);
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::Roll);
        app.update();

        let avant = (or(&app), etalage(&app), reliques(&app));
        app.world_mut()
            .write_message(PurchaseEvent { item_index: 0 });
        app.world_mut().write_message(SellEvent { slot: 0 });
        app.world_mut().write_message(RerollEvent);
        app.update();

        assert_eq!((or(&app), etalage(&app), reliques(&app)), avant);
    }

    #[test]
    fn test_free_slot_rule_is_written_once() {
        // La règle dit `occupied < capacity`, jamais un littéral.
        assert!(has_free_slot(4, 5));
        assert!(!has_free_slot(5, 5));
        assert!(has_free_slot(5, 6), "le Gobelet de Fortune");
        assert!(!has_free_slot(0, 0));
    }
}
