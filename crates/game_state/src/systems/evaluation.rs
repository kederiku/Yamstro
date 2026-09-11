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

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use core_engine::blinds::{BlindContext, BlindModifier};
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

/// La face que *Le Borgne* éteint, `None` sous toute autre manche.
///
/// **Unique lecture de `BlindModifier::DisableFace` du projet.** Un second
/// lecteur qui divergerait d'une face produirait deux mains pour le même
/// lancer, et l'écart ne se verrait qu'au score.
pub fn disabled_face(blind: &BlindContext) -> Option<u8> {
    match blind.blind.modifier {
        Some(BlindModifier::DisableFace(face)) => Some(face),
        _ => None,
    }
}

/// La tranche transmise à `HandEvaluator::evaluate` sous la manche courante.
///
/// # Le filtrage a lieu ici, et nulle part ailleurs
///
/// La signature d'`evaluate` est normative au caractère près depuis TASK-10 :
/// la règle « ni Chips, ni figure » s'obtient donc **par construction**, en ne
/// transmettant pas les dés éteints, plutôt que par deux branches à tenir
/// d'accord — une dans l'évaluateur, une dans le pipeline.
///
/// Les dés restent dans le `DicePool` : affichés, verrouillables, relançables.
/// Seul l'affichage les grisera (Étape 7).
///
/// # Ce que le filtrage coûte, mesuré
///
/// Sur `1-1-3-3-3` sous `DisableFace(1)`, l'évaluateur ne voit que `3-3-3` :
/// le **Full House disparaît**, sa paire étant faite de 1 ; la case des **As**
/// disparaît avec lui, `all_ids_of_face` ne trouvant plus rien ; **Chance perd
/// deux Chips**. Un Yams devient impossible dès qu'un 1 y serait nécessaire, et
/// la Grande Suite avec. Ce sont les effets voulus, pas des effets de bord.
///
/// Une main entièrement éteinte transmet une tranche **vide**, ce qui n'est pas
/// une erreur : `evaluate` rend alors **une** figure, Chance, à sa base nue et
/// sans un seul dé retenu. Chiffrer ce plancher est un arbitrage d'Étape 6 bis.
///
/// # Rendu, jamais rangé
///
/// Le `Vec` est reconstruit à chaque appel. Un champ de ressource portant la
/// main filtrée serait exactement le second état parallèle que TASK-35
/// proscrit. `Die` n'étant pas `Copy` (TASK-02), le filtre clone.
pub fn evaluable_dice(dice: &[Die], blind: &BlindContext) -> Vec<Die> {
    let Some(face) = disabled_face(blind) else {
        return dice.to_vec();
    };

    dice.iter()
        .filter(|die| die.current_value != face)
        .cloned()
        .collect()
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

/// Les dés masqués. La borne `With<Die>` évite qu'un `Hidden` posé sur une
/// entité d'affichage aveugle la main.
type HiddenDice<'w, 's> = Query<'w, 's, (), (With<Hidden>, With<Die>)>;

/// Les poses : valeur changée, verrou posé, masque posé.
type Touched<'w, 's> = Query<'w, 's, (), Or<(Changed<Die>, Added<Locked>, Added<Hidden>)>>;

/// Ce qui a bougé depuis le dernier passage : les poses d'un côté, les retraits
/// de l'autre. `Hidden` y figure au même titre que `Locked` : voir le `//!`.
///
/// **Groupés en `SystemParam`, comme `DispatchParams` l'est à TASK-49.** La
/// lecture du contexte de manche portait la signature à huit paramètres nus,
/// que clippy refuse au delà de sept, et `-D warnings` est dans la définition
/// de terminé. Le regroupement n'est pas qu'arithmétique : ces trois accès
/// répondent à une seule question — qu'est-ce qui a changé — et les nommer
/// ensemble empêche qu'on en ajoute un quatrième sans le voir.
///
/// L'alias de `poses` n'est pas décoratif : `clippy::type_complexity` refuse la
/// requête écrite en clair dans le champ.
#[derive(SystemParam)]
struct Mouvements<'w, 's> {
    poses: Touched<'w, 's>,
    deverrouilles: RemovedComponents<'w, 's, Locked>,
    demasques: RemovedComponents<'w, 's, Hidden>,
}

impl Mouvements<'_, '_> {
    /// Vrai si rien n'a bougé. **Lit les retraits dans tous les cas**, et en
    /// premier : un `RemovedComponents` non lu conserve ses entrées pour le
    /// passage suivant, ce qui provoquerait un recalcul fantôme une fois la run
    /// montée.
    fn rien(&mut self) -> bool {
        let retraits = self.deverrouilles.read().count() + self.demasques.read().count();
        self.poses.is_empty() && retraits == 0
    }
}

/// `Update`, sous `in_state(RunPhase::Roll)` et dans `GameSet::EvaluatingBoard`,
/// donc après les entrées.
fn update_hand_evaluations(
    session: Option<Res<RunSession>>,
    blind: Option<Res<BlindContext>>,
    hand: Option<ResMut<HandContext>>,
    dice: Query<&Die>,
    hidden: HiddenDice,
    mut mouvements: Mouvements,
) {
    // Avant toute sortie : les retraits se lisent à chaque passage, run montée
    // ou non.
    let immobile = mouvements.rien();

    let (Some(session), Some(mut hand)) = (session, hand) else {
        return;
    };

    // Piloté par le changement, jamais par l'horloge.
    if immobile {
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

    // Sous *Le Borgne*, l'évaluateur ne voit pas les dés éteints. Hors manche
    // le contexte est absent du monde, et la main passe entière.
    let roll = match blind {
        Some(blind) => evaluable_dice(&roll, &blind),
        None => roll,
    };

    let mut found = HandEvaluator::evaluate(&roll);
    // Après la détection, avant le remplissage. `evaluate` chiffre tout au
    // niveau 1 : sans ce passage, la surbrillance classe faux dès le premier
    // parchemin de grille. La liste sort retriée ; ne pas la retrier.
    //
    // **La même tranche** qu'`evaluate` : le chiffrage résout par identifiant
    // et ne somme que `scoring_dice`, donc le lancer complet donnerait le même
    // nombre — mais deux notions de « la main » circuleraient ici.
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

    // ---- TASK-72 : *Le Borgne* ----

    use core_engine::blinds::{BlindContext, BlindDefinition, BlindModifier};

    /// Manche inerte, ou porteuse d'un boss. Le constructeur de test de
    /// `core_engine` est `#[cfg(test)]`, donc invisible d'ici : le contexte se
    /// construit par littéral, comme dans `resources.rs`.
    fn manche(modifier: Option<BlindModifier>) -> BlindContext {
        BlindContext {
            blind: BlindDefinition {
                modifier,
                ..BlindDefinition::default()
            },
            target_score: 300,
            current_score: 0,
            hands_remaining: 4,
            used_hands: HandGrid::default(),
        }
    }

    /// Les figures d'une main vue sous une manche, dans l'ordre d'affichage.
    fn figures_sous(des: &[Die], manche: &BlindContext) -> Vec<(YahtzeeHand, u64)> {
        let evaluables = evaluable_dice(des, manche);
        let mut trouvees = HandEvaluator::evaluate(&evaluables);
        rescore_with_levels(&mut trouvees, &evaluables, &HandLevels::default());
        trouvees
            .iter()
            .map(|trouvee| (trouvee.hand, trouvee.potential_score))
            .collect()
    }

    #[test]
    fn test_borgne_excludes_ones() {
        let des = main_de(&[1, 1, 3, 3, 3]);

        // Sans boss, la paire de 1 fait le Full, et c'est de loin la meilleure
        // figure de la main.
        assert_eq!(
            figures_sous(&des, &manche(None)),
            vec![
                (YahtzeeHand::FullHouse, 164),
                (YahtzeeHand::Threes, 48),
                (YahtzeeHand::ThreeOfAKind, 38),
                (YahtzeeHand::Chance, 16),
                (YahtzeeHand::Aces, 7),
            ]
        );

        // Sous *Le Borgne*, l'évaluateur ne voit que `3-3-3`. Ce que le boss
        // coûte, nommément : le Full disparaît — sa paire était faite de 1 —,
        // la case des As avec lui, et Chance perd les deux Chips des 1. Le
        // Brelan et les Trois, qui ne devaient rien aux 1, ne bougent pas.
        assert_eq!(
            figures_sous(&des, &manche(Some(BlindModifier::DisableFace(1)))),
            vec![
                (YahtzeeHand::Threes, 48),
                (YahtzeeHand::ThreeOfAKind, 38),
                (YahtzeeHand::Chance, 14),
            ]
        );
    }

    #[test]
    fn test_borgne_dice_absent_from_both_lists() {
        let des = main_de(&[1, 1, 3, 3, 3]);
        let boss = manche(Some(BlindModifier::DisableFace(1)));
        let interdits = [DieId(0), DieId(1)];

        let trouvees = HandEvaluator::evaluate(&evaluable_dice(&des, &boss));
        assert!(!trouvees.is_empty(), "aucune figure à inspecter");

        for trouvee in &trouvees {
            for interdit in interdits {
                assert!(
                    !trouvee.scoring_dice.contains(&interdit),
                    "{:?} compte le dé {interdit:?}",
                    trouvee.hand
                );
                // L'écart avec la v1 : un dé filtré n'est pas « écarté », il
                // est **absent**. Sans quoi l'étape 2 du pipeline le verrait.
                assert!(
                    !trouvee.discarded_dice.contains(&interdit),
                    "{:?} écarte le dé {interdit:?} au lieu de l'ignorer",
                    trouvee.hand
                );
            }
        }
    }

    #[test]
    fn test_no_modifier_leaves_hand_untouched() {
        let des = main_de(&[1, 1, 3, 3, 3]);
        let inerte = manche(None);

        assert_eq!(evaluable_dice(&des, &inerte), des, "la main a été touchée");
        assert_eq!(disabled_face(&inerte), None);
        // Non-régression sur TASK-35 : même liste qu'avant l'Étape 6.
        assert_eq!(
            figures_sous(&des, &inerte),
            figures(&des, &HandLevels::default())
                .iter()
                .map(|trouvee| (trouvee.hand, trouvee.potential_score))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_disable_face_six_behaves_identically() {
        let des = main_de(&[6, 6, 2, 2, 2]);
        let boss = manche(Some(BlindModifier::DisableFace(6)));

        assert_eq!(
            disabled_face(&boss),
            Some(6),
            "le boss n'est pas câblé sur 1"
        );
        assert_eq!(
            figures_sous(&des, &boss),
            vec![
                // Brelan et Deux sont à égalité de Chips — base 10 pour l'un
                // comme pour l'autre — et le mult ×2 du Brelan les départage.
                (YahtzeeHand::ThreeOfAKind, 32),
                (YahtzeeHand::Twos, 16),
                (YahtzeeHand::Chance, 11),
            ]
        );
    }

    #[test]
    fn test_chance_still_playable_under_borgne() {
        let des = main_de(&[1, 1, 3, 3, 3]);
        let boss = manche(Some(BlindModifier::DisableFace(1)));

        let sous_boss = figures_sous(&des, &boss);
        assert!(
            sous_boss
                .iter()
                .any(|(figure, _)| *figure == YahtzeeHand::Chance),
            "la face désactivée a verrouillé Chance"
        );
        // La case reste jouable **et coûte quelque chose** : sans cette
        // seconde assertion, le test passerait sur un filtre inerte.
        assert_eq!(
            sous_boss
                .iter()
                .find(|(figure, _)| *figure == YahtzeeHand::Chance)
                .map(|(_, score)| *score),
            Some(14),
            "Chance doit perdre les deux Chips des 1"
        );
    }

    #[test]
    fn test_all_dice_filtered_is_not_a_panic() {
        let des = main_de(&[1, 1, 1, 1, 1]);
        let boss = manche(Some(BlindModifier::DisableFace(1)));

        let evaluables = evaluable_dice(&des, &boss);
        assert!(evaluables.is_empty(), "la tranche devait être vide");

        // **La tranche vide ne rend pas une liste vide.** `evaluate` empile
        // Chance hors de toute garde : le joueur peut la soumettre pour sa base
        // nue, sans un seul dé retenu. C'est la suite du filtrage amont, pas un
        // défaut ; chiffrer ce plancher est un arbitrage d'Étape 6 bis.
        let trouvees = HandEvaluator::evaluate(&evaluables);
        assert_eq!(trouvees.len(), 1);
        assert_eq!(trouvees[0].hand, YahtzeeHand::Chance);
        assert_eq!(trouvees[0].potential_score, 5);
        assert!(trouvees[0].scoring_dice.is_empty());
        assert!(trouvees[0].discarded_dice.is_empty());
    }

    #[test]
    fn test_other_modifiers_disable_no_face() {
        // **Le trou que la fixture masquait.** Avec `None` et `DisableFace` pour
        // seuls cas, un bras `Some(_) => Some(1)` passait tous les tests : les
        // quatorze autres boss auraient éteint les 1 en silence. La Cage, elle,
        // porte même un `u8` — rien n'empêche de le prendre pour une face.
        let des = main_de(&[1, 1, 3, 3, 3]);
        let temoin = figures_sous(&des, &manche(None));

        for autre in [
            BlindModifier::MaxRerolls(1),
            BlindModifier::DisableRelicSlot(1),
            BlindModifier::HideDice(2),
            BlindModifier::HalveBaseScores,
        ] {
            let manche = manche(Some(autre.clone()));
            assert_eq!(disabled_face(&manche), None, "{autre:?} éteint une face");
            assert_eq!(
                evaluable_dice(&des, &manche),
                des,
                "{autre:?} filtre la main"
            );
            assert_eq!(figures_sous(&des, &manche), temoin, "{autre:?}");
        }
    }

    #[test]
    fn test_aces_cell_is_unreachable_under_borgne() {
        let des = main_de(&[1, 1, 3, 3, 3]);

        assert!(
            figures(&des, &HandLevels::default())
                .iter()
                .any(|trouvee| trouvee.hand == YahtzeeHand::Aces),
            "sans boss, la case des As est atteignable"
        );
        // Conséquence assumée : *Le Borgne* supprime une case de la grille.
        // Nommée ici pour qu'elle soit au registre de l'Étape 6 bis, et non
        // découverte à l'équilibrage.
        assert!(
            !figures_sous(&des, &manche(Some(BlindModifier::DisableFace(1))))
                .iter()
                .any(|(figure, _)| *figure == YahtzeeHand::Aces),
            "la case des As reste atteignable sous Le Borgne"
        );
    }

    #[test]
    fn test_missing_blind_context_leaves_hand_untouched() {
        // Le contexte est `Option` parce qu'il est réellement absent du monde
        // au premier `OnEnter(BlindSelect)` d'une application sans session
        // (voir le `//!` de `setup.rs`). Le bras `None` doit passer la main
        // **entière** : sans cette épreuve, y filtrer la face 1 en dur passait
        // au vert, la fixture posant toujours le contexte.
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        app.world_mut().remove_resource::<BlindContext>();
        poser_les_valeurs(&mut app, &[1, 1, 3, 3, 3]);
        app.update();

        assert_eq!(score_de(&app, YahtzeeHand::FullHouse), 164);
        assert_eq!(score_de(&app, YahtzeeHand::Aces), 7);
    }

    #[test]
    fn test_removals_are_read_before_the_early_return() {
        // TASK-35 documentait cet invariant sans le garder : un
        // `RemovedComponents` non lu conserve ses entrées pour le passage
        // suivant, et la sortie anticipée sur ressources absentes les
        // laisserait en file. Le regroupement en `Mouvements` a réécrit ce
        // chemin ; l'invariant devient donc testable, et testé.
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        let session = app.world().resource::<RunSession>().clone();
        app.world_mut().remove_resource::<RunSession>();

        // Un verrou posé puis retiré pendant que la run est démontée.
        let entite = entites_des(&mut app)[0];
        app.world_mut().entity_mut(entite).insert(Locked);
        app.update();
        app.world_mut().entity_mut(entite).remove::<Locked>();
        app.update();

        // La run réapparaît, et rien d'autre ne bouge.
        app.world_mut().insert_resource(session);
        vider_les_evaluations(&mut app);
        app.update();

        assert!(
            evaluations_vides(&app),
            "un retrait périmé a provoqué un recalcul fantôme"
        );
    }

    #[test]
    fn test_rescore_reads_the_slice_by_id() {
        // Le chiffrage résout par identifiant et ne somme que `scoring_dice` :
        // rechiffrer sur la tranche filtrée ou sur le lancer complet donne le
        // **même** nombre. C'est mesuré, pas supposé — ce qui rend le choix de
        // la tranche filtrée une décision de lisibilité, et non de justesse.
        let des = main_de(&[1, 1, 3, 3, 3]);
        let boss = manche(Some(BlindModifier::DisableFace(1)));
        let evaluables = evaluable_dice(&des, &boss);

        let mut sur_filtre = HandEvaluator::evaluate(&evaluables);
        rescore_with_levels(&mut sur_filtre, &evaluables, &HandLevels::default());

        let mut sur_complet = HandEvaluator::evaluate(&evaluables);
        rescore_with_levels(&mut sur_complet, &des, &HandLevels::default());

        assert_eq!(sur_filtre, sur_complet);
        assert!(!sur_filtre.is_empty(), "rien à comparer");
    }

    #[test]
    fn test_system_applies_the_blind_filter() {
        // Le helper seul ne prouve pas que le système est branché : sans cette
        // épreuve de bout en bout, un filtrage jamais appelé passerait au vert.
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        poser_les_valeurs(&mut app, &[1, 1, 3, 3, 3]);
        app.update();
        assert_eq!(score_de(&app, YahtzeeHand::FullHouse), 164);

        app.world_mut()
            .resource_mut::<BlindContext>()
            .blind
            .modifier = Some(BlindModifier::DisableFace(1));
        poser_les_valeurs(&mut app, &[1, 1, 3, 3, 3]);
        app.update();

        assert_eq!(
            score_de(&app, YahtzeeHand::FullHouse),
            0,
            "le Full a survécu"
        );
        assert_eq!(score_de(&app, YahtzeeHand::Chance), 14);
        assert_eq!(score_de(&app, YahtzeeHand::ThreeOfAKind), 38);
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
