//! Interface de la boutique. **Aucune logique d'achat ici.**
//!
//! # Le sens des dépendances
//!
//! `core_engine ← game_state ← ui_and_juice ← shop_system`. Cette crate connaît
//! les trois autres ; **aucune ne la connaît**. Une seule ligne de dépendance
//! inverse, même « juste pour un type », ferme un cycle et le workspace cesse
//! de compiler. Quand `ShopInventory` semble mal placée, elle ne l'est pas :
//! elle reste dans le moteur.
//!
//! # Ce que ce plugin fait, et ce qu'il ne fait pas
//!
//! Il pose la ressource d'étalage, déclare les trois messages, et ordonne deux
//! ensembles **vides**. L'achat, la revente, la relance et le bouton de sortie
//! les peupleront ; ce fichier n'enregistre aucun système.
//!
//! # Un message, pas un événement
//!
//! Sur la ligne 0.17+, un événement tamponné se déclare `#[derive(Message)]`,
//! s'écrit par `MessageWriter`, se lit par `MessageReader` et s'enregistre par
//! `App::add_message` (bevy_ecs 0.19.1, `src/message/mod.rs` ; bevy_app 0.19.1,
//! `App::add_message`). Les trois noms ci-dessous gardent le suffixe `Event`
//! que le corpus déclare normatif, mais ce sont des **messages** au sens 0.19.

pub mod ui;

use bevy::prelude::*;
use core_engine::shop::ShopInventory;
use game_state::states::RunPhase;

/// Achat d'un article de l'étalage, désigné par son rang.
///
/// Le rang est un `usize` parce que c'est l'index de `ShopInventory.items` :
/// aucune conversion au seul endroit qui compte.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurchaseEvent {
    pub item_index: usize,
}

/// Revente de la relique occupant un slot de l'inventaire du joueur.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SellEvent {
    pub slot: u8,
}

/// Relance de l'étalage, aux frais du joueur.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RerollEvent;

/// Les deux temps d'une frame de boutique.
///
/// Les clics sont lus, **puis** l'affichage se resynchronise sur l'étalage.
/// L'ordre inverse afficherait l'état d'avant le clic pendant une frame.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShopSet {
    Interact,
    Refresh,
}

/// Monte la boutique.
///
/// **Il ne monte pas `JuicePlugin`, et le solde en dépend.** `AnimatedNumber`
/// est une donnée ; le système qui la fait converger vit dans la crate de mise
/// en scène. Une application qui monterait ce plugin sans elle afficherait un
/// solde figé. Le binaire final monte les deux.
pub struct ShopPlugin;

impl Plugin for ShopPlugin {
    fn build(&self, app: &mut App) {
        // Le `Default` manuel du moteur pose l'étalage vide et le coût de
        // relance initial, jamais zéro.
        app.init_resource::<ShopInventory>();

        app.add_message::<PurchaseEvent>();
        app.add_message::<SellEvent>();
        app.add_message::<RerollEvent>();

        // Deux ensembles vides, ordonnés et gardés. `RunPhase` est un
        // `SubStates` : hors de la run, l'état est absent du monde. Mesuré, un
        // ensemble vide ainsi gardé n'évalue jamais sa condition et ne panique
        // pas ; dès que des systèmes l'habiteront, les montages devront entrer
        // dans la run.
        app.configure_sets(
            Update,
            (ShopSet::Interact, ShopSet::Refresh)
                .chain()
                .run_if(in_state(RunPhase::Shop)),
        );
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;
    use bevy::ecs::schedule::Schedules;
    use bevy::prelude::*;
    use bevy::state::app::StatesPlugin;
    use core_engine::shop::{INITIAL_REROLL_COST, ShopInventory};

    use crate::ui::{ShopCardUI, spawn_shop_ui};
    use crate::{PurchaseEvent, RerollEvent, SellEvent, ShopPlugin, ShopSet};

    fn app_boutique() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, ShopPlugin));
        app
    }

    #[test]
    fn test_shop_plugin_boots_headless() {
        let mut app = app_boutique();
        // Trois frames : les sets sont gardés par un état absent du monde hors
        // `InRun`. Mesuré, un set vide ainsi gardé n'évalue jamais sa condition.
        app.update();
        app.update();
        app.update();
    }

    #[test]
    fn test_shop_sets_are_ordered() {
        let mut app = app_boutique();
        app.update();

        let schedules = app.world().resource::<Schedules>();
        let update = schedules.get(Update).expect("le calendrier Update");
        let graphe = update.graph();

        let aretes: Vec<(String, String)> = graphe
            .dependency()
            .graph()
            .all_edges()
            .map(|arete| {
                (
                    graphe.get_node_name(&arete.0),
                    graphe.get_node_name(&arete.1),
                )
            })
            .collect();

        // Les noms attendus viennent de l'énumération, jamais de chaînes
        // littérales : un renommage sans mise à jour du test passerait.
        let interact = format!("{:?}", ShopSet::Interact);
        let refresh = format!("{:?}", ShopSet::Refresh);

        assert!(
            aretes
                .iter()
                .any(|(de, vers)| de.contains(&interact) && vers.contains(&refresh)),
            "les clics doivent précéder la resynchronisation : {aretes:?}"
        );
        assert!(
            !aretes
                .iter()
                .any(|(de, vers)| de.contains(&refresh) && vers.contains(&interact)),
            "l'ordre est inversé"
        );
    }

    #[test]
    fn test_shop_inventory_is_initialised() {
        let app = app_boutique();
        let etalage = app.world().resource::<ShopInventory>();

        assert!(etalage.items.is_empty());
        assert_eq!(etalage.reroll_cost, INITIAL_REROLL_COST);
    }

    #[test]
    fn test_card_entities_carry_only_an_index() {
        // **Le nom d'un composant ne prouve rien ici.** Hors feature `debug`,
        // `ComponentInfo::name` rend partout la même chaîne d'excuse : une
        // inspection nominale passerait sur une carte portant tout ce qu'elle
        // devrait refuser. Mesuré.
        //
        // La garantie de type est établie à la **compilation**, par la paire de
        // doc-tests de `ui.rs` : aucun type de jeu n'est un `Component`, donc
        // aucun ne peut être attaché. Ce qui reste à garder ici, c'est qu'aucun
        // **enveloppeur** ne soit ajouté à la carte : on compte.
        let mut app = app_boutique();
        let etalage = app.world().resource::<ShopInventory>().clone();
        let racine = {
            let mut commandes = app.world_mut().commands();
            spawn_shop_ui(&mut commandes, &etalage)
        };
        app.update();
        assert!(app.world().get_entity(racine).is_ok());

        let mut requete = app.world_mut().query::<(Entity, &ShopCardUI)>();
        let mut cartes: Vec<(Entity, usize)> = requete
            .iter(app.world())
            .map(|(entite, carte)| (entite, carte.0))
            .collect();
        cartes.sort_by_key(|(_, rang)| *rang);

        assert_eq!(cartes.len(), 4, "quatre cartes");
        assert_eq!(
            cartes.iter().map(|(_, rang)| *rang).collect::<Vec<_>>(),
            vec![0, 1, 2, 3],
            "les rangs sont ceux de l'étalage"
        );

        // Le nombre de composants de la carte, épinglé. Tout enveloppeur ajouté
        // le fait bouger, quel que soit son nom.
        for (entite, rang) in &cartes {
            let combien = app.world().inspect_entity(*entite).expect("carte").count();
            assert_eq!(combien, COMPOSANTS_PAR_CARTE, "carte {rang}");
        }
    }

    /// Ce qu'une carte porte : son rang, le bouton et son habillage `Node`,
    /// plus ce que `Button` et `Node` exigent, plus le lien vers ses enfants.
    const COMPOSANTS_PAR_CARTE: usize = 21;

    #[test]
    fn test_messages_are_registered() {
        // **Rien n'émet ni ne lit ces messages à ce stade**, donc aucun test de
        // comportement ne peut voir leur enregistrement : mesuré, le retirer
        // survit à toute la suite. Ce qui s'observe, c'est le tampon que
        // l'enregistrement pose dans le monde.
        let app = app_boutique();
        let monde = app.world();

        assert!(monde.get_resource::<Messages<PurchaseEvent>>().is_some());
        assert!(monde.get_resource::<Messages<SellEvent>>().is_some());
        assert!(monde.get_resource::<Messages<RerollEvent>>().is_some());
    }

    #[test]
    fn test_multiple_cards_coexist() {
        // C'est **le** test qui attraperait une dérivation `Resource` posée par
        // erreur sur la carte : elle compilerait, et n'en laisserait qu'une.
        let mut app = app_boutique();
        let cartes: Vec<Entity> = (0..4)
            .map(|index| app.world_mut().spawn(ShopCardUI(index)).id())
            .collect();

        for entite in &cartes {
            assert!(
                app.world().get_entity(*entite).is_ok(),
                "une carte a disparu"
            );
        }
        assert_eq!(cartes.len(), 4);
    }
}
