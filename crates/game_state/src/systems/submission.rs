//! Soumission de la figure choisie.
//!
//! # Ce fichier n'écrit ni le score, ni la grille consommée, ni les mains
//!
//! Trois écritures appartiennent au **commit unique** de l'Étape 4, à la fin du
//! dépilement de la file (ADR-010) : le score courant, le nombre de mains
//! restantes, et la consommation de la case dans la grille. Les faire ici les
//! ferait exécuter deux fois, ce qui rendrait la blind injouable.
//!
//! C'est contre-intuitif — le refus **lit** la grille, il serait naturel de l'y
//! écrire dans la foulée — et la garde textuelle ne suffirait pas à l'empêcher.
//! Ce qui l'empêche est le **type** : le contexte de blind et celui de main
//! entrent en `Res`, jamais en `ResMut`. Une écriture ne compile pas.
//!
//! # Le refus précède le moindre marqueur
//!
//! Poser le marqueur de comptabilisation puis refuser laisserait des dés
//! marqués dans une phase de lancer, et le nettoyage est le travail de
//! `setup_round`, pas un rattrapage.
//!
//! # Une figure non réalisée est acceptée
//!
//! Si aucune évaluation ne correspond à la case choisie, aucun dé n'est marqué
//! et la transition a **quand même** lieu : la case est consommée pour un score
//! faible, ce qui est le risque que vend *La Fissure*, pas un refus.
//!
//! `set` suffit ici, la phase de comptage ne pouvant pas être la phase
//! courante ; `set_if_neq` sera requis en TASK-38, pour le retour au lancer.

use bevy::prelude::*;
use core_engine::blinds::BlindContext;
use core_engine::dice::Die;
use core_engine::relics::RelicInventory;
use core_engine::scoring::ScoringPipeline;

use crate::components::{DieView, Hidden, Scoring};
use crate::plugin::InputSet;
use crate::resources::{HandContext, RunSession, ScoringStepQueue};
use crate::states::RunPhase;
use crate::systems::evaluation::{is_hand_available, remplir, tranche_evaluable};

/// `Update`, sous `in_state(RunPhase::Roll)`, dans l'ensemble gelé par
/// l'overlay. C'est un système d'**entrée** : ni `OnExit`, ni `OnEnter`.
fn submit_hand(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    session: Option<Res<RunSession>>,
    blind: Option<Res<BlindContext>>,
    hand: Option<ResMut<HandContext>>,
    dice: Query<(Entity, &Die)>,
    mut next: ResMut<NextState<RunPhase>>,
) {
    if !keys.just_pressed(KeyCode::Enter) {
        return;
    }

    let (Some(session), Some(blind), Some(mut hand)) = (session, blind, hand) else {
        return;
    };

    // Les deux refus, avant tout marqueur et avant toute transition.
    let Some(cell) = hand.selected_hand else {
        return;
    };
    if !is_hand_available(&blind, cell) {
        return;
    }

    // **La révélation, ici et pas ailleurs.** *La Fissure* a laissé la liste
    // des figures vide toute la main : le marqueur `Scoring` se pose juste
    // dessous, et `build_scoring_report` relit `active_evaluations` à l'entrée
    // dans la phase de comptage. Compter sur le recalcul temps réel pour
    // repasser après coup suspendrait les deux à l'ordre de deux ensembles et
    // à une transition qui n'a pas encore pris effet.
    //
    // Le retrait passe par `Commands`, donc il est différé : c'est pourquoi le
    // recalcul ci-dessous ne consulte aucun marqueur et évalue la main
    // entière. `remplir` est le **même** chemin que l'évaluation temps réel ;
    // un second divergerait, et l'écart ne se verrait qu'au score.
    for (entity, _) in &dice {
        commands.entity(entity).remove::<Hidden>();
    }
    let roll = tranche_evaluable(dice.iter().map(|(_, die)| die.clone()), Some(&blind));
    hand.active_evaluations = remplir(&roll, &session.hand_levels);

    // Marquer exactement les dés retenus par la figure, et aucun autre. Aucun
    // dé n'est retiré, verrouillé ou non : seul le marqueur distingue les dés
    // comptabilisés, et les autres restent affichés pour l'animation de
    // l'Étape 4.
    if let Some(found) = hand
        .active_evaluations
        .iter()
        .find(|evaluated| evaluated.hand == cell)
    {
        for (entity, die) in &dice {
            if found.scoring_dice.contains(&die.id) {
                commands.entity(entity).insert(Scoring);
            }
        }
    }

    next.set(RunPhase::Scoring);
}

/// `OnEnter(RunPhase::Scoring)` : calcule le rapport et remplit la file.
///
/// **Une fois par entrée dans la phase**, jamais en `Update`. Il remplit ; le
/// dépilement et le commit sont l'Étape 4.
///
/// La file est **remplacée**, jamais complétée : une seconde entrée dans la
/// phase doit rendre la même file, pas une file doublée. Elle est donc remise à
/// zéro d'entrée de jeu, avant même les refus, ce qui évite qu'un rapport
/// périmé survive à une entrée qui ne calcule rien et se fasse commettre deux
/// fois par l'Étape 4.
///
/// **Deux refus, et aucune synthèse.** Sans figure choisie — cas réel : on
/// entre dans cette phase autrement que par la soumission — ou sans
/// `HandMatch` correspondant — cas voulu par TASK-36, la figure choisie
/// pouvant n'être pas réalisée — la file reste vide et le rapport absent.
/// Fabriquer un `HandMatch` à `scoring_dice` vide donnerait au joueur la base
/// de la figure, chips et mult au niveau courant : ce serait un arbitrage
/// d'équilibrage, et le corpus ne chiffre pas ce « score faible ».
fn build_scoring_report(
    session: Option<Res<RunSession>>,
    blind: Option<Res<BlindContext>>,
    hand: Option<Res<HandContext>>,
    relics: Option<Res<RelicInventory>>,
    dice: Query<(&DieView, &Die)>,
    queue: Option<ResMut<ScoringStepQueue>>,
) {
    let (Some(session), Some(blind), Some(hand), Some(relics), Some(mut queue)) =
        (session, blind, hand, relics, queue)
    else {
        return;
    };

    // Le réglage de vitesse est un choix de **joueur**, pas un état de manche :
    // il traverse la remise à zéro. Sans cela, un joueur qui choisit x4 le
    // reperd à la main suivante, et rien ne le signale.
    remettre_a_zero(&mut queue, ScoringStepQueue::default());

    let Some(cell) = hand.selected_hand else {
        return;
    };
    let Some(found) = hand
        .active_evaluations
        .iter()
        .find(|evaluated| evaluated.hand == cell)
    else {
        return;
    };

    // **La main entière**, jamais les seuls dés marqués. La raison n'est pas
    // que la résolution casserait — elle ignore sans panique un identifiant
    // absent — ni que le score changerait, le marqueur étant posé depuis
    // `scoring_dice` : c'est que ce slice devient `TriggerCtx.dice`, ce que les
    // reliques liront aux Étapes 5 et 9. Une relique qui compte les dés écartés
    // verrait sinon une main tronquée.
    //
    // Trié sur le rang d'affichage, puis sur l'identifiant : l'ordre
    // d'itération d'une requête n'est pas un contrat, et le rang seul n'est pas
    // une clé totale.
    let mut roll: Vec<(u8, Die)> = dice
        .iter()
        .map(|(view, die)| (view.order, die.clone()))
        .collect();
    roll.sort_unstable_by_key(|(order, die)| (*order, die.id));
    let roll: Vec<Die> = roll.into_iter().map(|(_, die)| die).collect();

    let report = ScoringPipeline::resolve(found, &roll, &session.hand_levels, &relics, &blind);

    // Les paliers sont déplacés tels quels : ni tri, ni déduplication, ni
    // filtrage des paliers nuls. Un palier nul reste un palier, l'Étape 4
    // l'anime.
    remettre_a_zero(
        &mut queue,
        ScoringStepQueue::new(
            report.steps.iter().copied().collect(),
            found.hand,
            report.final_score,
        ),
    );
}

/// Remplace la file en **conservant la vitesse de relecture**.
///
/// `new` et `Default` posent tous deux `speed_multiplier = 1`, ce qui est juste
/// pour une file neuve et faux pour le joueur : la valeur courante est donc
/// relue et reposée par le seul chemin d'écriture. L'`expect` n'est pas
/// décoratif — le champ est `pub`, et quiconque y écrirait un 3 à la main
/// l'apprendrait ici plutôt qu'à l'Étape 11.
fn remettre_a_zero(queue: &mut ScoringStepQueue, suivante: ScoringStepQueue) {
    let vitesse = queue.speed_multiplier;
    *queue = suivante;
    queue
        .set_speed_multiplier(vitesse)
        .expect("la vitesse courante est valide par construction");
}

/// Branche la soumission et le calcul du rapport.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        submit_hand
            .in_set(InputSet::FrozenByOverlay)
            .run_if(in_state(RunPhase::Roll)),
    );

    app.add_systems(OnEnter(RunPhase::Scoring), build_scoring_report);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Hidden;
    use core_engine::blinds::BlindContext;
    use core_engine::cups::CupId;
    use core_engine::dice::{Die, DieId};
    use core_engine::hands::YahtzeeHand;

    use core_engine::relics::RelicInventory;
    use core_engine::scoring::ScoreStep;

    use crate::components::{Locked, Scoring};
    use crate::resources::{HandContext, ScoringStepQueue};
    use crate::states::SettingsOverlay;
    use crate::systems::fixtures::{app_en_run, des_tries, entites_des, entrer_dans_roll, frapper};
    use crate::systems::input::select_hand;

    /// Application en phase de lancer, dés imposés à `[5,5,5,2,2]`, figure
    /// choisie par le point d'entrée unique.
    fn app_prete(figure: Option<YahtzeeHand>) -> App {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        for (entite, valeur) in entites_des(&mut app).into_iter().zip([5, 5, 5, 2, 2]) {
            app.world_mut()
                .get_mut::<Die>(entite)
                .expect("dé")
                .current_value = valeur;
        }
        app.update();

        if let Some(figure) = figure {
            choisir(&mut app, figure);
        }
        app
    }

    /// Référence **indépendante** de la file : rejoue le pipeline sur les
    /// mêmes entrées que le système, sans passer par la ressource testée.
    ///
    /// Le tri des dés est repris à l'identique du système : c'est l'entrée du
    /// pipeline, pas sa sortie, et une divergence de tri ferait échouer la
    /// comparaison pour la mauvaise raison.
    fn rapport_de_reference(app: &mut App) -> core_engine::scoring::ScoringReport {
        let mut roll: Vec<(u8, Die)> = des_tries(app)
            .into_iter()
            .map(|(_, die, view)| (view.order, die))
            .collect();
        roll.sort_unstable_by_key(|(order, die)| (*order, die.id));
        let roll: Vec<Die> = roll.into_iter().map(|(_, die)| die).collect();

        let monde = app.world();
        let session = monde.resource::<RunSession>();
        let blind = monde.resource::<BlindContext>();
        let main = monde.resource::<HandContext>();
        let relics = monde.resource::<RelicInventory>();

        let cell = main.selected_hand.expect("figure choisie");
        let found = main
            .active_evaluations
            .iter()
            .find(|evaluated| evaluated.hand == cell)
            .expect("figure réalisée");

        ScoringPipeline::resolve(found, &roll, &session.hand_levels, relics, blind)
    }

    /// Passe par `select_hand`, jamais par une écriture directe : c'est le
    /// point d'entrée unique, et les tests le traitent comme le clic le fera.
    fn choisir(app: &mut App, figure: YahtzeeHand) {
        let manche = app.world().resource::<BlindContext>().clone();
        let mut main = app.world_mut().resource_mut::<HandContext>();
        select_hand(&mut main, &manche, figure);
    }

    fn consommer(app: &mut App, figure: YahtzeeHand) {
        app.world_mut()
            .resource_mut::<BlindContext>()
            .used_hands
            .mark(figure);
    }

    fn phase(app: &App) -> RunPhase {
        *app.world().resource::<State<RunPhase>>().get()
    }

    // ---- TASK-74 : *L'Oubli* refuse, comme la grille consommée refuse ----

    /// Pose sur la manche vivante la contrainte que le catalogue rend.
    fn poser_le_boss(app: &mut App, id: core_engine::blinds::definitions::BossId) {
        let mut rng = core_engine::rng::RunRng::from_seed(0);
        let contrainte = core_engine::blinds::definitions::boss_definition(id, &mut rng.boss, 5);
        app.world_mut()
            .resource_mut::<BlindContext>()
            .blind
            .modifier = Some(contrainte.modifier);
    }

    #[test]
    fn test_oubli_debuffs_two_hands() {
        use core_engine::blinds::{BlindModifier, definitions::BossId};

        let mut app = app_prete(None);
        poser_le_boss(&mut app, BossId::Oblivion);

        // Le modificateur porte **deux** figures, ni une ni trois.
        let Some(BlindModifier::DebuffHands(figures)) = app
            .world()
            .resource::<BlindContext>()
            .blind
            .modifier
            .clone()
        else {
            panic!("L'Oubli affaiblit des figures");
        };
        assert_eq!(
            figures.as_slice(),
            [YahtzeeHand::Chance, YahtzeeHand::Yahtzee]
        );

        for figure in [YahtzeeHand::Chance, YahtzeeHand::Yahtzee] {
            // La sélection du tour précédent survit à son refus : la remettre
            // à zéro est le seul moyen d'observer ce que `select_hand` décide
            // pour **cette** figure.
            app.world_mut().resource_mut::<HandContext>().selected_hand = None;

            // `select_hand` refuse déjà : c'est le même prédicat.
            choisir(&mut app, figure);
            assert_eq!(figure_selectionnee(&app), None, "{figure:?} a été choisie");

            // Et si la sélection venait d'ailleurs, la soumission refuse aussi.
            app.world_mut().resource_mut::<HandContext>().selected_hand = Some(figure);
            let grille = app.world().resource::<BlindContext>().used_hands;
            frapper(&mut app, KeyCode::Enter);
            // **Deux frames.** `frapper` n'écrit qu'un message ; la première
            // frame l'exécute, la seconde applique le `NextState`. Sans elles,
            // l'assertion « toujours dans Roll » ne constate rien du tout.
            app.update();
            app.update();

            assert_eq!(phase(&app), RunPhase::Roll, "{figure:?} a fait transiter");
            assert!(
                des_marques(&mut app).is_empty(),
                "{figure:?} a posé un marqueur"
            );
            assert_eq!(
                app.world().resource::<BlindContext>().used_hands,
                grille,
                "{figure:?} a consommé la grille"
            );
        }

        // Contre-épreuve : une figure non interdite passe, sans quoi le test
        // passerait sur une soumission cassée pour tout le monde.
        app.world_mut().resource_mut::<HandContext>().selected_hand = None;
        choisir(&mut app, YahtzeeHand::FullHouse);
        assert_eq!(figure_selectionnee(&app), Some(YahtzeeHand::FullHouse));
        frapper(&mut app, KeyCode::Enter);
        app.update();
        app.update();
        assert_eq!(phase(&app), RunPhase::Scoring);
    }

    // ---- TASK-75 : *La Fissure* ----

    /// Les entités portant le marqueur d'occultation.
    fn masques(app: &mut App) -> Vec<Entity> {
        let mut q = app.world_mut().query_filtered::<Entity, With<Hidden>>();
        let mut v: Vec<Entity> = q.iter(app.world()).collect();
        v.sort();
        v
    }

    fn evaluations(app: &App) -> usize {
        app.world()
            .resource::<HandContext>()
            .active_evaluations
            .len()
    }

    #[test]
    fn test_submission_reveals_and_reevaluates() {
        let mut app = app_prete(None);
        poser_le_boss(&mut app, core_engine::blinds::definitions::BossId::Rift);
        // La manche est entrée avant que le boss soit posé : on repasse par
        // une main pour que `setup_round` applique l'occultation.
        entrer_dans_roll(&mut app);
        app.update();

        assert_eq!(masques(&mut app).len(), 2, "deux dés cachés");
        assert_eq!(evaluations(&app), 0, "la liste doit rester vide");

        // La case se choisit à l'aveugle, puis la soumission révèle.
        app.world_mut().resource_mut::<HandContext>().selected_hand = Some(YahtzeeHand::Chance);
        frapper(&mut app, KeyCode::Enter);
        // Deux frames : la première exécute la soumission, la seconde applique
        // le `NextState`.
        app.update();
        app.update();

        assert!(masques(&mut app).is_empty(), "un marqueur a survécu");
        assert!(evaluations(&app) > 0, "la révélation n'a rien rempli");
        assert_eq!(phase(&app), RunPhase::Scoring);

        // **Ce que la révélation sert, et ce qui la rend nécessaire.** Le
        // recalcul doit avoir lieu **dans** la soumission : le marqueur se pose
        // juste dessous, et le rapport se construit à l'entrée dans la phase.
        // Sans ces deux assertions, une révélation qui ne recalculerait rien
        // passerait au vert, la frame suivante remplissant la liste de toute
        // façon une fois les marqueurs retirés.
        assert!(
            !des_marques(&mut app).is_empty(),
            "aucun dé marqué : la figure n'a pas été retrouvée à la révélation"
        );
        assert!(
            !app.world().resource::<ScoringStepQueue>().steps.is_empty(),
            "la file est vide : le rapport n'a rien reçu"
        );
    }

    #[test]
    fn test_unrealised_hand_is_submittable() {
        // **Le cœur du boss.** Une figure non réalisée transite quand même, et
        // la case est misée. Mesuré : il n'y a alors **aucun** score, et non un
        // score faible : `build_scoring_report` sort tôt faute de `HandMatch`,
        // et `ScoringStepQueue` reste vide. Le « score faible » que le corpus
        // promet n'est chiffré nulle part, et le sort de la case est une
        // question ouverte pour TASK-81 et l'Étape 6 bis.
        let mut app = app_prete(None);
        // 5-5-5-2-2 : ni Yams, ni Grande Suite.
        app.world_mut().resource_mut::<HandContext>().selected_hand = Some(YahtzeeHand::Yahtzee);
        assert!(
            !app.world()
                .resource::<HandContext>()
                .active_evaluations
                .iter()
                .any(|trouvee| trouvee.hand == YahtzeeHand::Yahtzee),
            "la main réalise le Yams : le montage ne prouve rien"
        );

        frapper(&mut app, KeyCode::Enter);
        app.update();
        app.update();

        assert_eq!(
            phase(&app),
            RunPhase::Scoring,
            "la soumission a été refusée"
        );
        assert!(
            des_marques(&mut app).is_empty(),
            "un dé a été marqué pour une figure non réalisée"
        );
        assert!(
            app.world().resource::<ScoringStepQueue>().steps.is_empty(),
            "un score a été fabriqué pour une figure non réalisée"
        );
    }

    /// Les `DieId` portant le marqueur de comptabilisation, triés.
    fn des_marques(app: &mut App) -> Vec<DieId> {
        let mut q = app.world_mut().query_filtered::<&Die, With<Scoring>>();
        let mut v: Vec<DieId> = q.iter(app.world()).map(|die| die.id).collect();
        v.sort_unstable();
        v
    }

    fn figure_selectionnee(app: &App) -> Option<YahtzeeHand> {
        app.world().resource::<HandContext>().selected_hand
    }

    /// Soumet la figure choisie et laisse la transition s'appliquer.
    fn soumettre(app: &mut App) {
        frapper(app, KeyCode::Enter);
        app.update();
        app.update();
    }

    /// Ré-entre dans la phase de comptage, mêmes entrées. Un `set` nu vers
    /// l'état courant : la transition a bien lieu, et `OnEnter` retourne.
    fn reentrer_dans_scoring(app: &mut App) {
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::Scoring);
        app.update();
    }

    fn paliers(app: &App) -> Vec<ScoreStep> {
        app.world()
            .resource::<ScoringStepQueue>()
            .steps
            .iter()
            .cloned()
            .collect()
    }

    #[test]
    fn test_scoring_phase_commits_nothing() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        let avant = app.world().resource::<BlindContext>().clone();

        soumettre(&mut app);
        assert_eq!(phase(&app), RunPhase::Scoring);

        let apres = app.world().resource::<BlindContext>();
        assert_eq!(apres.current_score, avant.current_score, "score commis");
        assert_eq!(
            apres.hands_remaining, avant.hands_remaining,
            "main décomptée"
        );
        assert_eq!(apres.used_hands, avant.used_hands, "case consommée");

        let file = app.world().resource::<ScoringStepQueue>();
        assert!(
            !file.steps.is_empty(),
            "la file est vide : rien n'a été calculé"
        );
        assert!(
            !file.committed,
            "le garde-fou est né fermé : la file n'a pas été construite"
        );
        assert!(file.final_score > 0, "le total n'a pas été transporté");
        assert_eq!(file.hand, YahtzeeHand::FullHouse, "figure retenue");
    }

    #[test]
    fn test_queue_length_matches_report() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        soumettre(&mut app);

        // La file ne porte plus le rapport : la référence est **recalculée**,
        // ce qui la rend indépendante de la ressource testée. L'ancienne
        // version comparait deux champs de la même ressource et ne prouvait
        // que la cohérence interne du remplissage.
        let rapport = rapport_de_reference(&mut app);
        let file = app.world().resource::<ScoringStepQueue>();

        assert_eq!(
            file.steps.len(),
            rapport.steps.len(),
            "un palier a été perdu ou fusionné"
        );
        assert!(
            file.steps.iter().eq(rapport.steps.iter()),
            "les paliers ont été triés, dédupliqués ou filtrés"
        );
        assert_eq!(file.final_score, rapport.final_score, "total transporté");
        assert_eq!(
            file.final_score,
            file.steps.back().expect("au moins un palier").score_after,
            "le total diverge du dernier palier"
        );
    }

    #[test]
    fn test_queue_from_build_scoring_report_reads_back() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        soumettre(&mut app);
        let rapport = rapport_de_reference(&mut app);

        app.update();
        app.update();
        app.update();

        let file = app.world().resource::<ScoringStepQueue>();
        assert!(
            file.steps.iter().eq(rapport.steps.iter()),
            "paliers altérés"
        );
        assert_eq!(file.final_score, rapport.final_score);
        assert_eq!(file.hand, YahtzeeHand::FullHouse);
        assert!(!file.committed, "le garde-fou s'est fermé tout seul");
    }

    #[test]
    fn test_speed_multiplier_survives_a_second_submission() {
        // `new` et `Default` posent tous deux la vitesse à 1 : sans reprise
        // explicite, le joueur qui choisit x4 le reperd à la main suivante,
        // deux fois par manche et sans le moindre signal.
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        soumettre(&mut app);
        app.world_mut()
            .resource_mut::<ScoringStepQueue>()
            .set_speed_multiplier(4)
            .expect("vitesse admise");

        reentrer_dans_scoring(&mut app);

        assert_eq!(
            app.world().resource::<ScoringStepQueue>().speed_multiplier,
            4,
            "le réglage du joueur a été écrasé par la reconstruction"
        );
    }

    #[test]
    fn test_scoring_entry_is_deterministic() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        soumettre(&mut app);

        let premiers = paliers(&app);
        let total = app.world().resource::<ScoringStepQueue>().final_score;

        reentrer_dans_scoring(&mut app);

        assert_eq!(
            paliers(&app),
            premiers,
            "la file diffère d'une entrée à l'autre"
        );
        assert_eq!(
            app.world().resource::<ScoringStepQueue>().final_score,
            total
        );
    }

    #[test]
    fn test_queue_is_replaced_not_appended() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        soumettre(&mut app);
        let longueur = paliers(&app).len();
        assert!(longueur > 0);

        reentrer_dans_scoring(&mut app);

        assert_eq!(
            paliers(&app).len(),
            longueur,
            "la file s'est allongée : le rapport a été ajouté au lieu de remplacer"
        );
    }

    #[test]
    fn test_report_is_built_once_per_entry() {
        // « Une fois par entrée dans la phase, jamais en `Update` ». Pour le
        // voir, il faut faire ce que fera l'Étape 4 : dépiler. Un système posé
        // en `Update` remplirait la file à nouveau à la frame suivante, et le
        // décompte n'avancerait jamais.
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        soumettre(&mut app);
        assert!(!paliers(&app).is_empty());

        app.world_mut()
            .resource_mut::<ScoringStepQueue>()
            .steps
            .clear();
        app.update();

        assert!(
            paliers(&app).is_empty(),
            "la file s'est remplie à nouveau : le rapport est reconstruit à chaque frame"
        );
    }

    #[test]
    fn test_scoring_does_not_leave_the_phase() {
        // La transition vers la fin de manche a lieu quand la file est vide,
        // à l'Étape 4. Posée ici, elle sauterait tout le décompte.
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        soumettre(&mut app);

        app.update();
        app.update();

        assert_eq!(phase(&app), RunPhase::Scoring, "la phase a été quittée");
    }

    #[test]
    fn test_report_uses_session_hand_levels() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        soumettre(&mut app);
        let au_niveau_un = app.world().resource::<ScoringStepQueue>().final_score;

        {
            let mut session = app.world_mut().resource_mut::<RunSession>();
            session.hand_levels.upgrade(YahtzeeHand::FullHouse);
            session.hand_levels.upgrade(YahtzeeHand::FullHouse);
        }
        reentrer_dans_scoring(&mut app);

        let au_niveau_trois = app.world().resource::<ScoringStepQueue>().final_score;
        assert!(
            au_niveau_trois > au_niveau_un,
            "les niveaux de la session ne sont pas appliqués : {au_niveau_un} puis {au_niveau_trois}"
        );
    }

    #[test]
    fn test_stale_report_is_cleared_on_a_barren_entry() {
        // Un rapport calculé, puis une entrée dans la phase qui ne calcule
        // rien : sans remise à zéro, l'Étape 4 dépilerait et commettrait deux
        // fois le rapport précédent.
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        soumettre(&mut app);
        assert!(!paliers(&app).is_empty());

        // Retour au lancer : `setup_round` remet la figure choisie à zéro.
        entrer_dans_roll(&mut app);
        assert_eq!(figure_selectionnee(&app), None);

        reentrer_dans_scoring(&mut app);

        let file = app.world().resource::<ScoringStepQueue>();
        assert!(file.steps.is_empty(), "un rapport périmé a survécu");
        assert_eq!(
            *file,
            ScoringStepQueue::default(),
            "un résidu a survécu à l'entrée stérile"
        );
    }

    #[test]
    fn test_relic_states_unchanged_by_scoring() {
        // **Tautologique aujourd'hui, et il faut le dire.** `RelicId` porte ses
        // variantes sous `cfg(test)` de `core_engine` (TASK-21) : hors de ce
        // build l'enum est inhabité, donc aucun `RelicInstance` ne peut être
        // construit d'ici, et l'inventaire de test n'est qu'une suite de
        // `None`. Ce qui garantit réellement la propriété est la **signature** :
        // l'inventaire entre en `Res`, jamais en `ResMut`, et faire avancer un
        // état de relique ne compile pas.
        //
        // L'assertion ci-dessous est un fil de détente : le jour où TASK-17
        // peuplera le catalogue et où un montage pourra porter une vraie
        // relique, elle tombera et forcera à muscler ce test.
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        let avant = app.world().resource::<RelicInventory>().clone();
        assert!(
            avant.slots.iter().all(Option::is_none),
            "un RelicInstance est constructible : ce test peut redevenir un vrai test"
        );

        soumettre(&mut app);

        assert_eq!(*app.world().resource::<RelicInventory>(), avant);
    }

    #[test]
    fn test_scoring_without_selection_is_inert() {
        // Mesuré : on entre dans la phase de comptage sans passer par la
        // soumission — un test livré à TASK-35 le fait déjà. La figure choisie
        // vaut alors `None`, et un `expect` y paniquerait.
        let mut app = app_prete(None);
        reentrer_dans_scoring(&mut app);

        assert_eq!(phase(&app), RunPhase::Scoring);
        let file = app.world().resource::<ScoringStepQueue>();
        assert!(file.steps.is_empty());
        assert_eq!(*file, ScoringStepQueue::default());
    }

    #[test]
    fn test_unrealised_figure_produces_no_report() {
        // Une case choisie mais non réalisée n'a aucun `HandMatch`, et
        // `resolve` en exige un. Rien n'est synthétisé : la case sera
        // consommée pour zéro, faute d'un chiffre que le corpus n'a pas donné.
        let mut app = app_prete(None);
        let realisees: Vec<YahtzeeHand> = app
            .world()
            .resource::<HandContext>()
            .active_evaluations
            .iter()
            .map(|m| m.hand)
            .collect();
        let absente = YahtzeeHand::ALL
            .into_iter()
            .find(|figure| !realisees.contains(figure))
            .expect("une figure au moins n'est pas réalisée");
        choisir(&mut app, absente);

        soumettre(&mut app);

        assert_eq!(phase(&app), RunPhase::Scoring);
        let file = app.world().resource::<ScoringStepQueue>();
        assert!(file.steps.is_empty());
        assert_eq!(*file, ScoringStepQueue::default());
    }

    #[test]
    fn test_used_hand_is_refused() {
        let mut app = app_prete(None);
        // La case est choisie **avant** d'être consommée : sans cela,
        // `select_hand` refuserait déjà et le refus de la soumission ne serait
        // jamais exercé.
        choisir(&mut app, YahtzeeHand::FullHouse);
        consommer(&mut app, YahtzeeHand::FullHouse);

        frapper(&mut app, KeyCode::Enter);
        app.update();
        app.update();

        assert_eq!(
            phase(&app),
            RunPhase::Roll,
            "l'état a bougé malgré le refus"
        );
        assert!(des_marques(&mut app).is_empty(), "un dé a été marqué");
    }

    #[test]
    fn test_submit_without_selection_is_refused() {
        let mut app = app_prete(None);
        assert_eq!(figure_selectionnee(&app), None);
        let grille_avant = app.world().resource::<BlindContext>().used_hands;

        frapper(&mut app, KeyCode::Enter);
        app.update();
        app.update();

        assert_eq!(phase(&app), RunPhase::Roll);
        assert!(des_marques(&mut app).is_empty());
        assert_eq!(
            app.world().resource::<BlindContext>().used_hands,
            grille_avant
        );
    }

    #[test]
    fn test_submit_marks_exactly_scoring_dice() {
        // **Le brelan, et non le Full.** Sur `[5,5,5,2,2]`, le Full retient les
        // cinq dés : « marquer tous les dés » y est indistinguable de
        // « marquer exactement les bons », et le banc l'a montré. Le brelan
        // n'en retient que trois, ce qui rend le « et sur aucun autre »
        // vérifiable.
        let mut app = app_prete(Some(YahtzeeHand::ThreeOfAKind));

        let attendus = {
            let main = app.world().resource::<HandContext>();
            let trouvee = main
                .active_evaluations
                .iter()
                .find(|m| m.hand == YahtzeeHand::ThreeOfAKind)
                .expect("le brelan est réalisé par [5,5,5,2,2]");
            let mut ids = trouvee.scoring_dice.clone();
            ids.sort_unstable();
            ids
        };
        let total = des_tries(&mut app).len();
        assert!(
            !attendus.is_empty() && attendus.len() < total,
            "le sous-ensemble doit être strict : {} retenus sur {total}",
            attendus.len()
        );

        frapper(&mut app, KeyCode::Enter);
        app.update();

        assert_eq!(des_marques(&mut app), attendus);
    }

    #[test]
    fn test_submit_removes_no_die() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));

        let entites = entites_des(&mut app);
        for entite in entites.iter().take(2) {
            app.world_mut().entity_mut(*entite).insert(Locked);
        }

        frapper(&mut app, KeyCode::Enter);
        app.update();

        assert_eq!(entites_des(&mut app), entites, "des dés ont bougé");
        for entite in entites.iter().take(2) {
            assert!(
                app.world().get::<Locked>(*entite).is_some(),
                "un verrou a été retiré à la soumission"
            );
        }
    }

    #[test]
    fn test_submit_does_not_mark_used_hands() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        let avant = app.world().resource::<BlindContext>().clone();

        frapper(&mut app, KeyCode::Enter);
        app.update();

        let apres = app.world().resource::<BlindContext>();
        assert_eq!(
            apres.used_hands, avant.used_hands,
            "la case a été consommée"
        );
        assert_eq!(apres.current_score, avant.current_score);
        assert_eq!(apres.hands_remaining, avant.hands_remaining);
    }

    #[test]
    fn test_select_hand_refuses_used_cell() {
        let mut app = app_prete(None);
        consommer(&mut app, YahtzeeHand::FullHouse);

        choisir(&mut app, YahtzeeHand::FullHouse);

        assert_eq!(figure_selectionnee(&app), None);
    }

    #[test]
    fn test_select_hand_accepts_an_available_cell() {
        // Contre-épreuve du test précédent : sans elle, une fonction qui
        // n'écrirait jamais rien passerait le refus.
        let mut app = app_prete(None);

        choisir(&mut app, YahtzeeHand::FullHouse);

        assert_eq!(figure_selectionnee(&app), Some(YahtzeeHand::FullHouse));
    }

    #[test]
    fn test_select_hand_allows_an_unrealised_figure() {
        // La sélection porte sur la **case**, pas sur une figure réalisée.
        // Sans cela, La Fissure, qui laisse les évaluations vides, rendrait la
        // grille injouable.
        let mut app = app_prete(None);
        let realisees: Vec<YahtzeeHand> = app
            .world()
            .resource::<HandContext>()
            .active_evaluations
            .iter()
            .map(|m| m.hand)
            .collect();
        let absente = YahtzeeHand::ALL
            .into_iter()
            .find(|figure| !realisees.contains(figure))
            .expect("une figure au moins n'est pas réalisée par [5,5,5,2,2]");

        choisir(&mut app, absente);

        assert_eq!(figure_selectionnee(&app), Some(absente));
    }

    #[test]
    fn test_submit_unrealised_figure_still_enters_scoring() {
        let mut app = app_prete(None);
        let realisees: Vec<YahtzeeHand> = app
            .world()
            .resource::<HandContext>()
            .active_evaluations
            .iter()
            .map(|m| m.hand)
            .collect();
        let absente = YahtzeeHand::ALL
            .into_iter()
            .find(|figure| !realisees.contains(figure))
            .expect("une figure au moins n'est pas réalisée");
        choisir(&mut app, absente);

        frapper(&mut app, KeyCode::Enter);
        app.update();
        app.update();

        // La case est consommée pour un score faible : c'est le risque que
        // vend La Fissure, pas un refus.
        assert_eq!(phase(&app), RunPhase::Scoring);
        assert!(des_marques(&mut app).is_empty(), "un dé a été marqué");
    }

    #[test]
    fn test_valid_submit_enters_scoring() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        assert_eq!(phase(&app), RunPhase::Roll);

        frapper(&mut app, KeyCode::Enter);
        app.update();
        app.update();

        assert_eq!(phase(&app), RunPhase::Scoring);
    }

    #[test]
    fn test_submission_is_inert_when_overlay_open() {
        let mut app = app_prete(Some(YahtzeeHand::FullHouse));
        app.world_mut().resource_mut::<SettingsOverlay>().open = true;

        frapper(&mut app, KeyCode::Enter);
        app.update();
        app.update();

        assert_eq!(phase(&app), RunPhase::Roll);
        assert!(des_marques(&mut app).is_empty());
    }
}
