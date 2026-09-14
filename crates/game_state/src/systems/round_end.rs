//! Arbitrage de fin de manche, et câblage des transitions de phase.
//!
//! # L'arbitre unique, et l'ordre de ses branches
//!
//! `resolve_round_outcome` est le **seul** juge d'une manche. L'ordre des trois
//! branches est normatif : la cible d'abord, les mains ensuite. Tester
//! `hands_remaining == 0` en premier transformerait chaque blind gagnée sur la
//! dernière main en défaite.
//!
//! Il **n'écrit rien** : le contexte de blind entre en `Res`, jamais en
//! `ResMut`. Recalculer un score ou décompter une main ne compile pas. Le
//! score qu'il lit a été commis par l'Étape 4 ; aujourd'hui il vaut zéro, et
//! l'arbitre rend donc toujours « main suivante » ou « défaite » en jeu réel.
//!
//! # Les huit transitions déclarées
//!
//! ```text
//! BlindSelect → Roll                       (blind_select_start)
//! Roll        → Scoring                    (submit_hand, TASK-36)
//! Scoring     → RoundEnd                   (file vide)
//! RoundEnd    → Roll                       (main suivante, même blind)
//! RoundEnd    → Shop                       (blind battue)
//! RoundEnd    → AppState::GameOver         (plus de main, cible non atteinte)
//! Shop        → BlindSelect                (« Continuer »)
//! BlindSelect → AppState::Victory          (check_run_completion, TASK-32)
//! ```
//!
//! **Deux d'entre elles n'avaient aucun déclencheur.** Mesuré : rien en
//! production ne posait `RunPhase::Roll`, et le corpus disait de
//! `Scoring → RoundEnd` à la fois qu'elle appartient à l'Étape 4 et que ce
//! ticket doit la couvrir au test — ce qui ne tient pas ensemble, car une
//! phase que personne ne quitte est un cul-de-sac. Elles sont donc posées ici.
//! L'Étape 4 possède la **cadence du dépilement**, pas la transition ; avec une
//! file pleine et pas d'Étape 4, la phase attend, ce qui est le comportement
//! voulu.
//!
//! Les déclencheurs de `blind_select_start` et de `shop_continue` sont
//! **provisoires** : une validation clavier, en attendant les écrans des
//! Étapes 6 et 7. Ce sont les déclencheurs qui changeront, jamais les
//! transitions.
//!
//! # Ce qui n'est délibérément pas câblé
//!
//! Les transitions de niveau `AppState` — `MainMenu → CupSelect → InRun`,
//! `MainMenu ↔ Codex`, `GameOver → MainMenu`, `Victory → MainMenu` — ne le sont
//! pas. Elles demanderaient six touches que le corpus n'a jamais arbitrées, et
//! surtout `CupSelect → InRun` ouvrirait une run **sans `RunSession`** :
//! mesuré, chaque système de run sort alors immédiatement, et on obtiendrait
//! une partie inerte atteignable au clavier. Ce chemin appartient à qui
//! **crée** la session, ce que personne ne fait encore.
//!
//! # `set_if_neq`, et ce qu'il garde vraiment
//!
//! La règle est sans exception, et elle est appliquée partout ici. Mais aucune
//! transition de ce fichier ne vise l'état courant, et les entités de run sont
//! rattachées à `DespawnOnExit(AppState::InRun)` : mesuré, `set` et
//! `set_if_neq` y sont indistinguables. Le couple de tests qui les distingue
//! vit à TASK-29, sur une transition vers l'état courant.

use bevy::prelude::*;
use core_engine::blinds::BlindContext;

use crate::plugin::{GameSet, InputSet};
use crate::resources::ScoringStepQueue;
use crate::states::{AppState, RunPhase};

/// Vrai si la transition de phase fait partie des huit déclarées.
///
/// Les paires portent des `Option` : la sous-phase **naît et meurt** avec
/// `AppState::InRun`, ce qui s'observe comme `None → Some(BlindSelect)` et
/// `Some(_) → None`. Ce ne sont pas des transitions de jeu mais de machine, et
/// le corpus les oubliait en parlant de « paires (from, to) ».
///
/// Les deux sorties vers `AppState` s'observent au niveau de la phase comme une
/// disparition de la sous-phase : elles sont donc listées à leur départ exact,
/// et non par un joker. Quitter la run depuis `Roll` n'est pas déclaré.
///
/// Publique, comme `select_hand` (TASK-36) : aucun système ne la consulte, et
/// une fonction privée sans appelant est du code mort refusé sous
/// `-D warnings`. C'est aussi la table que l'Étape 7 lira pour savoir ce qu'un
/// écran a le droit de déclencher.
pub fn is_declared(exited: Option<RunPhase>, entered: Option<RunPhase>) -> bool {
    matches!(
        (exited, entered),
        // Bruit de machine : apparition et disparition de la sous-phase.
        (None, None)
            | (None, Some(RunPhase::BlindSelect))
            // Les huit lignes de la table.
            | (Some(RunPhase::BlindSelect), Some(RunPhase::Roll))
            | (Some(RunPhase::Roll), Some(RunPhase::Scoring))
            | (Some(RunPhase::Scoring), Some(RunPhase::RoundEnd))
            | (Some(RunPhase::RoundEnd), Some(RunPhase::Roll))
            | (Some(RunPhase::RoundEnd), Some(RunPhase::Shop))
            | (Some(RunPhase::RoundEnd), None)
            | (Some(RunPhase::Shop), Some(RunPhase::BlindSelect))
            | (Some(RunPhase::BlindSelect), None)
    )
}

/// `Update`, sous `in_state(RunPhase::RoundEnd)`, dans `GameSet::Resolving`.
///
/// Idempotent : deux passages avant que la transition ne s'applique produisent
/// deux fois la même cible.
fn resolve_round_outcome(
    blind: Option<Res<BlindContext>>,
    mut phase: ResMut<NextState<RunPhase>>,
    mut app_state: ResMut<NextState<AppState>>,
) {
    let Some(blind) = blind else {
        return;
    };

    // La cible d'abord. L'inversion est le défaut à ne pas commettre.
    if blind.current_score >= blind.target_score {
        NextState::set_if_neq(&mut phase, RunPhase::Shop);
    } else if blind.hands_remaining == 0 {
        NextState::set_if_neq(&mut app_state, AppState::GameOver);
    } else {
        NextState::set_if_neq(&mut phase, RunPhase::Roll);
    }
}

/// `Update`, sous `in_state(RunPhase::Scoring)` : la phase se quitte quand la
/// file est vide **et le score commis**. L'Étape 4 la vide au rythme de son
/// animation, puis commet après sa pause finale.
///
/// **Les deux conditions, et pas seulement la première.** La file est vide dès
/// que le dernier palier est dépilé, mais le commit n'a lieu qu'une demi-seconde
/// plus tard : quitter à la première condition ferait arbitrer `RoundEnd` sur un
/// `BlindContext` où rien n'a été ajouté, où aucune main n'a été décomptée, et la
/// run boucherait sur `Roll` avec un score nul. Le drapeau ordonne les deux
/// crates sans dépendre de l'ordonnancement des systèmes de l'une et de l'autre.
///
/// **La transition reste ici, la décision de TASK-38 ne bouge pas.** Sans Étape 4
/// montée, la file ne se vide jamais et la phase attend, ce qui est le
/// comportement voulu ; une file au repos, elle, naît `committed` et sort
/// aussitôt, n'ayant rien à rejouer.
fn leave_scoring_when_queue_is_empty(
    queue: Option<Res<ScoringStepQueue>>,
    mut phase: ResMut<NextState<RunPhase>>,
) {
    let Some(queue) = queue else {
        return;
    };
    if queue.steps.is_empty() && queue.committed {
        NextState::set_if_neq(&mut phase, RunPhase::RoundEnd);
    }
}

/// Déclencheur **provisoire** de `BlindSelect → Roll`. L'écran de sélection de
/// blind (Étape 6) le remplacera ; la transition, elle, ne bougera pas.
///
/// Inerte quand une victoire est en attente : `check_run_completion` (TASK-32)
/// l'a posée dans le même `OnEnter`, et entamer une manche sur une run déjà
/// gagnée n'aurait aucun sens.
fn blind_select_start(keys: Res<ButtonInput<KeyCode>>, mut phase: ResMut<NextState<RunPhase>>) {
    if keys.just_pressed(KeyCode::Enter) {
        NextState::set_if_neq(&mut phase, RunPhase::Roll);
    }
}

/// Branche l'arbitre et les déclencheurs de transition.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        resolve_round_outcome
            .in_set(GameSet::Resolving)
            .run_if(in_state(RunPhase::RoundEnd)),
    );

    app.add_systems(
        Update,
        leave_scoring_when_queue_is_empty
            .in_set(GameSet::Resolving)
            .run_if(in_state(RunPhase::Scoring)),
    );

    // Les deux déclencheurs sont des **entrées** : ils rejoignent l'ensemble
    // gelé par l'overlay, comme la relance et la soumission.
    // **Le déclencheur de sortie de boutique a migré.** Il vit désormais dans
    // la crate de boutique, sur l'interaction du bouton « Continuer » : le sens
    // des dépendances interdit à cette crate de lire un bouton de celle-là. La
    // **transition**, elle, n'a pas changé d'un caractère.
    app.add_systems(
        Update,
        blind_select_start
            .run_if(in_state(RunPhase::BlindSelect))
            .run_if(not(crate::systems::setup::victory_is_pending))
            .in_set(InputSet::FrozenByOverlay),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::DieView;
    use crate::resources::{HandContext, RunSession, ScoringStepQueue};
    use crate::systems::fixtures::{app_en_run, entrer_dans_roll, frapper};
    use crate::systems::input::select_hand;
    use core_engine::blinds::{BlindContext, BlindType};
    use core_engine::cups::CupId;

    type Paire = (Option<RunPhase>, Option<RunPhase>);

    /// Une ligne de la table : son nom, la transition attendue, le scénario
    /// qui l'exerce.
    type Cas = (&'static str, Paire, fn(&mut App));

    fn entrer_dans(app: &mut App, phase: RunPhase) {
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(phase);
        app.update();
    }

    fn phase(app: &App) -> Option<RunPhase> {
        app.world()
            .get_resource::<State<RunPhase>>()
            .map(|etat| *etat.get())
    }

    fn etat(app: &App) -> AppState {
        *app.world().resource::<State<AppState>>().get()
    }

    /// Pose la manche sur un verdict donné, sans rien recalculer. Le score et
    /// les mains restantes sont écrits par l'Étape 4 ; ici, par le test.
    fn arbitrer(app: &mut App, score: u64, cible: u64, mains: u8) {
        let mut manche = app.world_mut().resource_mut::<BlindContext>();
        manche.current_score = score;
        manche.target_score = cible;
        manche.hands_remaining = mains;
    }

    fn entites_des(app: &mut App) -> Vec<Entity> {
        let mut q = app.world_mut().query::<(Entity, &DieView)>();
        let mut v: Vec<(Entity, u8)> = q.iter(app.world()).map(|(e, w)| (e, w.order)).collect();
        v.sort_by_key(|(_, rang)| *rang);
        v.into_iter().map(|(e, _)| e).collect()
    }

    /// Transitions enregistrées **au fil de l'eau**.
    #[derive(Resource, Default)]
    struct Journal(Vec<Paire>);

    /// Draine les messages de transition à chaque frame.
    ///
    /// **Lire l'historique à la fin ne marche pas.** `Messages<T>` est un
    /// tampon à deux temps, et sa rotation est pilotée par `FixedUpdate`, donc
    /// par le **temps réel**. Mesuré : pour le même scénario, sept transitions
    /// retenues sur la machine de développement, **trois** sur le runner de
    /// CI — le test passait ici et échouait là-bas. Un système qui draine
    /// chaque frame ne perd rien, quelle que soit la vitesse.
    ///
    /// Il tourne en `Last` : les transitions sont appliquées en
    /// `StateTransition`, plus tôt dans la même frame.
    fn enregistrer(
        mut journal: ResMut<Journal>,
        mut messages: MessageReader<StateTransitionEvent<RunPhase>>,
    ) {
        journal.0.extend(
            messages
                .read()
                .map(|message| (message.exited, message.entered)),
        );
    }

    /// Arme le journal. Les deux transitions de mise en route — l'apparition de
    /// la sous-phase — ont déjà eu lieu quand l'application est rendue ; elles
    /// sont du bruit de machine, et `is_declared` les couvre par ailleurs.
    fn armer_le_journal(app: &mut App) {
        app.init_resource::<Journal>();
        app.add_systems(Last, enregistrer);
    }

    fn transitions(app: &App) -> Vec<Paire> {
        app.world().resource::<Journal>().0.clone()
    }

    fn deux_frames(app: &mut App) {
        app.update();
        app.update();
    }

    // ---- les huit scénarios, un par ligne de la table ----

    fn cas_blind_select_vers_roll(app: &mut App) {
        frapper(app, KeyCode::Enter);
        deux_frames(app);
    }

    fn cas_roll_vers_scoring(app: &mut App) {
        entrer_dans_roll(app);
        soumettre_une_figure(app);
    }

    /// Choisit la première figure évaluée et la soumet, depuis la phase de
    /// lancer.
    fn soumettre_une_figure(app: &mut App) {
        let manche = app.world().resource::<BlindContext>().clone();
        let figure = app.world().resource::<HandContext>().active_evaluations[0].hand;
        {
            let mut main = app.world_mut().resource_mut::<HandContext>();
            select_hand(&mut main, &manche, figure);
        }
        frapper(app, KeyCode::Enter);
        deux_frames(app);
    }

    fn cas_scoring_vers_round_end(app: &mut App) {
        entrer_dans_roll(app);
        entrer_dans(app, RunPhase::Scoring);
        vider_et_commettre(app);
        deux_frames(app);
    }

    fn cas_round_end_vers_roll(app: &mut App) {
        entrer_dans_roll(app);
        arbitrer(app, 0, 300, 3);
        entrer_dans(app, RunPhase::RoundEnd);
        deux_frames(app);
    }

    fn cas_round_end_vers_shop(app: &mut App) {
        entrer_dans_roll(app);
        arbitrer(app, 500, 300, 2);
        entrer_dans(app, RunPhase::RoundEnd);
        deux_frames(app);
    }

    fn cas_round_end_vers_game_over(app: &mut App) {
        entrer_dans_roll(app);
        arbitrer(app, 100, 300, 0);
        entrer_dans(app, RunPhase::RoundEnd);
        deux_frames(app);
    }

    /// **Le déclencheur a migré, la transition non.** Le bouton « Continuer »
    /// vit dans la crate de boutique, que celle-ci ne peut pas connaître : la
    /// transition se pilote donc directement, ce qui est bien ce que ce test
    /// mesure — qu'elle est **déclarée et atteignable**, non qu'une touche la
    /// provoque. Le déclencheur, lui, est éprouvé là où il vit.
    fn cas_shop_vers_blind_select(app: &mut App) {
        entrer_dans(app, RunPhase::Shop);
        {
            let mut phase = app.world_mut().resource_mut::<NextState<RunPhase>>();
            NextState::set_if_neq(&mut phase, RunPhase::BlindSelect);
        }
        deux_frames(app);
    }

    fn cas_blind_select_vers_victory(app: &mut App) {
        app.world_mut().resource_mut::<RunSession>().ante = 8;
        {
            let mut manche = app.world_mut().resource_mut::<BlindContext>();
            manche.blind.kind = BlindType::Boss;
            manche.current_score = manche.target_score;
        }
        entrer_dans(app, RunPhase::Shop);
        entrer_dans(app, RunPhase::BlindSelect);
        deux_frames(app);
    }

    fn observer(scenario: fn(&mut App)) -> Vec<Paire> {
        let mut app = app_en_run(CupId::Standard);
        armer_le_journal(&mut app);
        scenario(&mut app);
        transitions(&app)
    }

    // ---- tests ----

    #[test]
    fn test_round_end_to_roll_is_set_if_neq() {
        // **Ce que ce test prouve, et ce qu'il ne prouve pas.** Il prouve que
        // les entités de run survivent au passage à la main suivante. Il ne
        // distingue **pas** `set` de `set_if_neq`, et le corpus se trompe en
        // l'affirmant : `RoundEnd → Roll` n'est pas une transition vers l'état
        // courant, et les dés sont rattachés à `DespawnOnExit(AppState::InRun)`
        // depuis TASK-33, donc aucune transition de phase ne les touche.
        // Mesuré : cinq dés avant, cinq après, entités identiques, dans les
        // deux formes d'appel.
        //
        // Le couple qui les distingue existe, à TASK-29 :
        // `test_set_if_neq_on_current_state_is_inert` et
        // `test_plain_set_on_current_state_despawns`, sur une transition vers
        // l'état courant et une entité rattachée à la phase.
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        let avant = entites_des(&mut app);
        assert!(!avant.is_empty());

        arbitrer(&mut app, 0, 300, 3);
        deux_frames(&mut app);

        assert_eq!(phase(&app), Some(RunPhase::Roll));
        assert_eq!(entites_des(&mut app), avant, "des dés ont été détruits");
    }

    #[test]
    fn test_game_over_when_hands_exhausted() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        arbitrer(&mut app, 100, 300, 0);
        entrer_dans(&mut app, RunPhase::RoundEnd);

        deux_frames(&mut app);

        assert_eq!(etat(&app), AppState::GameOver);
    }

    #[test]
    fn test_target_reached_on_the_last_hand_goes_to_shop() {
        // Cas limite normatif : cible atteinte **et** plus aucune main. Tester
        // les mains avant la cible transformerait chaque blind gagnée de
        // justesse en défaite.
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        arbitrer(&mut app, 300, 300, 0);
        entrer_dans(&mut app, RunPhase::RoundEnd);

        deux_frames(&mut app);

        assert_eq!(etat(&app), AppState::InRun, "la run s'est arrêtée");
        assert_eq!(phase(&app), Some(RunPhase::Shop));
    }

    #[test]
    fn test_round_end_to_shop_when_target_reached() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        arbitrer(&mut app, 500, 300, 2);
        entrer_dans(&mut app, RunPhase::RoundEnd);

        deux_frames(&mut app);

        assert_eq!(phase(&app), Some(RunPhase::Shop));
    }

    #[test]
    fn test_blind_select_start_enters_roll() {
        let mut app = app_en_run(CupId::Standard);
        assert_eq!(phase(&app), Some(RunPhase::BlindSelect));

        frapper(&mut app, KeyCode::Enter);
        deux_frames(&mut app);

        assert_eq!(phase(&app), Some(RunPhase::Roll));
    }

    /// Vide la file **et pose le drapeau de commit**, comme le fera l'Étape 4
    /// après sa pause finale. Les deux gestes vont ensemble : la phase ne se
    /// quitte plus sur la seule file vide.
    fn vider_et_commettre(app: &mut App) {
        let mut file = app.world_mut().resource_mut::<ScoringStepQueue>();
        file.steps.clear();
        file.committed = true;
    }

    #[test]
    fn test_scoring_leaves_only_when_the_queue_is_empty() {
        // La file se remplit par une **vraie** soumission : `build_scoring_report`
        // remet la file à zéro à l'entrée dans la phase, donc un palier posé
        // avant d'y entrer serait effacé et la phase serait quittée aussitôt.
        let mut app = app_en_run(CupId::Standard);
        cas_roll_vers_scoring(&mut app);

        assert!(!app.world().resource::<ScoringStepQueue>().steps.is_empty());
        assert_eq!(
            phase(&app),
            Some(RunPhase::Scoring),
            "la phase a été quittée avec une file pleine"
        );

        // Vidée à la main, comme le fera le dépilement de l'Étape 4 — **sans
        // commettre** : la file vide ne suffit plus.
        app.world_mut()
            .resource_mut::<ScoringStepQueue>()
            .steps
            .clear();
        deux_frames(&mut app);
        assert_eq!(
            phase(&app),
            Some(RunPhase::Scoring),
            "la phase a été quittée avant que le score soit commis"
        );

        // Puis commise, comme le fera `commit_score_when_drained` après sa
        // pause finale.
        app.world_mut().resource_mut::<ScoringStepQueue>().committed = true;
        deux_frames(&mut app);
        assert_eq!(phase(&app), Some(RunPhase::RoundEnd));
    }

    #[derive(Resource, Default)]
    struct EntreesDansRoundEnd(usize);

    #[test]
    fn test_round_end_transition_once() {
        // **Trou trouvé par l'audit de fin d'étape.** La transition vers
        // `RoundEnd` a changé de main à TASK-50 — le commit vit dans le juice,
        // la sortie de phase reste ici — et personne n'avait écrit qu'elle n'a
        // lieu qu'une fois. Un `set()` nu au lieu de `set_if_neq`, ou une
        // condition qui resterait vraie, la rejouerait à chaque frame.
        let mut app = app_en_run(CupId::Standard);
        app.init_resource::<EntreesDansRoundEnd>();
        app.add_systems(
            OnEnter(RunPhase::RoundEnd),
            |mut compteur: ResMut<EntreesDansRoundEnd>| compteur.0 += 1,
        );

        cas_roll_vers_scoring(&mut app);
        vider_et_commettre(&mut app);
        for _ in 0..100 {
            app.update();
        }

        assert_eq!(
            app.world().resource::<EntreesDansRoundEnd>().0,
            1,
            "la fin de manche a été jouée plusieurs fois"
        );
    }

    #[test]
    fn test_scoring_never_leaves_while_steps_remain() {
        // **Ceinture et bretelles, et c'est délibéré.** Aujourd'hui `committed`
        // n'est jamais vrai avec des paliers en attente : le remplissage rend
        // toujours un garde-fou ouvert. Le banc a montré que la condition sur la
        // file est donc redondante — mais elle protège d'un futur remplissage
        // qui oublierait de rouvrir le garde-fou, et qui ferait sinon quitter la
        // phase en plein dépilement. Ce test tient cette protection.
        let mut app = app_en_run(CupId::Standard);
        cas_roll_vers_scoring(&mut app);

        app.world_mut().resource_mut::<ScoringStepQueue>().committed = true;
        deux_frames(&mut app);

        assert_eq!(
            phase(&app),
            Some(RunPhase::Scoring),
            "la phase a été quittée avec des paliers en attente"
        );
    }

    #[test]
    fn test_boss_ante_8_leads_to_victory() {
        // Exerce `check_run_completion`, livré par TASK-32, sans le
        // réimplémenter.
        let mut app = app_en_run(CupId::Standard);
        cas_blind_select_vers_victory(&mut app);

        assert_eq!(etat(&app), AppState::Victory);
    }

    #[test]
    fn test_all_eight_transitions_are_covered() {
        let cas: [Cas; 8] = [
            (
                "BlindSelect → Roll",
                (Some(RunPhase::BlindSelect), Some(RunPhase::Roll)),
                cas_blind_select_vers_roll,
            ),
            (
                "Roll → Scoring",
                (Some(RunPhase::Roll), Some(RunPhase::Scoring)),
                cas_roll_vers_scoring,
            ),
            (
                "Scoring → RoundEnd",
                (Some(RunPhase::Scoring), Some(RunPhase::RoundEnd)),
                cas_scoring_vers_round_end,
            ),
            (
                "RoundEnd → Roll",
                (Some(RunPhase::RoundEnd), Some(RunPhase::Roll)),
                cas_round_end_vers_roll,
            ),
            (
                "RoundEnd → Shop",
                (Some(RunPhase::RoundEnd), Some(RunPhase::Shop)),
                cas_round_end_vers_shop,
            ),
            (
                "RoundEnd → GameOver",
                (Some(RunPhase::RoundEnd), None),
                cas_round_end_vers_game_over,
            ),
            (
                "Shop → BlindSelect",
                (Some(RunPhase::Shop), Some(RunPhase::BlindSelect)),
                cas_shop_vers_blind_select,
            ),
            (
                "BlindSelect → Victory",
                (Some(RunPhase::BlindSelect), None),
                cas_blind_select_vers_victory,
            ),
        ];

        for (nom, attendue, scenario) in cas {
            let observees = observer(scenario);
            assert!(
                observees.contains(&attendue),
                "{nom} n'a pas été exercée. Observées : {observees:?}"
            );
        }
    }

    #[test]
    fn test_no_undeclared_transition_is_reachable() {
        // **Un tour de boucle complet**, et chaque étape est vérifiée au
        // passage. Sans ces assertions intermédiaires, un scénario qui
        // s'arrêterait en chemin passerait le test sans avoir rien parcouru —
        // c'est ce que le banc a montré.
        let mut app = app_en_run(CupId::Standard);
        armer_le_journal(&mut app);

        cas_blind_select_vers_roll(&mut app);
        assert_eq!(phase(&app), Some(RunPhase::Roll), "entrée dans la main");

        soumettre_une_figure(&mut app);
        assert_eq!(phase(&app), Some(RunPhase::Scoring), "soumission");

        // Cible atteinte : l'arbitre enverra en boutique. Posé avant que la
        // file ne se vide, sinon l'arbitre tranche sur l'ancien contexte.
        arbitrer(&mut app, 500, 300, 2);
        vider_et_commettre(&mut app);
        deux_frames(&mut app);
        deux_frames(&mut app);
        assert_eq!(phase(&app), Some(RunPhase::Shop), "blind battue");

        // Même motif : la transition se pilote, son déclencheur vivant dans la
        // crate de boutique.
        {
            let mut phase = app.world_mut().resource_mut::<NextState<RunPhase>>();
            NextState::set_if_neq(&mut phase, RunPhase::BlindSelect);
        }
        deux_frames(&mut app);
        assert_eq!(phase(&app), Some(RunPhase::BlindSelect), "continuer");

        // Cinq, et non sept : les deux transitions de mise en route ont eu lieu
        // avant que le journal ne soit armé.
        let observees = transitions(&app);
        assert!(observees.len() >= 5, "scénario trop court : {observees:?}");
        for paire in observees {
            assert!(
                is_declared(paire.0, paire.1),
                "transition non déclarée atteinte : {paire:?}"
            );
        }
    }

    #[test]
    fn test_blind_select_start_is_inert_on_a_won_run() {
        // Entamer une manche sur une run déjà gagnée n'a aucun sens. La
        // victoire l'emporterait de toute façon à la transition suivante,
        // donc rien ne se verrait à l'écran : ce test regarde l'intention
        // posée, et non son effet.
        let mut app = app_en_run(CupId::Standard);
        app.world_mut().resource_mut::<RunSession>().ante = 8;
        {
            let mut manche = app.world_mut().resource_mut::<BlindContext>();
            manche.blind.kind = BlindType::Boss;
            manche.current_score = manche.target_score;
        }
        entrer_dans(&mut app, RunPhase::Shop);

        // **La frappe est posée avant la frame de la transition.** C'est la
        // seule frame où l'on soit en sélection de blind avec une victoire en
        // attente : à la suivante, la sortie de la run a déjà retiré la
        // sous-phase et le système ne tourne plus du tout.
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::BlindSelect);
        frapper(&mut app, KeyCode::Enter);
        app.update();

        assert!(
            matches!(
                *app.world().resource::<NextState<AppState>>(),
                NextState::PendingIfNeq(AppState::Victory)
            ),
            "la victoire n'est pas en attente : le scénario ne teste rien"
        );
        assert!(
            matches!(
                *app.world().resource::<NextState<RunPhase>>(),
                NextState::Unchanged
            ),
            "une manche a été entamée sur une run gagnée"
        );
    }

    #[test]
    fn test_undeclared_transitions_are_actually_refused() {
        // Contre-épreuve : sans elle, une table blanche qui dirait « vrai »
        // partout passerait le test précédent.
        assert!(!is_declared(Some(RunPhase::Shop), Some(RunPhase::Roll)));
        assert!(!is_declared(
            Some(RunPhase::RoundEnd),
            Some(RunPhase::BlindSelect)
        ));
        assert!(!is_declared(Some(RunPhase::Roll), None));
    }
}
