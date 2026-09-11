//! Avancement d'état des reliques et or de fin de blind.
//!
//! # Les deux sites, et pourquoi ceux-là
//!
//! **`OnExit(RunPhase::Scoring)` pour l'avancement par main.** Il se déclenche
//! exactement une fois par main, et nécessairement **après** le commit, puisque
//! c'est `ScoringStepQueue::committed` qui cause la transition
//! (`leave_scoring_when_queue_is_empty`). Une garde `in_state(Scoring)` en
//! `Update` tournerait à chaque frame de la phase et compterait les relances
//! restantes des dizaines de fois pour une seule main. Ce site rend aussi
//! inutile tout ordonnancement inter-crates : rien à déclarer côté
//! `ui_and_juice`, et `commit_score_when_drained` n'est ni lu ni touché.
//!
//! **`OnEnter(RunPhase::Shop)` pour l'or.** `RunPhase::RoundEnd` est entrée
//! après **chaque main** — `resolve_round_outcome` y arbitre entre main
//! suivante, boutique et défaite —, si bien qu'y encaisser rendrait le plafond
//! de la *Tirelire en Terre* inopérant : quatre mains à trois relances
//! donneraient douze au lieu de cinq. La seule transition qui signifie « cette
//! blind est finie » est celle qui mène à la boutique.
//!
//! Le vocabulaire diverge donc du code : `Hook::OnRoundEnd` est une **fin de
//! blind**, quand `RunPhase::RoundEnd` est une fin de main. Le nom du hook est
//! normatif et ne bouge pas ; celui des systèmes d'ici dit ce qu'ils font.
//!
//! # L'ordre, à l'intérieur du second système
//!
//! Encaisser **puis** remettre à zéro. L'inverse rend zéro or à chaque blind,
//! sans erreur ni échec de compilation.

pub mod drag_drop;
pub mod slots;

use bevy::prelude::*;
use core_engine::dice::Die;
use core_engine::economy::round_end_gold;
use core_engine::evaluator::HandMatch;
use core_engine::hands::HandLevels;
use core_engine::relics::effects::advance_state;
use core_engine::relics::{RelicInventory, RelicState};
use core_engine::scoring::{Hook, TriggerCtx};

use crate::resources::{HandContext, RunSession};
use crate::states::RunPhase;
use core_engine::blind::BlindContext;

/// Valeurs d'attente des champs que seule la passe de score renseigne.
///
/// Aucun bras de `gold_for` ni d'`advance_state` ne les lit : ces deux
/// fonctions répondent sur l'état de la relique et les relances restantes. Les
/// nommer plutôt que d'écrire des zéros nus dit que ce sont des trous connus,
/// et non des valeurs calculées.
///
/// `roll_index` prend `u8::MAX` pour la raison retenue par le pipeline : à
/// zéro, la garde de *Dé Fantôme* serait vraie hors de tout lancer.
const BASE_CHIPS_HORS_PIPELINE: u64 = 0;
const BASE_MULT_HORS_PIPELINE: i64 = 0;
const ROLL_INDEX_HORS_LANCER: u8 = u8::MAX;

/// La figure retenue de la main qui vient d'être jouée.
///
/// Retrouvée comme `build_scoring_report` la retrouve, et depuis les mêmes
/// sources : `HandContext` n'est réinitialisé que par `setup_round`, sur
/// `OnEnter(Roll)`, que ni la sortie de `Scoring` ni l'entrée en boutique ne
/// traversent.
fn figure_jouee(hand: &HandContext) -> Option<&HandMatch> {
    let cell = hand.selected_hand?;
    hand.active_evaluations
        .iter()
        .find(|evaluated| evaluated.hand == cell)
}

/// Prend les **niveaux**, non la session : le second système écrit
/// `session.gold` pendant que ce contexte vit, et un emprunt partagé de la
/// session entière le lui interdirait.
fn contexte_de_base<'a>(
    hand_levels: &'a HandLevels,
    blind: &'a BlindContext,
    hand: &'a HandMatch,
    dice: &'a [Die],
    rerolls_left: u8,
) -> TriggerCtx<'a> {
    TriggerCtx {
        hand,
        dice,
        hand_levels,
        blind,
        uid: 0,
        slot: 0,
        state: RelicState::None,
        die: None,
        base_chips: BASE_CHIPS_HORS_PIPELINE,
        base_mult: BASE_MULT_HORS_PIPELINE,
        left_effects: &[],
        roll_index: ROLL_INDEX_HORS_LANCER,
        rerolls_left,
    }
}

fn des_tries(dice: &Query<&Die>) -> Vec<Die> {
    let mut v: Vec<Die> = dice.iter().cloned().collect();
    v.sort_unstable_by_key(|de| de.id);
    v
}

/// Une main vient d'être scorée : chaque relique avance son état.
pub fn advance_relic_states_on_hand(
    session: Option<Res<RunSession>>,
    blind: Option<Res<BlindContext>>,
    hand: Option<Res<HandContext>>,
    inventory: Option<ResMut<RelicInventory>>,
    dice: Query<&Die>,
) {
    let (Some(session), Some(blind), Some(hand), Some(mut inventory)) =
        (session, blind, hand, inventory)
    else {
        return;
    };
    let Some(figure) = figure_jouee(&hand) else {
        return;
    };

    let des = des_tries(&dice);
    let niveaux = session.hand_levels.clone();
    let base = contexte_de_base(&niveaux, &blind, figure, &des, hand.rerolls_left);
    avancer(&mut inventory, &base, Hook::OnHandScored);
}

/// La blind est battue : l'or des reliques rentre, puis les compteurs repartent.
pub fn collect_relic_gold_on_blind_end(
    session: Option<ResMut<RunSession>>,
    blind: Option<Res<BlindContext>>,
    hand: Option<Res<HandContext>>,
    inventory: Option<ResMut<RelicInventory>>,
    dice: Query<&Die>,
) {
    let (Some(mut session), Some(blind), Some(hand), Some(mut inventory)) =
        (session, blind, hand, inventory)
    else {
        return;
    };
    // Ce repli est **inatteignable en jeu** : entrer en boutique exige que le
    // score ait atteint la cible, ce que seul le commit produit, et le commit
    // ne suit qu'une soumission — laquelle pose `selected_hand`.
    let Some(figure) = figure_jouee(&hand) else {
        return;
    };

    let des = des_tries(&dice);
    let niveaux = session.hand_levels.clone();
    let base = contexte_de_base(&niveaux, &blind, figure, &des, hand.rerolls_left);

    // Encaisser d'abord. L'ordre inverse rend zéro, en silence.
    let gagne = round_end_gold(&inventory, &base);
    session.gold = session.gold.saturating_add(gagne);

    avancer(&mut inventory, &base, Hook::OnRoundEnd);
}

/// `advance_state` est le seul producteur de la valeur écrite : ces deux
/// lignes ne calculent rien, elles rangent (ADR-010).
fn avancer(inventory: &mut RelicInventory, base: &TriggerCtx<'_>, hook: Hook) {
    for (slot, inst) in inventory.slots.iter_mut().enumerate() {
        let Some(inst) = inst else { continue };
        if !inst.participe() {
            continue;
        }
        let ctx = TriggerCtx {
            uid: inst.uid,
            slot: slot as u8,
            state: inst.state,
            ..*base
        };
        inst.state = advance_state(inst.def, hook, &ctx, inst.state);
    }
}

pub(crate) fn register(app: &mut App) {
    slots::register(app);
    drag_drop::register(app);
    app.add_systems(OnExit(RunPhase::Scoring), advance_relic_states_on_hand);
    app.add_systems(OnEnter(RunPhase::Shop), collect_relic_gold_on_blind_end);
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::cups::CupId;
    use core_engine::relics::RelicId;

    use crate::resources::ScoringStepQueue;
    use crate::systems::fixtures::{app_en_run, entrer_dans_roll, frapper};
    use crate::systems::input::select_hand;

    fn deux_frames(app: &mut App) {
        app.update();
        app.update();
    }

    fn phase(app: &App) -> Option<RunPhase> {
        app.world().get_resource::<State<RunPhase>>().map(|s| **s)
    }

    fn tirelire(app: &mut App) -> RelicState {
        app.world()
            .resource::<RelicInventory>()
            .slots
            .iter()
            .flatten()
            .find(|inst| inst.def == RelicId::ClayPiggyBank)
            .expect("tirelire au stock")
            .state
    }

    fn or(app: &App) -> u32 {
        app.world().resource::<RunSession>().gold
    }

    fn poser_tirelire(app: &mut App) {
        app.world_mut()
            .resource_mut::<RelicInventory>()
            .add_relic(RelicId::ClayPiggyBank)
            .expect("slot libre");
    }

    /// Joue une main de bout en bout et s'arrête **dans `RoundEnd`**, une fois
    /// `OnExit(Scoring)` passé.
    ///
    /// La soumission emprunte le vrai chemin — `select_hand` puis la frappe —
    /// parce que `submit_hand` ne transite que sur `just_pressed` : poser
    /// `selected_hand` à la main laisse la phase dans `Roll`, la file jamais
    /// remplie, et le système d'ici jamais déclenché.
    ///
    /// `rerolls_left` se pose **après** l'entrée en lancer, `setup_round` le
    /// recalculant à ce moment-là, et il survit ensuite jusqu'à la main
    /// suivante : `HandContext` n'est réinitialisé que sur `OnEnter(Roll)`.
    fn jouer_une_main(app: &mut App, relances_restantes: u8) {
        entrer_dans_roll(app);
        deux_frames(app);
        app.world_mut().resource_mut::<HandContext>().rerolls_left = relances_restantes;

        let grille = app.world().resource::<BlindContext>().used_hands;
        let figure = app.world().resource::<HandContext>().active_evaluations[0].hand;
        {
            let mut main = app.world_mut().resource_mut::<HandContext>();
            select_hand(&mut main, &grille, figure);
        }
        frapper(app, KeyCode::Enter);
        deux_frames(app);

        // Vidée et commise, comme le fera le dépilement de l'Étape 4.
        let mut file = app.world_mut().resource_mut::<ScoringStepQueue>();
        file.steps.clear();
        file.committed = true;
        // Une frame pour que la transition soit posée, une pour qu'elle
        // s'applique : c'est celle-là qui fait tourner `OnExit(Scoring)`.
        deux_frames(app);
    }

    #[test]
    fn test_advance_runs_once_per_hand() {
        // Une garde `in_state(Scoring)` en `Update` compterait les relances à
        // chaque frame ; ce montage laisse passer plusieurs frames par main et
        // exige que le compteur avance d'un cran exactement.
        let mut app = app_en_run(CupId::Standard);
        poser_tirelire(&mut app);

        jouer_une_main(&mut app, 2);
        assert_eq!(tirelire(&mut app), RelicState::Counter(2));

        deux_frames(&mut app);
        deux_frames(&mut app);
        assert_eq!(
            tirelire(&mut app),
            RelicState::Counter(2),
            "le compteur a bougé sans qu'une main soit jouée"
        );

        jouer_une_main(&mut app, 1);
        assert_eq!(tirelire(&mut app), RelicState::Counter(3));
    }

    #[test]
    fn test_round_end_phase_does_not_collect() {
        // `RunPhase::RoundEnd` est entrée après **chaque** main. Y encaisser
        // rendrait le plafond inopérant : c'est le défaut que ce test garde.
        let mut app = app_en_run(CupId::Standard);
        poser_tirelire(&mut app);
        let depart = or(&app);

        jouer_une_main(&mut app, 2);
        assert_eq!(phase(&app), Some(RunPhase::RoundEnd));
        assert_eq!(or(&app), depart, "de l'or est tombé au bout d'une main");
        assert_eq!(tirelire(&mut app), RelicState::Counter(2));
    }

    #[test]
    fn test_relic_gold_collected_once_per_blind() {
        let mut app = app_en_run(CupId::Standard);
        poser_tirelire(&mut app);
        let depart = or(&app);

        for relances in [2, 1, 0] {
            jouer_une_main(&mut app, relances);
        }
        assert_eq!(tirelire(&mut app), RelicState::Counter(3));

        // La cible atteinte fait passer en boutique : la blind est finie.
        app.world_mut().resource_mut::<BlindContext>().current_score = u64::MAX;
        jouer_une_main(&mut app, 2);
        deux_frames(&mut app);

        assert_eq!(phase(&app), Some(RunPhase::Shop));
        assert_eq!(
            or(&app),
            depart + 5,
            "trois mains plus deux, sous le plafond"
        );

        deux_frames(&mut app);
        assert_eq!(or(&app), depart + 5, "l'or est tombé deux fois");
    }

    #[test]
    fn test_gold_collected_before_reset() {
        let mut app = app_en_run(CupId::Standard);
        poser_tirelire(&mut app);
        let depart = or(&app);

        app.world_mut().resource_mut::<BlindContext>().current_score = u64::MAX;
        jouer_une_main(&mut app, 2);
        deux_frames(&mut app);

        assert_eq!(or(&app), depart + 2, "remis à zéro avant d'être encaissé");
        assert_eq!(
            tirelire(&mut app),
            RelicState::Counter(0),
            "encaissé mais jamais remis à zéro"
        );
    }

    #[test]
    fn test_gold_added_with_saturating_add() {
        let mut app = app_en_run(CupId::Standard);
        poser_tirelire(&mut app);
        app.world_mut().resource_mut::<RunSession>().gold = u32::MAX - 1;

        app.world_mut().resource_mut::<BlindContext>().current_score = u64::MAX;
        jouer_une_main(&mut app, 5);
        deux_frames(&mut app);

        assert_eq!(or(&app), u32::MAX);
    }

    #[test]
    fn test_disabled_relic_neither_pays_nor_advances() {
        let mut app = app_en_run(CupId::Standard);
        poser_tirelire(&mut app);
        jouer_une_main(&mut app, 3);
        assert_eq!(tirelire(&mut app), RelicState::Counter(3));

        // Éteinte après coup : elle garde son compteur et ne verse rien.
        app.world_mut()
            .resource_mut::<RelicInventory>()
            .slots
            .iter_mut()
            .flatten()
            .for_each(|inst| inst.state = RelicState::Disabled);
        let depart = or(&app);

        app.world_mut().resource_mut::<BlindContext>().current_score = u64::MAX;
        jouer_une_main(&mut app, 2);
        deux_frames(&mut app);

        assert_eq!(or(&app), depart);
        assert_eq!(tirelire(&mut app), RelicState::Disabled);
    }
}
