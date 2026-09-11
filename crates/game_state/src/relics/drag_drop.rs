//! Glisser-déposer des reliques : le joueur réordonne, donc il change son score.
//!
//! # Ce que ce geste change
//!
//! L'ordre strict de gauche à droite (ADR-005) fait qu'une multiplication
//! placée avant une addition ne rend pas le même score que l'inverse. Sur un
//! Brelan de 4, `[Maître du Brelan, Le Balancier]` rend **264** et l'ordre
//! inverse **198** : deux entiers exacts. Sans ce fichier, ces deux nombres
//! sont une propriété du moteur ; avec lui, ils deviennent un choix du joueur.
//!
//! # Une action, deux entrées
//!
//! `begin_hold` et `drop_on` sont **publiques** et ne dépendent d'aucun
//! périphérique. La souris les appelle ici ; le clavier et la manette de
//! l'Étape 11 appelleront les mêmes. Enfermer l'action dans le système de
//! souris obligerait l'Étape 11 à la réécrire, et deux réécritures du même
//! ordonnancement divergent au premier cas limite.
//!
//! Elles n'ordonnancent rien elles-mêmes : elles **appellent `reorder`**. Pas
//! de retrait suivi d'une insertion locale, pas d'accès direct aux slots. Le
//! décalage a une seule implémentation, et c'est celle qui est testée.
//!
//! # La cible vient d'`Interaction`, et ce que cela laisse non vérifié
//!
//! `Interaction` est le type de première main de `bevy_ui` pour cela, et
//! `ui_focus_system` l'alimente sans condition — il n'est pas derrière la
//! feature de picking. Refaire le test d'intersection à la main depuis
//! `ComputedNode` et `UiGlobalTransform` réécrirait ce système pour le même
//! résultat.
//!
//! **Mais rien ne l'alimente ici.** `ui_focus_system` exige `UiPlugin` et une
//! fenêtre principale, qu'aucun test de ce dépôt ne monte et qu'aucune UI de
//! production ne réclame encore. Les tests posent donc l'`Interaction` à la
//! main, comme le dépôt pilote déjà `ButtonInput` : **la logique de prise et de
//! dépose est vérifiée, le lien entre un pixel et une carte ne l'est pas.** Il
//! le sera quand une UI sera montée, et pas avant.
//!
//! # Le curseur
//!
//! `DragState.cursor` n'entre dans aucune décision, la cible venant
//! d'`Interaction`. Il est tout de même alimenté depuis les messages
//! `CursorMoved`, pour que l'Étape 7 y trouve de quoi faire suivre une carte
//! fantôme : un champ public qu'aucune source n'écrit finit par contenir
//! n'importe quoi.

use bevy::prelude::*;
use bevy::window::CursorMoved;
use core_engine::relics::RelicInventory;

use crate::components::RelicSlotUI;
use crate::states::AppState;

/// L'état du geste en cours. **`Resource` uniquement.**
///
/// En 0.19, `Resource` est un sous-trait de `Component` : les deux ne
/// coexistent pas, et une copie logée dans une entité perdrait l'unicité qui
/// fait tout l'intérêt d'un état d'interaction global.
///
/// `held` porte un **index de slot**, ni une `Entity` ni un `uid` : c'est ce
/// que `reorder` consomme. Ce n'est pas une donnée de jeu — l'inventaire reste
/// la source de vérité — mais un index en cours de manipulation.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct DragState {
    pub held: Option<u8>,
    pub cursor: Vec2,
}

/// Prend la relique du slot `slot`.
pub fn begin_hold(state: &mut DragState, slot: u8) {
    state.held = Some(slot);
}

/// Dépose la relique tenue sur le slot `slot`.
///
/// **Seul point d'écriture de l'inventaire de ce fichier**, et il délègue :
/// `reorder` décale, il n'échange pas, et il rend la main sans rien faire sur
/// un index hors bornes.
pub fn drop_on(state: &mut DragState, inventory: &mut RelicInventory, slot: u8) {
    if let Some(from) = state.held.take() {
        inventory.reorder(from, slot);
    }
}

/// Relâche sans rien déposer : l'inventaire est strictement inchangé.
pub fn cancel_hold(state: &mut DragState) {
    state.held = None;
}

/// Oublie une prise dont le slot s'est vidé entre-temps.
///
/// Vente, boss d'extinction, despawn : un index périmé passé à `reorder` ne
/// serait qu'un no-op silencieux, mais l'état d'interaction resterait bloqué
/// jusqu'au prochain relâchement. La vérification est en tête du système, une
/// fois par frame, plutôt que dans `drop_on` — qui arrive trop tard.
fn forget_vanished_hold(state: &mut DragState, inventory: &RelicInventory) {
    if let Some(slot) = state.held
        && !inventory.iter_slots().any(|(occupe, _)| occupe == slot)
    {
        state.held = None;
    }
}

/// Le seul système de ce fichier : il lit les entrées et appelle les actions.
pub fn drag_relic_cards(
    mut state: ResMut<DragState>,
    mut inventory: ResMut<RelicInventory>,
    mut moved: MessageReader<CursorMoved>,
    mouse: Res<ButtonInput<MouseButton>>,
    cards: Query<(&RelicSlotUI, &Interaction)>,
) {
    if let Some(dernier) = moved.read().last() {
        state.cursor = dernier.position;
    }
    forget_vanished_hold(&mut state, &inventory);

    if mouse.just_pressed(MouseButton::Left)
        && let Some((carte, _)) = cards
            .iter()
            .find(|(_, interaction)| **interaction == Interaction::Pressed)
    {
        begin_hold(&mut state, carte.0);
    }

    if mouse.just_released(MouseButton::Left) {
        let cible = cards
            .iter()
            .find(|(_, interaction)| {
                matches!(interaction, Interaction::Pressed | Interaction::Hovered)
            })
            .map(|(carte, _)| carte.0);
        match cible {
            Some(slot) => drop_on(&mut state, &mut inventory, slot),
            None => cancel_hold(&mut state),
        }
    }
}

pub(crate) fn register(app: &mut App) {
    // `CursorMoved` appartient à `WindowPlugin`, que les montages headless ne
    // posent pas. `add_message` est idempotent — il ne fait rien si la file
    // existe déjà —, donc cette ligne est sans effet en production et rend le
    // paramètre valide partout ailleurs.
    app.add_message::<CursorMoved>();
    app.init_resource::<DragState>();
    app.add_systems(
        Update,
        drag_relic_cards
            .run_if(in_state(AppState::InRun).and_then(resource_exists::<RelicInventory>)),
    );
}

#[cfg(test)]
mod tests {
    use bevy::input::ButtonState;
    use bevy::input::mouse::MouseButtonInput;
    use bevy::prelude::*;
    use core_engine::blind::{BlindContext, BlindDefinition};
    use core_engine::cups::CupId;
    use core_engine::dice::{Die, DieId};
    use core_engine::evaluator::{HandEvaluator, HandMatch};
    use core_engine::hands::{HandGrid, HandLevels, YahtzeeHand};
    use core_engine::relics::{RelicId, RelicInventory};
    use core_engine::scoring::ScoringPipeline;

    use super::*;
    use crate::components::RelicSlotUI;
    use crate::systems::fixtures::app_en_run;

    const CINQ: [RelicId; 5] = [
        RelicId::CrackedDie,
        RelicId::PolishedStone,
        RelicId::TripletMaster,
        RelicId::FullHouseArchitect,
        RelicId::StellarAlignment,
    ];

    fn inventaire(defs: &[RelicId]) -> RelicInventory {
        let mut stock = RelicInventory::new(5);
        for def in defs {
            stock.add_relic(*def).expect("slot libre");
        }
        stock
    }

    /// Les définitions slot par slot, trous compris : c'est la comparaison
    /// « identique slot par slot » que les tests d'ordre demandent.
    fn ordre(stock: &RelicInventory) -> Vec<Option<RelicId>> {
        stock
            .slots
            .iter()
            .map(|slot| slot.map(|instance| instance.def))
            .collect()
    }

    fn deux_frames(app: &mut App) {
        app.update();
        app.update();
    }

    /// Pose l'`Interaction` d'une carte à la main. `ui_focus_system` la
    /// poserait en production, mais il exige `UiPlugin` et une fenêtre, que ce
    /// dépôt ne monte nulle part — voir l'en-tête du module.
    fn interagir(app: &mut App, slot: u8, etat: Interaction) {
        let mut requete = app.world_mut().query::<(Entity, &RelicSlotUI)>();
        let cible = requete
            .iter(app.world())
            .find(|(_, carte)| carte.0 == slot)
            .map(|(entite, _)| entite);
        if let Some(entite) = cible {
            app.world_mut().entity_mut(entite).insert(etat);
        }
    }

    /// Écrit un message de bouton, jamais une pression posée sur la ressource.
    ///
    /// `mouse_button_input_system` ouvre sur `clear()` en `PreUpdate` : une
    /// pression posée à la main est effacée avant l'`Update`, et
    /// `just_pressed` ne voit rien. Même piège que celui que `fixtures::frapper`
    /// documente pour le clavier, et il coûte ici un test qui passerait en
    /// mesurant le vide.
    fn bouton(app: &mut App, etat: ButtonState) {
        app.world_mut().write_message(MouseButtonInput {
            button: MouseButton::Left,
            state: etat,
            window: Entity::PLACEHOLDER,
        });
    }

    #[test]
    fn test_drag_zero_to_two_equals_reorder() {
        let mut etat = DragState::default();
        let mut glisse = inventaire(&CINQ);
        let mut direct = inventaire(&CINQ);

        begin_hold(&mut etat, 0);
        drop_on(&mut etat, &mut glisse, 2);
        direct.reorder(0, 2);

        assert_eq!(ordre(&glisse), ordre(&direct));
        assert_eq!(
            ordre(&glisse).into_iter().flatten().collect::<Vec<_>>(),
            vec![
                RelicId::PolishedStone,
                RelicId::TripletMaster,
                RelicId::CrackedDie,
                RelicId::FullHouseArchitect,
                RelicId::StellarAlignment,
            ]
        );
    }

    #[test]
    fn test_drag_does_not_duplicate_shift_logic() {
        // Un décalage, jamais un échange : l'échange rendrait `[A,D,C,B,E]`.
        let mut etat = DragState::default();
        let mut stock = inventaire(&CINQ);

        begin_hold(&mut etat, 3);
        drop_on(&mut etat, &mut stock, 1);

        assert_eq!(
            ordre(&stock).into_iter().flatten().collect::<Vec<_>>(),
            vec![
                RelicId::CrackedDie,
                RelicId::FullHouseArchitect,
                RelicId::PolishedStone,
                RelicId::TripletMaster,
                RelicId::StellarAlignment,
            ]
        );
    }

    #[test]
    fn test_release_outside_valid_slot_cancels() {
        let mut stock = inventaire(&CINQ);
        let avant = ordre(&stock);

        // Relâchement hors de toute carte.
        let mut etat = DragState::default();
        begin_hold(&mut etat, 0);
        cancel_hold(&mut etat);
        assert_eq!(etat.held, None);
        assert_eq!(ordre(&stock), avant);

        // Relâchement sur un index hors bornes : `reorder` est un no-op.
        begin_hold(&mut etat, 0);
        drop_on(&mut etat, &mut stock, 99);
        assert_eq!(etat.held, None);
        assert_eq!(ordre(&stock), avant);
    }

    #[test]
    fn test_drag_state_returns_to_none() {
        let mut stock = inventaire(&CINQ);

        let mut reussi = DragState::default();
        begin_hold(&mut reussi, 0);
        assert_eq!(reussi.held, Some(0));
        drop_on(&mut reussi, &mut stock, 1);
        assert_eq!(reussi.held, None);

        let mut annule = DragState::default();
        begin_hold(&mut annule, 0);
        assert_eq!(annule.held, Some(0));
        cancel_hold(&mut annule);
        assert_eq!(annule.held, None);
    }

    #[test]
    fn test_held_is_cleared_if_card_disappears() {
        let mut etat = DragState::default();
        let mut stock = inventaire(&CINQ);
        begin_hold(&mut etat, 2);

        stock.remove_relic(2).expect("relique au slot 2");
        forget_vanished_hold(&mut etat, &stock);
        assert_eq!(etat.held, None, "prise restée sur un slot vidé");

        // Le relâchement suivant ne réordonne rien.
        let avant = ordre(&stock);
        drop_on(&mut etat, &mut stock, 0);
        assert_eq!(ordre(&stock), avant);
    }

    #[test]
    fn test_keyboard_path_uses_the_same_action() {
        // Deux entrées, un seul chemin : la souris appelle ces deux fonctions,
        // le clavier de l'Étape 11 appellera les mêmes.
        let mut par_action = inventaire(&CINQ);
        let mut etat = DragState::default();
        begin_hold(&mut etat, 1);
        drop_on(&mut etat, &mut par_action, 3);

        let mut app = app_en_run(CupId::Standard);
        {
            let mut stock = app.world_mut().resource_mut::<RelicInventory>();
            for def in CINQ {
                stock.add_relic(def).expect("slot libre");
            }
        }
        deux_frames(&mut app);

        bouton(&mut app, ButtonState::Pressed);
        interagir(&mut app, 1, Interaction::Pressed);
        app.update();
        assert_eq!(app.world().resource::<DragState>().held, Some(1));

        interagir(&mut app, 1, Interaction::None);
        interagir(&mut app, 3, Interaction::Hovered);
        bouton(&mut app, ButtonState::Released);
        app.update();

        assert_eq!(app.world().resource::<DragState>().held, None);
        assert_eq!(
            ordre(app.world().resource::<RelicInventory>()),
            ordre(&par_action)
        );
    }

    /// Une application en run, inventaire rempli et cartes nées.
    fn app_avec_cartes() -> App {
        let mut app = app_en_run(CupId::Standard);
        {
            let mut stock = app.world_mut().resource_mut::<RelicInventory>();
            for def in CINQ {
                stock.add_relic(def).expect("slot libre");
            }
        }
        deux_frames(&mut app);
        app
    }

    #[test]
    fn test_system_release_outside_any_card_changes_nothing() {
        // **Mesuré : sans ce test, un relâchement hors cible qui déposerait sur
        // le slot 0 survit.** Les fonctions pures sont éprouvées plus haut, mais
        // c'est le système qui choisit entre déposer et annuler, et personne ne
        // le regardait faire ce choix.
        let mut app = app_avec_cartes();
        let avant = ordre(app.world().resource::<RelicInventory>());

        bouton(&mut app, ButtonState::Pressed);
        interagir(&mut app, 3, Interaction::Pressed);
        app.update();
        assert_eq!(app.world().resource::<DragState>().held, Some(3));

        // Aucune carte sous le curseur au relâchement.
        interagir(&mut app, 3, Interaction::None);
        bouton(&mut app, ButtonState::Released);
        app.update();

        assert_eq!(app.world().resource::<DragState>().held, None);
        assert_eq!(
            ordre(app.world().resource::<RelicInventory>()),
            avant,
            "un relâchement dans le vide a réordonné l'inventaire"
        );
    }

    #[test]
    fn test_system_clears_hold_when_the_slot_empties() {
        // **Mesuré aussi : sans ce test, retirer le nettoyage du système ne
        // casse rien.** Le test de la fonction pure appelle
        // `forget_vanished_hold` lui-même ; il ne dit pas que le système
        // l'appelle.
        let mut app = app_avec_cartes();

        bouton(&mut app, ButtonState::Pressed);
        interagir(&mut app, 2, Interaction::Pressed);
        app.update();
        assert_eq!(app.world().resource::<DragState>().held, Some(2));

        app.world_mut()
            .resource_mut::<RelicInventory>()
            .remove_relic(2)
            .expect("relique au slot 2");
        app.update();

        assert_eq!(
            app.world().resource::<DragState>().held,
            None,
            "la prise est restée sur un slot vidé"
        );
    }

    #[test]
    fn test_cursor_follows_cursor_moved_messages() {
        // Le curseur n'entre dans aucune décision — la cible vient
        // d'`Interaction` — mais il est alimenté, et l'Étape 7 y trouvera de
        // quoi faire suivre une carte fantôme. Un champ qu'aucune source
        // n'écrit finit par contenir n'importe quoi.
        let mut app = app_en_run(CupId::Standard);
        app.world_mut().write_message(CursorMoved {
            window: Entity::PLACEHOLDER,
            position: Vec2::new(120.0, 48.0),
            delta: None,
        });
        app.update();

        assert_eq!(
            app.world().resource::<DragState>().cursor,
            Vec2::new(120.0, 48.0)
        );
    }

    #[test]
    fn test_score_changes_after_reorder() {
        // **Ce que ce test ajoute, c'est le chemin du geste.** L'arithmétique
        // — 264 puis 198 — est déjà gardée par `test_relic_reorder_changes_
        // score`, écrit à TASK-59 sur ce même inventaire. Recopier les deux
        // nombres pour eux-mêmes donnerait deux exemplaires à maintenir ; ce
        // qui n'est gardé nulle part, c'est que `drop_on` atteigne le moteur.
        let dice: Vec<Die> = [4u8, 4, 4, 6, 1]
            .into_iter()
            .enumerate()
            .map(|(index, valeur)| {
                let mut de = Die::new(DieId(index as u32), 6);
                de.current_value = valeur;
                de
            })
            .collect();
        let main: HandMatch = HandEvaluator::evaluate(&dice)
            .into_iter()
            .find(|candidat| candidat.hand == YahtzeeHand::ThreeOfAKind)
            .expect("brelan");
        let manche = BlindContext {
            blind: BlindDefinition::default(),
            target_score: 300,
            current_score: 0,
            hands_remaining: 4,
            used_hands: HandGrid::default(),
        };

        let mut stock = inventaire(&[RelicId::TripletMaster, RelicId::Pendulum]);
        let score = |stock: &RelicInventory| {
            ScoringPipeline::resolve(&main, &dice, &HandLevels::default(), stock, &manche)
                .final_score
        };

        let avant = score(&stock);
        let mut etat = DragState::default();
        begin_hold(&mut etat, 0);
        drop_on(&mut etat, &mut stock, 1);
        let apres = score(&stock);

        let mut par_reorder = inventaire(&[RelicId::TripletMaster, RelicId::Pendulum]);
        par_reorder.reorder(0, 1);
        assert_eq!(
            ordre(&stock),
            ordre(&par_reorder),
            "le geste ne produit pas le même inventaire que reorder"
        );

        assert_eq!(avant, 264);
        assert_eq!(apres, 198);
        assert_ne!(avant, apres);
    }
}
