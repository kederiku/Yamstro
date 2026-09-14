//! Montages de test partagés par les modules de systèmes.
//!
//! **Un module `cfg(test)`, jamais une feature Cargo.** La feature
//! `test-fixtures` est proscrite par le critère 6 du volet 1 : elle ferait
//! entrer du code de test dans un build de production. Un module `cfg(test)`
//! disparaît en release et ne franchit pas la frontière de crate.
//!
//! Sans lui, chaque module de systèmes recopierait le même montage, et les
//! copies divergeraient au premier champ ajouté à `RunSession`.
//!
//! **`InputPlugin` est monté ici.** `handle_dice_input` (TASK-34) lit
//! `Res<ButtonInput<KeyCode>>` : sans le plugin, la ressource est absente et
//! la construction des paramètres du système panique, sur un message qui ne
//! nomme ni le système ni la ressource hors feature `debug`.

use bevy::input::InputPlugin;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::{ButtonState, keyboard::KeyCode};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use core_engine::blinds::BlindType;
use core_engine::config::RunConfig;
use core_engine::cups::definitions::cup;
use core_engine::cups::{CupDeck, CupId};
use core_engine::dice::Die;
use core_engine::hands::HandLevels;
use core_engine::relics::RelicInventory;
use core_engine::rng::RunRng;

use crate::components::DieView;
use crate::resources::RunSession;
use crate::states::{AppState, RunPhase};

/// Session d'une run, sur un gobelet du catalogue et un niveau de stake.
pub(crate) fn session(id: CupId, stake_level: u8) -> RunSession {
    let deck = cup(id);
    RunSession {
        config: RunConfig::from_cup(&deck),
        ante: 1,
        blind_kind: BlindType::Small,
        gold: deck.starting_gold,
        cup_id: id,
        stake_level,
        hand_levels: HandLevels::default(),
        rng: RunRng::from_seed(1),
    }
}

/// Inventaire vide, dimensionné par la configuration et jamais par un littéral.
pub(crate) fn inventaire(config: &RunConfig) -> RelicInventory {
    RelicInventory::new(config.relic_capacity)
}

/// Gobelet ad hoc, pour les tailles de main que le catalogue ne porte pas.
/// **Aucun gobelet n'est ajouté au catalogue**, arrêté à l'Étape 9.
pub(crate) fn deck_de(n: u8) -> CupDeck {
    CupDeck {
        dice_count: n,
        sides: vec![6; usize::from(n)],
        ..cup(CupId::Standard)
    }
}

/// Application montée en headless, session et inventaire posés **avant**
/// l'entrée dans la run : le premier `OnEnter(BlindSelect)` suit immédiatement
/// `OnEnter(InRun)`, dans la même transition.
pub(crate) fn app_en_run(id: CupId) -> App {
    let partie = session(id, 0);
    let stock = inventaire(&partie.config);

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        InputPlugin,
        crate::GameStatePlugin,
    ));
    app.insert_resource(partie);
    app.insert_resource(stock);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::InRun);
    app.update();
    app
}

/// Application en run, sur une graine maîtresse choisie.
pub(crate) fn app_a_la_graine(id: CupId, seed: u64) -> App {
    let mut app = app_en_run(id);
    app.world_mut().resource_mut::<RunSession>().rng = RunRng::from_seed(seed);
    app
}

/// Entre dans la phase de lancer. Un `set` nu : la ré-entrée `Roll → Roll` est
/// un cas normal, celui de la main suivante d'une même blind.
pub(crate) fn entrer_dans_roll(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<RunPhase>>()
        .set(RunPhase::Roll);
    app.update();
}

/// Frappe une touche : une pression **et** son relâchement, écrits en
/// **messages** `KeyboardInput`.
///
/// Deux mesures dictent cette forme, et aucune n'est devinable :
///
/// 1. `keyboard_input_system` ouvre sur `clear()` en `PreUpdate`. Une pression
///    posée à la main sur `ButtonInput` est donc effacée avant l'`Update`, et
///    le test ne teste rien — un test de refus passerait sans qu'aucune touche
///    n'ait jamais été vue.
/// 2. `ButtonInput::press` n'alimente `just_pressed` que si la touche n'était
///    pas déjà dans `pressed`, et `clear()` ne vide **que** `just_pressed` et
///    `just_released`. Sans relâchement, la touche reste enfoncée pour toujours
///    et la **seconde** frappe est muette. Deux bascules successives sur le
///    même dé sont alors inexprimables.
///
/// Le relâchement laisse `just_pressed` armé pour la frame en cours : c'est
/// bien une frappe, vue une fois et une seule.
///
/// `logical_key` est laissé indéterminé : seul `key_code` alimente le
/// `ButtonInput<KeyCode>` que lisent les systèmes de ce projet.
pub(crate) fn frapper(app: &mut App, code: KeyCode) {
    for etat in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput {
            key_code: code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: etat,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    }
}

/// Les dés du monde, triés par `DieId`.
pub(crate) fn des_tries(app: &mut App) -> Vec<(Entity, Die, DieView)> {
    let mut etat = app.world_mut().query::<(Entity, &Die, &DieView)>();
    let mut v: Vec<(Entity, Die, DieView)> = etat
        .iter(app.world())
        .map(|(e, d, w)| (e, d.clone(), *w))
        .collect();
    v.sort_unstable_by_key(|(_, d, _)| d.id);
    v
}

/// Les entités-dés, triées par `DieId`.
pub(crate) fn entites_des(app: &mut App) -> Vec<Entity> {
    des_tries(app).into_iter().map(|(e, _, _)| e).collect()
}

/// Les valeurs courantes des dés, triées par `DieId`.
pub(crate) fn valeurs_des(app: &mut App) -> Vec<u8> {
    des_tries(app)
        .into_iter()
        .map(|(_, d, _)| d.current_value)
        .collect()
}
