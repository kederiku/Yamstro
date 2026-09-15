//! État de run propre au harnais.
//!
//! **Deux des trois types annoncés par le document source ne sont pas dans le
//! moteur.** Il place l'état de run et le contexte de main parmi les types que
//! le harnais instancierait directement ; ils vivent en réalité dans la crate
//! d'états, qui dépend du moteur graphique et que le harnais ne peut pas
//! importer. Le harnais porte donc les siens, aux **mêmes noms de champs** —
//! c'est ce qui rendra lisible la comparaison du fixture d'accord de TASK-155 —
//! et sous des **noms de types différents** : deux structures homonymes dans un
//! même dépôt, l'une lue par le jeu et l'autre par l'instrument, seraient la
//! duplication silencieuse que les audits reprochent.
//!
//! Seul le contexte de blind vit bien dans le moteur, et se construit par
//! littéral, ses cinq champs étant publics.
//!
//! Si la compilation d'un `use` échoue parce qu'un type manque ici, le recours
//! est une entrée dans `MISSING_API.md` — jamais une ligne dans le moteur.

use core_engine::config::RunConfig;
use core_engine::cups::CupId;
use core_engine::cups::definitions::cup;
use core_engine::evaluator::HandMatch;
use core_engine::hands::{HandLevels, YahtzeeHand};
use core_engine::relics::RelicInventory;
use core_engine::rng::RunRng;

#[derive(Debug, Clone)]
pub struct SimSession {
    pub config: RunConfig,
    pub ante: u8,
    pub gold: u32,
    pub cup_id: CupId,
    pub stake_level: u8,
    pub hand_levels: HandLevels,
    pub relics: RelicInventory,
    pub rng: RunRng,
}

#[derive(Debug, Clone)]
pub struct SimHand {
    pub rerolls_left: u8,
    pub active_evaluations: Vec<HandMatch>,
    pub selected_hand: Option<YahtzeeHand>,
}

impl SimSession {
    /// Ouvre une run. **Seul point de construction**, et c'est la raison d'être
    /// de cette fonction : sans elle, la boucle écrirait les huit champs et la
    /// règle « l'or de départ est écrit une fois » n'aurait aucun site où
    /// tenir.
    ///
    /// Le `CupDeck` est lu puis **laissé mourir** : la configuration en est
    /// dérivée, l'or de départ y est pris une fois, et le garder serait une
    /// seconde source des six champs que la configuration porte déjà. Les faces
    /// des dés se relisent par `cup(session.cup_id)` à chaque manche — fonction
    /// pure, et la main de dés est de portée manche, pas de portée run.
    ///
    /// Aucun `Default` : une run à l'ante zéro, sans gobelet et sans cible,
    /// compilerait et produirait des chiffres faux dans le rapport.
    #[must_use]
    pub fn new(cup_id: CupId, stake_level: u8, seed: u64) -> Self {
        let deck = cup(cup_id);
        let config = RunConfig::from_cup(&deck);
        Self {
            config,
            ante: 1,
            // **Seule source de vérité de l'or.** Écrit ici, jamais relu du
            // gobelet ensuite, et aucun solde parallèle ailleurs.
            gold: deck.starting_gold,
            cup_id,
            stake_level,
            hand_levels: HandLevels::default(),
            // Seule fabrique, et la capacité vient de la configuration : le
            // Gobelet de Fortune la porte à six, et aucun littéral ne le sait.
            relics: RelicInventory::new(config.relic_capacity),
            rng: RunRng::from_seed(seed),
        }
    }
}

impl SimHand {
    /// Ouvre une main. Les évaluations et la figure choisie naissent vides :
    /// la boucle les remplit après le lancer.
    #[must_use]
    pub fn new(rerolls_left: u8) -> Self {
        Self {
            rerolls_left,
            active_evaluations: Vec::new(),
            selected_hand: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::cups::definitions::cup;

    #[test]
    fn test_sim_session_needs_no_bevy() {
        let deck = cup(CupId::Standard);
        let session = SimSession::new(CupId::Standard, 1, 1);

        assert_eq!(session.config, RunConfig::from_cup(&deck));
        assert_eq!(session.ante, 1, "une run démarre à l'ante un");
        assert_eq!(
            session.gold, deck.starting_gold,
            "l'or de départ vient du gobelet"
        );
        assert_eq!(session.cup_id, CupId::Standard);
        assert_eq!(session.stake_level, 1);
        assert_eq!(
            session.relics.capacity(),
            usize::from(deck.relic_capacity),
            "la capacité vient de la configuration, jamais d'un littéral"
        );
        assert!(session.relics.is_empty());

        // Le Gobelet de Fortune porte la capacité à six : aucun littéral cinq
        // n'a pu se glisser dans la construction.
        let fortune = SimSession::new(CupId::Fortune, 1, 1);
        assert_eq!(fortune.relics.capacity(), 6);
        assert_ne!(fortune.relics.capacity(), session.relics.capacity());

        let main = SimHand::new(session.config.base_rerolls);
        assert_eq!(main.rerolls_left, session.config.base_rerolls);
        assert!(main.active_evaluations.is_empty());
        assert_eq!(main.selected_hand, None);
    }

    #[test]
    fn test_hand_levels_built_only_by_upgrade() {
        let mut session = SimSession::new(CupId::Standard, 1, 1);
        for _ in 0..3 {
            session.hand_levels.upgrade(YahtzeeHand::Yahtzee);
        }

        assert_eq!(session.hand_levels.level(YahtzeeHand::Yahtzee), 4);
        for hand in YahtzeeHand::ALL {
            if hand != YahtzeeHand::Yahtzee {
                assert_eq!(session.hand_levels.level(hand), 1, "{hand:?}");
            }
        }
    }

    #[test]
    fn test_gold_is_written_once_from_the_cup() {
        // **Une seule source de vérité de l'or.** Le gobelet le pose à la
        // construction et n'est plus relu : deux gobelets de dotations
        // différentes donnent deux soldes différents, et rien d'autre ne les
        // écrit.
        let mut session = SimSession::new(CupId::Abandoned, 1, 1);
        assert_eq!(session.gold, cup(CupId::Abandoned).starting_gold);

        session.gold = session.gold.saturating_add(7);
        assert_eq!(
            session.gold,
            cup(CupId::Abandoned).starting_gold + 7,
            "le solde est porté par la session, pas relu du gobelet"
        );
    }
}
