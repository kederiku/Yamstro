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
use core_engine::scoring::ScoreStep;

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

/// Vitesse de relecture refusée.
///
/// **Première erreur du dépôt**, et donc la convention : un type nu, `Display`
/// et `std::error::Error` écrits à la main, aucune dépendance ajoutée pour un
/// seul type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSpeed(pub u32);

impl std::fmt::Display for InvalidSpeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "vitesse de relecture invalide : {} n'est ni 1, ni 2, ni 4",
            self.0
        )
    }
}

impl std::error::Error for InvalidSpeed {}

/// Intervalle entre deux paliers, avant application de la vitesse.
const STEP_INTERVAL_SECS: f32 = 0.25;
/// Pause après le dernier palier. **Jamais accélérée.**
const FINAL_PAUSE_SECS: f32 = 0.5;

/// File d'animation du score, alimentée par le rapport du pipeline.
///
/// **Forme définitive.** Huit champs, aucun de plus : ni curseur d'index, ni
/// drapeau d'application, ni identifiant de blind. Le dépilement se fait par
/// `pop_front`, et un champ d'attribution inviterait au double comptage que la
/// séparation calcul/commit supprime (ADR-010).
///
/// # Ce que `report` portait, et pourquoi il a disparu
///
/// TASK-31 stockait le `ScoringReport` entier. L'Étape 4 n'en tirait que deux
/// valeurs, qui sont désormais des champs. Les deux sont **dérivables du
/// rapport** — `final_score` est même, de l'aveu de `core_engine`, « redondante
/// par construction avec le dernier pas » — et c'est sans importance : **au
/// moment du commit, la file est vide**. Tout ce qui se déduirait des `steps` a
/// disparu quand vient le seul instant où on en a besoin. C'est la raison
/// d'être des deux champs, et la seule.
///
/// `committed` est le garde-fou du commit unique. Il naît **fermé** dans
/// `Default` : une file par défaut n'a rien à commettre, et sa figure est une
/// sentinelle qui ne doit jamais être lue.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct ScoringStepQueue {
    pub steps: VecDeque<ScoreStep>,
    /// Figure retenue, à marquer dans `used_hands`.
    pub hand: YahtzeeHand,
    /// Total calculé par `core_engine`, transporté tel quel, jamais recalculé.
    pub final_score: u64,
    /// `TimerMode::Repeating`, 0,25 s.
    pub step_timer: Timer,
    /// `TimerMode::Once`, 0,5 s, **jamais accélérée**.
    pub final_pause: Timer,
    /// 1 | 2 | 4. Écrit **uniquement** par `set_speed_multiplier`.
    pub speed_multiplier: u32,
    /// Espace ou clic gauche maintenu.
    pub fast_forward: bool,
    /// Garde-fou : le commit n'a lieu qu'une fois.
    pub committed: bool,
}

impl Default for ScoringStepQueue {
    /// **Écrit à la main, jamais dérivé.** Un `Default` dérivé donnerait deux
    /// minuteries de durée nulle ; or `Timer::tick` calcule alors
    /// `elapsed.checked_div(0).map_or(u32::MAX, …)`, si bien que
    /// `times_finished_this_tick()` vaut **`u32::MAX`** dès la première frame et
    /// que la boucle de dépilement ferait 4 294 967 295 tours.
    ///
    /// La figure est une **sentinelle** : `YahtzeeHand::ALL[0]` est `Aces`, une
    /// figure parfaitement réelle. Elle n'est jamais lue parce que `committed`
    /// naît à `true`. Quiconque lit `hand` sans avoir vérifié `committed`
    /// marquera `Aces` comme jouée.
    fn default() -> Self {
        Self {
            steps: VecDeque::new(),
            hand: YahtzeeHand::ALL[0],
            final_score: 0,
            step_timer: Timer::from_seconds(STEP_INTERVAL_SECS, TimerMode::Repeating),
            final_pause: Timer::from_seconds(FINAL_PAUSE_SECS, TimerMode::Once),
            speed_multiplier: 1,
            fast_forward: false,
            committed: true,
        }
    }
}

impl ScoringStepQueue {
    /// Seul chemin de remplissage. `speed_multiplier` vaut toujours 1, donc
    /// aucun `Result` ne remonte dans `build_scoring_report` ; le réglage
    /// courant, lui, est reposé par l'appelant (voir `build_scoring_report`).
    pub fn new(steps: VecDeque<ScoreStep>, hand: YahtzeeHand, final_score: u64) -> Self {
        Self {
            steps,
            hand,
            final_score,
            committed: false,
            ..Self::default()
        }
    }

    /// Seul chemin d'écriture de `speed_multiplier`.
    ///
    /// **Rejette, ne corrige pas.** Un `3` venu du réglage de l'Étape 11 est un
    /// bug de l'appelant ; un `clamp` le rendrait invisible et le joueur
    /// verrait une vitesse qu'il n'a pas demandée. En cas de rejet, la valeur
    /// courante n'est pas touchée.
    pub fn set_speed_multiplier(&mut self, value: u32) -> Result<(), InvalidSpeed> {
        if !matches!(value, 1 | 2 | 4) {
            return Err(InvalidSpeed(value));
        }
        self.speed_multiplier = value;
        Ok(())
    }

    /// Facteur de vitesse appliqué au dépilement.
    ///
    /// **Plafond à 8.** 0,25 s ÷ 8 vaut 31 ms, soit deux frames à 60 FPS. En
    /// dessous, le joueur ne voit plus quel dé ou quelle relique produit quel
    /// incrément : le séquencement visible n'existe plus.
    pub fn effective_speed(&self) -> f32 {
        (self.speed_multiplier as f32 * if self.fast_forward { 4.0 } else { 1.0 }).min(8.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn file() -> ScoringStepQueue {
        ScoringStepQueue::new(VecDeque::new(), YahtzeeHand::FullHouse, 1_234)
    }

    #[test]
    fn test_effective_speed_table() {
        let mut q = file();
        for (multiplicateur, sans, avec) in [(1, 1.0, 4.0), (2, 2.0, 8.0), (4, 4.0, 8.0)] {
            q.set_speed_multiplier(multiplicateur)
                .expect("vitesse admise");
            q.fast_forward = false;
            assert_eq!(
                q.effective_speed(),
                sans,
                "x{multiplicateur} sans avance rapide"
            );
            q.fast_forward = true;
            assert_eq!(
                q.effective_speed(),
                avec,
                "x{multiplicateur} avec avance rapide"
            );
        }
    }

    #[test]
    fn test_speed_multiplier_rejects_invalid() {
        let mut q = file();
        q.set_speed_multiplier(4).expect("vitesse admise");
        for invalide in [0, 3, 5, 8, u32::MAX] {
            assert_eq!(
                q.set_speed_multiplier(invalide),
                Err(InvalidSpeed(invalide))
            );
            assert_eq!(q.speed_multiplier, 4, "un rejet a tout de même écrit");
        }
    }

    #[test]
    fn test_committed_is_false_at_construction() {
        assert!(!file().committed);
        assert_eq!(file().hand, YahtzeeHand::FullHouse);
        assert_eq!(file().final_score, 1_234);
        assert_eq!(file().speed_multiplier, 1);
        assert!(!file().fast_forward);
    }

    #[test]
    fn test_default_is_a_closed_guard() {
        // Une file par défaut n'a **rien** à commettre : le garde-fou naît
        // fermé, et la figure sentinelle n'est donc jamais lue. Sans cela,
        // `hand` vaut `Aces`, une figure parfaitement réelle, et un commit
        // égaré la marquerait comme jouée.
        let q = ScoringStepQueue::default();
        assert!(q.committed);
        assert!(q.steps.is_empty());
        assert_eq!(q.final_score, 0);
        assert_eq!(q.speed_multiplier, 1);
    }

    #[test]
    fn test_timer_modes_and_durations() {
        let q = file();
        assert_eq!(q.step_timer.mode(), TimerMode::Repeating);
        assert_eq!(q.step_timer.duration(), Duration::from_millis(250));
        assert_eq!(q.final_pause.mode(), TimerMode::Once);
        assert_eq!(q.final_pause.duration(), Duration::from_millis(500));
        // Le `Default` porte les mêmes durées : un `derive(Default)` donnerait
        // deux minuteries de durée nulle, et `Timer::tick` rend alors
        // `times_finished_this_tick() == u32::MAX` (checked_div sur zéro), ce
        // qui ferait tourner la boucle de dépilement 4 294 967 295 fois.
        let d = ScoringStepQueue::default();
        assert_eq!(d.step_timer.duration(), q.step_timer.duration());
        assert_eq!(d.final_pause.duration(), q.final_pause.duration());
    }

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
        let inventaire = RelicInventory::new(session.config.relic_capacity);

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
        assert_eq!(
            *monde.resource::<ScoringStepQueue>(),
            ScoringStepQueue::default(),
            "la file insérée par le plugin n'est pas une file au repos"
        );
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
