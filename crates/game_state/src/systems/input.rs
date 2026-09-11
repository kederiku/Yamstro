//! Entrées du joueur pendant la phase de lancer : verrouillage et relance.
//!
//! # Ce module dépend d'`InputPlugin`
//!
//! `handle_dice_input` lit `Res<ButtonInput<KeyCode>>`. Bevy n'offre aucun
//! moyen d'**exiger** un plugin, et une ressource absente fait paniquer la
//! construction des paramètres sur un message qui ne nomme ni le système ni la
//! ressource hors feature `debug`. La dépendance est donc énoncée ici et dans
//! le montage de test partagé, faute de pouvoir l'être dans le type.
//!
//! `GameStatePlugin` ne monte pas `InputPlugin` lui-même : un binaire qui
//! l'ajouterait avant `DefaultPlugins` le verrait monté deux fois, ce qui
//! panique. Même doctrine qu'à TASK-28 pour le gestionnaire d'erreur, le choix
//! appartient au binaire final.
//!
//! # Le gel par l'overlay vit dans son propre ensemble
//!
//! `InputSet::FrozenByOverlay` (déclaré dans `plugin.rs`) porte la condition,
//! et non `GameSet::HandlingInput` tout entier : `toggle_settings_overlay`
//! (TASK-39) **est** une entrée, et c'est celle qui ferme l'overlay. La geler
//! enfermerait le joueur dans un menu qu'aucune touche ne peut plus quitter.
//!
//! # Pourquoi deux filtres sur le verrouillage
//!
//! Le marqueur `Locked` est posé par une commande, appliquée en fin de
//! schedule. Verrouiller un dé et relancer dans la **même** frame laisse donc
//! la requête `Without<Locked>` voir ce dé comme libre ; c'est `Die.locked`,
//! posé immédiatement, que `Die::roll` consulte et qui le sauve. Les deux
//! filtres ne font pas double emploi, ils se couvrent.

use bevy::prelude::*;
use core_engine::blinds::BlindContext;
use core_engine::dice::{Die, DieId};

use core_engine::hands::YahtzeeHand;

use crate::components::{DieView, Locked};
use crate::plugin::{GameSet, InputSet};
use crate::resources::{HandContext, RunSession};
use crate::states::{RunPhase, SettingsOverlay};
use crate::systems::evaluation::is_hand_available;

/// Touches de rang, dans l'ordre. `KeyCode` est un enum sans arithmétique : la
/// correspondance rang vers touche doit bien être écrite quelque part, et ce
/// tableau en est le seul endroit.
///
/// **Aucune branche par touche.** La plage suit le nombre de dés présents, qui
/// vaut 4, 5 ou 6 selon le gobelet et peut décroître en cours de manche
/// (*La Meule*, Étape 9) ; neuf entrées couvrent tout ce que la main peut
/// atteindre, et un rang absent est un no-op silencieux.
const RANK_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// Tous les dés de la main, verrou compris : la requête de la bascule.
type AllDice<'w, 's> = Query<'w, 's, (Entity, &'static mut Die, &'static DieView, Has<Locked>)>;

/// Les dés libres, seuls concernés par la relance. Le filtre fait doublon avec
/// le `Die.locked` que `Die::roll` consulte — mesuré : le retirer ne change
/// aucun résultat — et le ticket le veut quand même, parce qu'il évite de
/// parcourir des dés qu'on ne relancera pas.
type FreeDice<'w, 's> = Query<'w, 's, (Entity, &'static mut Die), Without<Locked>>;

/// Bascule le verrou d'un dé.
///
/// **Point d'entrée unique.** Le clic, câblé aux Étapes 7 et 11, passera par
/// ici : il ne doit exister aucune seconde implémentation de la bascule. Le
/// marqueur et `Die.locked` se posent et se retirent ensemble, jamais
/// séparément.
///
/// Verrouiller est gratuit et illimité : rien ici ne touche aux relances.
pub(crate) fn toggle_lock(commands: &mut Commands, entity: Entity, die: &mut Die, locked: bool) {
    if locked {
        commands.entity(entity).remove::<Locked>();
        die.locked = false;
    } else {
        commands.entity(entity).insert(Locked);
        die.locked = true;
    }
}

/// Choisit une case de la grille.
///
/// **Point d'entrée unique de l'écriture de la figure choisie**, la remise à
/// zéro de `setup_round` mise à part. C'est l'invariant qui empêche
/// l'auto-sélection de revenir par une porte dérobée (ADR-001), et il se
/// vérifie mécaniquement.
///
/// Une fonction, et non un système : le clic sur une case est câblé aux Étapes
/// 7 et 11, et aucun composant de case n'existe encore. Un système enregistré
/// qui ne lit aucune entrée donnerait l'illusion que la sélection est branchée.
/// Le clic appellera ceci, sans le réécrire.
///
/// **La sélection porte sur la case, pas sur une figure réalisée.** Une case
/// dont la figure n'apparaît pas dans les évaluations reste choisissable :
/// sans cela, *La Fissure* (Étape 6), qui laisse la liste vide, rendrait la
/// grille injouable. Une case déjà consommée est un no-op silencieux.
///
/// Publique, comme `resolve_rerolls` (TASK-32) : aucun système ne l'appelle
/// avant que le clic existe, et une fonction privée sans appelant est du code
/// mort refusé sous `-D warnings`.
pub fn select_hand(hand: &mut HandContext, blind: &BlindContext, cell: YahtzeeHand) {
    if is_hand_available(blind, cell) {
        hand.selected_hand = Some(cell);
    }
}

/// `Update`, **sans aucune garde d'état** : le menu doit rester atteignable
/// depuis n'importe quel `AppState`.
///
/// **L'overlay n'est jamais un état.** En faire une variante obligerait à le
/// dupliquer dans chaque phase et à mémoriser un état de retour ; la ressource
/// supprime les deux. Aucun `NextState` n'est écrit ici, dans aucune branche.
///
/// Ce système est dans `GameSet::HandlingInput` — c'est une entrée, et son rang
/// d'ordonnancement parmi les autres compte — mais **hors** de
/// `InputSet::FrozenByOverlay` : geler la touche qui ferme l'overlay
/// enfermerait le joueur dans un menu qu'aucune touche ne peut plus quitter.
fn toggle_settings_overlay(keys: Res<ButtonInput<KeyCode>>, mut overlay: ResMut<SettingsOverlay>) {
    if keys.just_pressed(KeyCode::Escape) {
        overlay.open = !overlay.open;
    }
}

/// Rang d'affichage visé par une touche. `dice_count` étant un `u8`, la
/// saturation est inatteignable.
fn rank_of(index: usize) -> u8 {
    u8::try_from(index).unwrap_or(u8::MAX)
}

/// `Update`, sous `in_state(RunPhase::Roll)` et dans `GameSet::HandlingInput`.
///
/// Le verrouillage passe avant la relance : verrouiller et relancer dans la
/// même frame conserve le dé, ce qui est le sens attendu du geste.
fn handle_dice_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    session: Option<ResMut<RunSession>>,
    hand: Option<ResMut<HandContext>>,
    mut dice: ParamSet<(AllDice, FreeDice)>,
) {
    let (Some(mut session), Some(mut hand)) = (session, hand) else {
        return;
    };

    // La touche de rang N vise le dé de `DieView.order == N - 1`. Un rang hors
    // de la main ne rencontre personne : no-op silencieux, sans panic.
    for (index, key) in RANK_KEYS.iter().enumerate() {
        if !keys.just_pressed(*key) {
            continue;
        }
        let rank = rank_of(index);
        for (entity, mut die, view, locked) in dice.p0().iter_mut() {
            if view.order == rank {
                toggle_lock(&mut commands, entity, &mut die, locked);
            }
        }
    }

    if !keys.just_pressed(KeyCode::Space) {
        return;
    }

    // Refus **avant** de toucher aux dés : relancer puis refuser laisserait la
    // main modifiée avec un compteur intact.
    if hand.rerolls_left == 0 {
        return;
    }

    // Trié par identifiant avant de consommer le flux : l'ordre d'itération
    // d'une requête suit l'archétype, que tout marqueur déplace.
    let mut free = dice.p1();
    let mut order: Vec<(DieId, Entity)> =
        free.iter().map(|(entity, die)| (die.id, entity)).collect();
    order.sort_unstable_by_key(|(id, _)| *id);

    for (_, entity) in order {
        if let Ok((_, mut die)) = free.get_mut(entity) {
            die.roll(&mut session.rng.dice, false);
        }
    }

    hand.rerolls_left = hand.rerolls_left.saturating_sub(1);
}

/// Branche les entrées de la phase de lancer.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        handle_dice_input
            .in_set(InputSet::FrozenByOverlay)
            .run_if(in_state(RunPhase::Roll)),
    );

    app.add_systems(
        Update,
        toggle_settings_overlay.in_set(GameSet::HandlingInput),
    );
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use core_engine::config::RunConfig;
    use core_engine::cups::CupId;
    use core_engine::dice::Die;

    use crate::components::{Hidden, Locked};
    use crate::resources::{HandContext, RunSession};
    use crate::states::{AppState, RunPhase, SettingsOverlay};
    use crate::systems::fixtures::{
        app_a_la_graine, app_en_run, deck_de, entites_des, entrer_dans_roll, frapper, valeurs_des,
    };

    #[test]
    fn test_escape_toggles_overlay_without_state_change() {
        // Depuis trois états, dont un hors run : le menu doit être atteignable
        // partout, et n'écrire aucun `NextState` dans aucune branche.
        for cible in [AppState::MainMenu, AppState::InRun, AppState::Codex] {
            let mut app = app_en_run(CupId::Standard);
            app.world_mut()
                .resource_mut::<NextState<AppState>>()
                .set(cible);
            app.update();
            let avant = *app.world().resource::<State<AppState>>().get();
            assert_eq!(avant, cible, "montage : {cible:?}");
            assert!(!app.world().resource::<SettingsOverlay>().open);

            frapper(&mut app, KeyCode::Escape);
            app.update();
            assert!(
                app.world().resource::<SettingsOverlay>().open,
                "l'overlay ne s'ouvre pas depuis {cible:?}"
            );
            assert_eq!(*app.world().resource::<State<AppState>>().get(), avant);

            frapper(&mut app, KeyCode::Escape);
            app.update();
            assert!(
                !app.world().resource::<SettingsOverlay>().open,
                "l'overlay ne se referme pas depuis {cible:?}"
            );
            assert_eq!(
                *app.world().resource::<State<AppState>>().get(),
                avant,
                "l'état a bougé depuis {cible:?}"
            );
        }
    }

    #[test]
    fn test_game_input_is_frozen_while_overlay_open() {
        // **Deux des trois entrées du corpus n'existent pas.** Il n'y a aucun
        // clic dans le projet, et `select_hand` est une fonction, pas un
        // système : aucun ensemble ne peut la geler. Ce test porte donc sur les
        // trois entrées qui existent — verrouillage, relance, soumission.
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        frapper(&mut app, KeyCode::Escape);
        app.update();
        assert!(app.world().resource::<SettingsOverlay>().open);

        let entite = entites_des(&mut app)[0];
        let valeurs = valeurs_des(&mut app);
        let relances = app.world().resource::<HandContext>().rerolls_left;

        frapper(&mut app, KeyCode::Digit1);
        frapper(&mut app, KeyCode::Space);
        frapper(&mut app, KeyCode::Enter);
        app.update();
        app.update();

        assert!(
            app.world().get::<Locked>(entite).is_none(),
            "un verrou a bougé"
        );
        assert_eq!(valeurs_des(&mut app), valeurs, "un dé a bougé");
        assert_eq!(
            app.world().resource::<HandContext>().rerolls_left,
            relances,
            "une relance a été consommée"
        );
        assert_eq!(
            *app.world().resource::<State<RunPhase>>().get(),
            RunPhase::Roll,
            "la soumission est passée"
        );

        // Refermé, les mêmes entrées agissent de nouveau.
        frapper(&mut app, KeyCode::Escape);
        app.update();
        frapper(&mut app, KeyCode::Digit1);
        app.update();

        assert!(
            app.world().get::<Locked>(entite).is_some(),
            "les entrées ne sont pas revenues"
        );
    }

    #[test]
    fn test_reroll_blocked_at_zero() {
        // Le compte de relances n'est pas posé à la main : le gobelet Abandonné
        // n'en donne aucune, et c'est la chaîne de TASK-32 qui le dit.
        let mut app = app_en_run(CupId::Abandoned);
        entrer_dans_roll(&mut app);
        assert_eq!(app.world().resource::<HandContext>().rerolls_left, 0);

        let avant = valeurs_des(&mut app);
        frapper(&mut app, KeyCode::Space);
        app.update();

        assert_eq!(
            valeurs_des(&mut app),
            avant,
            "un dé a bougé malgré le refus"
        );
        assert_eq!(app.world().resource::<HandContext>().rerolls_left, 0);
    }

    #[test]
    fn test_locked_die_keeps_its_value_on_reroll() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        // Verrouiller **par l'entrée**, jamais à la main : c'est ce qui fait
        // mordre ce test sur une bascule qui oublierait `Die.locked`.
        frapper(&mut app, KeyCode::Digit1);
        app.update();

        let avant = valeurs_des(&mut app);
        let relances = app.world().resource::<HandContext>().rerolls_left;

        frapper(&mut app, KeyCode::Space);
        app.update();

        let apres = valeurs_des(&mut app);
        assert_eq!(apres[0], avant[0], "le dé verrouillé a été relancé");
        assert_ne!(&apres[1..], &avant[1..], "aucun dé libre n'a été relancé");
        assert_eq!(
            app.world().resource::<HandContext>().rerolls_left,
            relances.saturating_sub(1)
        );
    }

    #[test]
    fn test_toggle_lock_twice_is_identity() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        let entite = entites_des(&mut app)[0];

        frapper(&mut app, KeyCode::Digit1);
        app.update();
        // Le marqueur **et** le champ, dans la même bascule : une bascule qui
        // n'en poserait qu'un rendrait le verrouillage inopérant sans erreur
        // de compilation.
        assert!(app.world().get::<Locked>(entite).is_some(), "marqueur posé");
        assert!(
            app.world().get::<Die>(entite).expect("dé").locked,
            "champ Die.locked posé"
        );

        frapper(&mut app, KeyCode::Digit1);
        app.update();
        assert!(
            app.world().get::<Locked>(entite).is_none(),
            "marqueur retiré"
        );
        assert!(
            !app.world().get::<Die>(entite).expect("dé").locked,
            "champ Die.locked retiré"
        );
    }

    #[test]
    fn test_reroll_decrements_once_per_press() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        let avant = app.world().resource::<HandContext>().rerolls_left;
        assert!(avant >= 2, "le gobelet doit pouvoir relancer deux fois");

        // Une seule frappe, trois frames. `just_pressed` ne vaut qu'une fois ;
        // `pressed` vaudrait trois, et viderait les relances d'un coup.
        frapper(&mut app, KeyCode::Space);
        app.update();
        app.update();
        app.update();

        assert_eq!(
            app.world().resource::<HandContext>().rerolls_left,
            avant.saturating_sub(1)
        );
    }

    #[test]
    fn test_input_is_inert_when_overlay_open() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        app.world_mut().resource_mut::<SettingsOverlay>().open = true;

        let valeurs = valeurs_des(&mut app);
        let relances = app.world().resource::<HandContext>().rerolls_left;
        let entite = entites_des(&mut app)[0];

        frapper(&mut app, KeyCode::Space);
        frapper(&mut app, KeyCode::Digit1);
        app.update();

        assert_eq!(valeurs_des(&mut app), valeurs, "un dé a bougé");
        assert_eq!(
            app.world().resource::<HandContext>().rerolls_left,
            relances,
            "les relances ont bougé"
        );
        assert!(
            app.world().get::<Locked>(entite).is_none(),
            "un verrou a bougé"
        );
    }

    #[test]
    fn test_digit_out_of_range_is_ignored() {
        let mut app = app_en_run(CupId::Standard);
        app.world_mut().resource_mut::<RunSession>().config = RunConfig::from_cup(&deck_de(4));
        entrer_dans_roll(&mut app);
        assert_eq!(entites_des(&mut app).len(), 4);

        frapper(&mut app, KeyCode::Digit6);
        app.update();

        for entite in entites_des(&mut app) {
            assert!(
                app.world().get::<Locked>(entite).is_none(),
                "un rang hors de la main a verrouillé un dé"
            );
        }
    }

    #[test]
    fn test_reroll_order_follows_die_ids() {
        // Deux applications de même graine, dont l'une a vu ses dés changer
        // d'archétype. `Hidden` ne touche pas au filtre `Without<Locked>` :
        // seul l'ordre d'itération de la requête bouge. Sans le tri par
        // `DieId`, les deux relances divergeraient sur le même flux.
        let mut a = app_a_la_graine(CupId::Standard, 42);
        let mut b = app_a_la_graine(CupId::Standard, 42);
        entrer_dans_roll(&mut a);
        entrer_dans_roll(&mut b);
        assert_eq!(valeurs_des(&mut a), valeurs_des(&mut b));

        for (rang, entite) in entites_des(&mut b).into_iter().enumerate() {
            if rang % 2 == 0 {
                b.world_mut().entity_mut(entite).insert(Hidden);
            }
        }

        frapper(&mut a, KeyCode::Space);
        frapper(&mut b, KeyCode::Space);
        a.update();
        b.update();

        assert_eq!(valeurs_des(&mut a), valeurs_des(&mut b));
    }

    #[test]
    fn test_lock_and_reroll_in_the_same_frame() {
        // Le marqueur `Locked` est posé par une commande, appliquée en fin de
        // schedule : dans la même frame, la requête `Without<Locked>` voit
        // encore le dé comme libre. C'est `Die.locked`, posé immédiatement,
        // que `Die::roll` consulte et qui le sauve. Les deux filtres ne font
        // pas double emploi, ils se couvrent.
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        let avant = valeurs_des(&mut app);

        frapper(&mut app, KeyCode::Digit1);
        frapper(&mut app, KeyCode::Space);
        app.update();

        let apres = valeurs_des(&mut app);
        assert_eq!(
            apres[0], avant[0],
            "le dé verrouillé dans la même frame a été relancé"
        );
        assert_ne!(&apres[1..], &avant[1..], "aucun dé libre n'a été relancé");
    }
}
