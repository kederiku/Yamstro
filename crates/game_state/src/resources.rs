//! Ressources de partie. Les données de jeu vivent ici, jamais dans une entité.
//!
//! # Trois sources de vérité uniques
//!
//! `RunSession.gold` est le **seul** détenteur de l'or : l'or de départ du
//! gobelet y est écrit une fois et n'est plus relu ensuite. Les niveaux de
//! figures vivent dans `RunSession.hand_levels` et **ne sont jamais insérés**
//! comme ressource — dériver sans insérer est sans effet, insérer créerait deux
//! tables désynchronisées au premier achat en boutique. Et le contexte de
//! manche est celui de `core_engine`, importé, jamais redéclaré ici.
//!
//! # Ce qui n'est pas ici, et pourquoi
//!
//! Ni cible, ni score courant : ce sont des données de **manche**, portées par
//! le contexte de blind. Une donnée de manche qui survivrait à la manche
//! fausserait silencieusement l'arbitrage de fin de tour à la blind suivante.

use std::collections::VecDeque;

use bevy::prelude::*;
use core_engine::config::RunConfig;
use core_engine::cups::CupId;
use core_engine::evaluator::HandMatch;
use core_engine::hands::{HandLevels, YahtzeeHand};
use core_engine::rng::RunRng;
use core_engine::scoring::{ScoreStep, ScoringReport};

/// Ce qui dure toute une run.
///
/// Ne dérive **pas** `Default` : ces valeurs naissent d'un gobelet, jamais de
/// zéros implicites. Ne dérive pas non plus `Reflect` : `RunRng` porte quatre
/// générateurs qui ne l'implémentent pas. Si un registre en avait besoin un
/// jour, ce serait avec le champ ignoré, et jamais en tant que ressource
/// réfléchie.
#[derive(Resource, Debug, Clone)]
pub struct RunSession {
    pub config: RunConfig,
    pub ante: u8,
    pub gold: u32,
    pub cup_id: CupId,
    pub stake_level: u8,
    pub hand_levels: HandLevels,
    pub rng: RunRng,
}

/// Ce qui dure une main : relances restantes, figures détectées, figure
/// choisie. Ne dérive pas `Default` pour la même raison que la session.
#[derive(Resource, Debug, Clone)]
pub struct HandContext {
    pub rerolls_left: u8,
    pub active_evaluations: Vec<HandMatch>,
    pub selected_hand: Option<YahtzeeHand>,
}

/// File d'animation du score, alimentée par le rapport du pipeline.
///
/// **Forme minimale.** L'Étape 4 arrête la forme définitive, curseur et
/// minuteries de dépilement compris. Elle porte le rapport en plus des paliers
/// parce que le commit unique a besoin du total et de la figure retenue.
///
/// Aucun champ d'attribution ici — ni drapeau d'application, ni identifiant de
/// blind : il inviterait au double comptage que la séparation calcul/commit
/// supprime.
#[derive(Resource, Debug, Clone, Default)]
pub struct ScoringStepQueue {
    pub steps: VecDeque<ScoreStep>,
    pub report: Option<ScoringReport>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;
    use bevy::state::app::StatesPlugin;
    use core_engine::blind::{BlindContext, BlindDefinition};
    use core_engine::cups::CupId;
    use core_engine::cups::definitions::cup;
    use core_engine::hands::{HandGrid, HandLevels, YahtzeeHand};
    use core_engine::relics::RelicInventory;
    use core_engine::rng::RunRng;

    fn session(id: CupId) -> RunSession {
        let deck = cup(id);
        RunSession {
            config: RunConfig::from_cup(&deck),
            ante: 1,
            gold: deck.starting_gold,
            cup_id: id,
            stake_level: 0,
            hand_levels: HandLevels::default(),
            rng: RunRng::from_seed(1),
        }
    }

    /// Manche inerte. Le constructeur de test de `core_engine` est
    /// `#[cfg(test)]`, donc invisible depuis cette crate : le contexte se
    /// construit ici par littéral, ce que les `Default` publics de
    /// `BlindDefinition` et `HandGrid` rendent court.
    fn manche() -> BlindContext {
        BlindContext {
            blind: BlindDefinition::default(),
            target_score: 300,
            current_score: 0,
            hands_remaining: 4,
            used_hands: HandGrid::default(),
        }
    }

    fn app_avec_ressources(id: CupId) -> App {
        let session = session(id);
        let inventaire = RelicInventory {
            slots: vec![None; usize::from(session.config.relic_capacity)],
        };

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            InputPlugin,
            crate::GameStatePlugin,
        ));
        app.insert_resource(session);
        app.insert_resource(manche());
        app.insert_resource(HandContext {
            rerolls_left: 3,
            active_evaluations: Vec::new(),
            selected_hand: None,
        });
        app.insert_resource(inventaire);
        app.init_resource::<ScoringStepQueue>();
        app
    }

    #[test]
    fn test_five_resources_insert_and_read_back() {
        let mut app = app_avec_ressources(CupId::Standard);
        app.update();
        app.update();
        app.update();

        let monde = app.world();
        assert_eq!(monde.resource::<RunSession>().ante, 1);
        assert_eq!(monde.resource::<BlindContext>().hands_remaining, 4);
        assert_eq!(monde.resource::<HandContext>().rerolls_left, 3);
        assert_eq!(monde.resource::<RelicInventory>().slots.len(), 5);
        assert!(monde.resource::<ScoringStepQueue>().steps.is_empty());
        assert!(monde.resource::<ScoringStepQueue>().report.is_none());
    }

    #[test]
    fn test_relic_slots_match_cup_capacity() {
        // La capacité vient de la configuration, jamais d'un littéral ni d'un
        // `Default` : c'est le gobelet qui la fixe.
        for (id, attendu) in [
            (CupId::Standard, 5),
            (CupId::Fortune, 6),
            (CupId::Cheater, 5),
        ] {
            let app = app_avec_ressources(id);
            let monde = app.world();
            let capacite = monde.resource::<RunSession>().config.relic_capacity;
            assert_eq!(usize::from(capacite), attendu, "gobelet {id:?}");
            assert_eq!(
                monde.resource::<RelicInventory>().slots.len(),
                usize::from(capacite),
                "gobelet {id:?}"
            );
        }
    }

    #[test]
    fn test_used_hands_is_empty_at_construction() {
        let contexte = manche();

        assert!(contexte.used_hands.is_empty());
        for figure in YahtzeeHand::ALL {
            assert!(!contexte.used_hands.contains(figure), "figure {figure:?}");
        }
    }

    #[test]
    fn test_hand_levels_is_not_a_world_resource() {
        // Les niveaux vivent dans la session, et nulle part ailleurs. Ce test
        // tombe si quelqu'un insère les niveaux comme ressource du monde.
        let mut app = app_avec_ressources(CupId::Standard);
        app.update();

        assert!(app.world().get_resource::<HandLevels>().is_none());
        let niveaux = &app.world().resource::<RunSession>().hand_levels;
        assert_eq!(niveaux.level(YahtzeeHand::FullHouse), 1);
    }

    #[test]
    fn test_single_blind_context_type() {
        // Le type de la ressource **est** celui de `core_engine` : une fonction
        // qui n'accepte que celui-là reçoit la ressource sans conversion. Un
        // second type homonyme dans cette crate ferait échouer la compilation.
        fn exige_le_type_de_core(_: &core_engine::blind::BlindContext) {}

        let mut app = app_avec_ressources(CupId::Standard);
        app.update();
        exige_le_type_de_core(app.world().resource::<BlindContext>());
    }
}
