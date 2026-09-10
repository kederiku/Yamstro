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
use core_engine::blind::BlindContext;
use core_engine::dice::Die;

use crate::components::Scoring;
use crate::plugin::InputSet;
use crate::resources::HandContext;
use crate::states::RunPhase;
use crate::systems::evaluation::is_hand_available;

/// `Update`, sous `in_state(RunPhase::Roll)`, dans l'ensemble gelé par
/// l'overlay. C'est un système d'**entrée** : ni `OnExit`, ni `OnEnter`.
fn submit_hand(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    blind: Option<Res<BlindContext>>,
    hand: Option<Res<HandContext>>,
    dice: Query<(Entity, &Die)>,
    mut next: ResMut<NextState<RunPhase>>,
) {
    if !keys.just_pressed(KeyCode::Enter) {
        return;
    }

    let (Some(blind), Some(hand)) = (blind, hand) else {
        return;
    };

    // Les deux refus, avant tout marqueur et avant toute transition.
    let Some(cell) = hand.selected_hand else {
        return;
    };
    if !is_hand_available(&blind.used_hands, cell) {
        return;
    }

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

/// Branche la soumission.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        submit_hand
            .in_set(InputSet::FrozenByOverlay)
            .run_if(in_state(RunPhase::Roll)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::blind::BlindContext;
    use core_engine::cups::CupId;
    use core_engine::dice::{Die, DieId};
    use core_engine::hands::YahtzeeHand;

    use crate::components::{Locked, Scoring};
    use crate::resources::HandContext;
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

    /// Passe par `select_hand`, jamais par une écriture directe : c'est le
    /// point d'entrée unique, et les tests le traitent comme le clic le fera.
    fn choisir(app: &mut App, figure: YahtzeeHand) {
        let grille = app.world().resource::<BlindContext>().used_hands;
        let mut main = app.world_mut().resource_mut::<HandContext>();
        select_hand(&mut main, &grille, figure);
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
