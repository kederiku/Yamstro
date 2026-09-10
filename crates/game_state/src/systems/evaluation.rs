//! Recalcul des figures disponibles, et disponibilité des cases.
//!
//! # Aucune sélection automatique
//!
//! Ce module remplit la liste des figures trouvées et **ne choisit rien**. La
//! v1 retenait « par défaut la figure au score potentiel le plus fort », ce qui
//! réduisait la boucle à « relancer, relancer, jouer » et supprimait la seule
//! décision du joueur (C08, ADR-001). Le champ du contexte de main qui porte
//! ce choix n'est écrit nulle part ici, et le nom même n'apparaît que dans les
//! assertions du test qui le vérifie.
//!
//! # La surbrillance est dérivée, jamais stockée
//!
//! `is_hand_available` est le prédicat unique : la surbrillance calculée ici et
//! le refus de `submit_hand` (TASK-36) le partagent, et le grisage des cases
//! (Étapes 7 et 11) le partagera. Stocker un drapeau « grisé » ou une figure
//! « surlignée » recréerait le second état parallèle que le corpus interdit ;
//! `HandContext` garde ses trois champs canoniques.
//!
//! **Il n'y a donc qu'un seul système enregistré ici, contre les deux
//! annoncés.** Le second, `lock_used_hands_ui`, devait griser et rendre non
//! cliquables les cases consommées — or aucun composant de case n'existe, et le
//! ticket interdit d'en inventer. Un système enregistré qui n'écrit nulle part
//! donnerait l'illusion que le grisage est branché. Le livrable réel est le
//! prédicat ; l'Étape 7 posera le système d'affichage qui le consomme.
//!
//! # Ce qui déclenche un recalcul, et ce qui n'en déclenche pas
//!
//! Mesuré :
//!
//! | geste | `Changed<Die>` | `Added<Locked>` | retrait |
//! | :-- | :-- | :-- | :-- |
//! | lancer, relance | oui | non | non |
//! | frappe de verrouillage (TASK-34) | oui, **la frame même** | oui, **la suivante** | non |
//! | marqueur posé seul | non | oui | non |
//! | marqueur retiré seul | non | non | oui |
//!
//! Les déclencheurs sur les marqueurs sont donc **redondants aujourd'hui** :
//! TASK-34 impose que le marqueur et `Die.locked` bougent ensemble, et le champ
//! suffit. Ils restent en place comme assurance contre un effet de l'Étape 9
//! qui casserait cet invariant et rendrait la surbrillance muette. Verrouiller
//! ne change d'ailleurs aucun résultat : l'évaluation est une fonction pure des
//! valeurs courantes.
//!
//! `Hidden` entre dans la porte pour une raison qui, elle, n'est pas
//! défensive : sans lui, *La Fissure* masquerait un dé en cours de main, la
//! porte resterait fermée, et la liste garderait les scores potentiels calculés
//! **avant** le masquage — la couche d'affichage montrerait exactement ce que
//! le boss est censé cacher. La fuite existe dans l'autre sens : marqueur
//! retiré, porte fermée, main aveugle jusqu'au prochain lancer.

use bevy::prelude::*;
use core_engine::dice::Die;
use core_engine::evaluator::{HandEvaluator, HandMatch, rescore_with_levels};
use core_engine::hands::{HandGrid, YahtzeeHand};

use crate::components::{Hidden, Locked};
use crate::plugin::GameSet;
use crate::resources::{HandContext, RunSession};
use crate::states::RunPhase;

/// Vrai tant que la figure n'a pas été consommée dans la blind.
///
/// **Prédicat unique.** `used_hands` est le seul bitset des figures
/// consommées ; une seconde liste divergerait au premier oubli et ferait mentir
/// la grille.
pub fn is_hand_available(used: &HandGrid, hand: YahtzeeHand) -> bool {
    !used.contains(hand)
}

/// La meilleure figure encore disponible : la première entrée de la liste,
/// **déjà triée** par `rescore_with_levels`, qui n'a pas été consommée.
///
/// Dérivée à chaque appel, jamais mémorisée. Rend `None` quand la main ne
/// produit aucune figure disponible — ce qui, avec quatre mains pour treize
/// cases, ne peut pas arriver en jeu.
pub fn best_available_hand(evals: &[HandMatch], used: &HandGrid) -> Option<YahtzeeHand> {
    evals
        .iter()
        .map(|found| found.hand)
        .find(|hand| is_hand_available(used, *hand))
}

/// Ce qui a bougé depuis le dernier passage. `Hidden` y figure au même titre
/// que `Locked` : voir le `//!`.
type Touched<'w, 's> = Query<'w, 's, (), Or<(Changed<Die>, Added<Locked>, Added<Hidden>)>>;

/// Les dés masqués. La borne `With<Die>` évite qu'un `Hidden` posé sur une
/// entité d'affichage aveugle la main.
type HiddenDice<'w, 's> = Query<'w, 's, (), (With<Hidden>, With<Die>)>;

/// `Update`, sous `in_state(RunPhase::Roll)` et dans `GameSet::EvaluatingBoard`,
/// donc après les entrées.
fn update_hand_evaluations(
    session: Option<Res<RunSession>>,
    hand: Option<ResMut<HandContext>>,
    dice: Query<&Die>,
    hidden: HiddenDice,
    touched: Touched,
    mut unlocked: RemovedComponents<Locked>,
    mut unhidden: RemovedComponents<Hidden>,
) {
    // Les retraits se lisent **toujours**, et en premier : un
    // `RemovedComponents` non lu conserve ses entrées pour le passage suivant,
    // ce qui provoquerait un recalcul fantôme une fois la run montée.
    let removals = unlocked.read().count() + unhidden.read().count();

    let (Some(session), Some(mut hand)) = (session, hand) else {
        return;
    };

    // Piloté par le changement, jamais par l'horloge.
    if touched.is_empty() && removals == 0 {
        return;
    }

    // Aucune divulgation : tant qu'un dé est masqué, la liste reste vide.
    // Pas d'évaluation partielle, qui divulguerait l'information cachée (C23).
    if !hidden.is_empty() {
        if !hand.active_evaluations.is_empty() {
            hand.active_evaluations.clear();
        }
        return;
    }

    // Trié par identifiant : `evaluate` retient les plus petits identifiants
    // d'une face, donc l'ordre d'itération de la requête changerait les dés
    // retenus par chaque figure.
    let mut roll: Vec<Die> = dice.iter().cloned().collect();
    roll.sort_unstable_by_key(|die| die.id);

    let mut found = HandEvaluator::evaluate(&roll);
    // Après la détection, avant le remplissage. `evaluate` chiffre tout au
    // niveau 1 : sans ce passage, la surbrillance classe faux dès le premier
    // parchemin de grille. La liste sort retriée ; ne pas la retrier.
    rescore_with_levels(&mut found, &roll, &session.hand_levels);

    hand.active_evaluations = found;
}

/// Branche le recalcul des figures.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        update_hand_evaluations
            .in_set(GameSet::EvaluatingBoard)
            .run_if(in_state(RunPhase::Roll)),
    );
}

#[cfg(test)]
mod tests {
    // `use super::*` apporte déjà le prélude de Bevy et les types de
    // `core_engine` importés par l'implémentation.
    use super::*;
    use core_engine::cups::CupId;
    use core_engine::dice::DieId;
    use core_engine::evaluator::rescore_with_levels;
    use core_engine::hands::HandLevels;

    use crate::components::Scoring;
    use crate::systems::fixtures::{
        app_a_la_graine, app_en_run, entites_des, entrer_dans_roll, frapper,
    };

    /// Main aux valeurs imposées, identifiants croissants.
    fn main_de(valeurs: &[u8]) -> Vec<Die> {
        valeurs
            .iter()
            .enumerate()
            .map(|(rang, valeur)| {
                let mut die = Die::new(DieId(rang as u32), 6);
                die.current_value = *valeur;
                die
            })
            .collect()
    }

    /// Les figures d'une main, chiffrées aux niveaux donnés.
    fn figures(des: &[Die], niveaux: &HandLevels) -> Vec<core_engine::evaluator::HandMatch> {
        let mut trouvees = HandEvaluator::evaluate(des);
        rescore_with_levels(&mut trouvees, des, niveaux);
        trouvees
    }

    /// Vide la liste : si le système se réexécute, il la remplit à nouveau.
    /// C'est une observation de **contenu**, là où un horodatage de changement
    /// ne dirait que « le système a écrit ».
    fn vider_les_evaluations(app: &mut App) {
        app.world_mut()
            .resource_mut::<HandContext>()
            .active_evaluations
            .clear();
    }

    /// Impose les valeurs des dés, dans l'ordre des `DieId`. Réécrire une
    /// valeur identique rouvre quand même la porte : c'est l'écriture qui
    /// marque le changement, pas la différence.
    fn poser_les_valeurs(app: &mut App, valeurs: &[u8]) {
        for (entite, valeur) in entites_des(app).into_iter().zip(valeurs) {
            app.world_mut()
                .get_mut::<Die>(entite)
                .expect("dé")
                .current_value = *valeur;
        }
    }

    fn score_de(app: &App, figure: YahtzeeHand) -> u64 {
        app.world()
            .resource::<HandContext>()
            .active_evaluations
            .iter()
            .find(|trouvee| trouvee.hand == figure)
            .map(|trouvee| trouvee.potential_score)
            .unwrap_or_default()
    }

    fn evaluations_vides(app: &App) -> bool {
        app.world()
            .resource::<HandContext>()
            .active_evaluations
            .is_empty()
    }

    #[test]
    fn test_no_auto_selection() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        let main = app.world().resource::<HandContext>();
        assert!(
            !main.active_evaluations.is_empty(),
            "aucune figure évaluée à l'entrée dans la main"
        );
        // La régression exacte que corrige l'ADR-001 : sélectionner la
        // meilleure figure réduirait la boucle à « relancer, relancer, jouer ».
        assert!(
            main.selected_hand.is_none(),
            "une figure a été choisie sans le joueur"
        );
    }

    #[test]
    fn test_used_hand_is_never_highlighted() {
        let des = main_de(&[5, 5, 5, 2, 2]);
        let trouvees = figures(&des, &HandLevels::default());

        let mut consommees = HandGrid::default();
        assert_eq!(
            best_available_hand(&trouvees, &consommees),
            Some(YahtzeeHand::FullHouse),
            "sans rien de consommé, le Full est la tête de liste"
        );

        consommees.mark(YahtzeeHand::FullHouse);
        let surbrillance = best_available_hand(&trouvees, &consommees);

        assert_ne!(surbrillance, Some(YahtzeeHand::FullHouse));
        assert!(surbrillance.is_some(), "plus aucune figure disponible");
        // La figure consommée reste **évaluée** : la retirer priverait
        // `submit_hand` du `HandMatch` et masquerait le refus au lieu de le
        // montrer.
        assert!(
            trouvees.iter().any(|f| f.hand == YahtzeeHand::FullHouse),
            "la figure consommée a disparu de la liste"
        );
    }

    #[test]
    fn test_highlight_follows_hand_levels() {
        let des = main_de(&[2, 2, 2, 3, 3, 4, 5, 6]);

        let au_niveau_un = figures(&des, &HandLevels::default());
        assert_eq!(
            best_available_hand(&au_niveau_un, &HandGrid::default()),
            Some(YahtzeeHand::LargeStraight)
        );
        assert_eq!(au_niveau_un[0].potential_score, 240);

        let mut niveaux = HandLevels::default();
        niveaux.upgrade(YahtzeeHand::FullHouse);
        niveaux.upgrade(YahtzeeHand::FullHouse);
        assert_eq!(niveaux.level(YahtzeeHand::FullHouse), 3);

        let au_niveau_trois = figures(&des, &niveaux);
        assert_eq!(
            best_available_hand(&au_niveau_trois, &HandGrid::default()),
            Some(YahtzeeHand::FullHouse)
        );
        assert_eq!(au_niveau_trois[0].potential_score, 432);
    }

    #[test]
    fn test_hidden_die_yields_no_evaluation() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        assert!(!evaluations_vides(&app), "rien n'était évalué au départ");

        // Masqué **en cours de main**, comme le ferait La Fissure. Posé avant
        // la première évaluation, le marqueur passerait par `Changed<Die>` et
        // ne prouverait rien du garde-fou.
        let entite = entites_des(&mut app)[0];
        app.world_mut().entity_mut(entite).insert(Hidden);
        app.update();

        assert!(
            evaluations_vides(&app),
            "un dé masqué n'a pas vidé les évaluations"
        );

        // Un `Hidden` posé sur une entité qui n'est **pas** un dé n'a rien à
        // voir avec la main : la borne `With<Die>` existe pour lui.
        app.world_mut().spawn(Hidden);
        app.update();
        assert!(evaluations_vides(&app), "le dé est toujours masqué");

        // Démasquage. Le seul signal est ici le **retrait** du marqueur : sans
        // sa lecture, la porte reste fermée et la main reste aveugle jusqu'au
        // prochain lancer. Et l'entité masquée qui subsiste ne doit rien
        // aveugler du tout.
        app.world_mut().entity_mut(entite).remove::<Hidden>();
        app.update();
        assert!(
            !evaluations_vides(&app),
            "les évaluations ne sont pas revenues après le démasquage"
        );
    }

    #[test]
    fn test_evaluations_use_session_hand_levels() {
        // Le test de surbrillance ci-dessus porte sur les fonctions pures :
        // il ne dit rien de ce que le **système** chiffre. Sans celui-ci, un
        // système qui oublierait `rescore_with_levels`, ou qui le lancerait sur
        // des niveaux par défaut, passerait inaperçu.
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        poser_les_valeurs(&mut app, &[5, 5, 5, 2, 2]);
        app.update();

        let avant = score_de(&app, YahtzeeHand::FullHouse);
        assert!(avant > 0, "le Full n'a pas été détecté");

        {
            let mut session = app.world_mut().resource_mut::<RunSession>();
            session.hand_levels.upgrade(YahtzeeHand::FullHouse);
            session.hand_levels.upgrade(YahtzeeHand::FullHouse);
        }
        poser_les_valeurs(&mut app, &[5, 5, 5, 2, 2]);
        app.update();

        let apres = score_de(&app, YahtzeeHand::FullHouse);
        assert!(
            apres > avant,
            "les niveaux de la session ne sont pas appliqués : {avant} puis {apres}"
        );
    }

    #[test]
    fn test_no_evaluation_outside_the_roll_phase() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        vider_les_evaluations(&mut app);

        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::Scoring);
        app.update();

        // Un dé touché hors de la phase de lancer ne recalcule rien : la main
        // est soumise, et son aperçu n'a plus à bouger.
        poser_les_valeurs(&mut app, &[3, 3, 3, 3, 3]);
        app.update();

        assert!(
            evaluations_vides(&app),
            "l'évaluation a tourné hors de la phase de lancer"
        );
    }

    #[test]
    fn test_evaluation_recomputes_on_lock_change() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        let entite = entites_des(&mut app)[0];
        vider_les_evaluations(&mut app);

        // Le marqueur **seul**, sans toucher `Die.locked` : mesuré, c'est le
        // seul cas où `Changed<Die>` reste à zéro, donc le seul que le
        // déclencheur `Added<Locked>` sauve. Une vraie frappe passerait par le
        // champ et ne prouverait rien.
        app.world_mut().entity_mut(entite).insert(Locked);
        app.update();

        assert!(
            !evaluations_vides(&app),
            "le verrouillage n'a pas déclenché le recalcul"
        );
    }

    #[test]
    fn test_no_recompute_on_a_calm_frame() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        vider_les_evaluations(&mut app);
        app.update();

        assert!(
            evaluations_vides(&app),
            "le recalcul suit l'horloge et non le changement"
        );
    }

    #[test]
    fn test_evaluations_are_deterministic() {
        let mut a = app_a_la_graine(CupId::Standard, 42);
        let mut b = app_a_la_graine(CupId::Standard, 42);
        entrer_dans_roll(&mut a);
        entrer_dans_roll(&mut b);

        // Divergence d'archétype dans b : `Scoring` ne change ni les valeurs
        // ni le filtre de relance, seulement l'ordre d'itération. Sans le tri
        // par `DieId` avant l'évaluation, les dés retenus par chaque figure
        // changeraient.
        for (rang, entite) in entites_des(&mut b).into_iter().enumerate() {
            if rang % 2 == 0 {
                b.world_mut().entity_mut(entite).insert(Scoring);
            }
        }

        frapper(&mut a, KeyCode::Space);
        frapper(&mut b, KeyCode::Space);
        a.update();
        b.update();

        let de_a = a
            .world()
            .resource::<HandContext>()
            .active_evaluations
            .clone();
        let de_b = b
            .world()
            .resource::<HandContext>()
            .active_evaluations
            .clone();

        assert!(!de_a.is_empty());
        assert_eq!(de_a, de_b);
    }

    #[test]
    fn test_used_hands_is_only_read() {
        // `is_hand_available` est le prédicat unique : la surbrillance d'ici et
        // le refus de TASK-36 le partagent, au lieu de tenir deux listes qui
        // divergeraient au premier oubli.
        let mut consommees = HandGrid::default();
        for figure in YahtzeeHand::ALL {
            assert!(is_hand_available(&consommees, figure), "{figure:?}");
        }

        consommees.mark(YahtzeeHand::Chance);
        assert!(!is_hand_available(&consommees, YahtzeeHand::Chance));
        for figure in YahtzeeHand::ALL {
            if figure != YahtzeeHand::Chance {
                assert!(is_hand_available(&consommees, figure), "{figure:?}");
            }
        }
    }
}
