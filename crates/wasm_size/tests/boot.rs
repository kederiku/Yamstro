//! **Le jeu mesuré démarre.** La cible de mesure n'est jamais exécutée : ce test
//! monte la même liste de plugins, dans le même ordre, sur une base sans fenêtre
//! ni rendu, et la fait tourner. C'est aussi le premier endroit du dépôt où les
//! cinq plugins vivent dans une même application.

use audio_system::{AudioBackendHandle, GameAudioPlugin};
use bevy::{
    input::InputPlugin, mesh::MeshPlugin, prelude::*, render::sync_world::SyncWorldPlugin,
    state::app::StatesPlugin,
};
use core_engine::shop::ShopInventory;
use game_state::states::AppState;
use ui_and_juice::settings::{JuiceSettings, SafeMode};
use wasm_size::add_game_plugins;

/// Ce que `DefaultPlugins` monte et dont les plugins de jeu ont besoin, sans le
/// rendu ni winit : la base des tests de la mise en scène et de la boutique.
fn headless_base() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        StatesPlugin,
        InputPlugin,
        MeshPlugin,
        WindowPlugin::default(),
        SyncWorldPlugin,
    ));
    app.init_asset::<Shader>();
    app
}

#[test]
fn test_the_measured_game_boots() {
    let mut app = headless_base();
    add_game_plugins(&mut app);
    // Le son après la liste, comme dans la cible ; sur le backend nul, pour
    // qu'aucun test n'ouvre un périphérique.
    app.add_plugins(GameAudioPlugin::headless());
    for _ in 0..5 {
        app.update();
    }

    // Chaque plugin de la liste a laissé sa trace : les états, la mise en scène, les effets
    // visuels, la boutique. Un plugin retiré de la liste fait tomber sa ligne.
    let world = app.world();
    assert!(world.contains_resource::<State<AppState>>(), "les états");
    assert!(
        world.contains_resource::<JuiceSettings>(),
        "la mise en scène"
    );
    assert!(world.contains_resource::<SafeMode>(), "les effets visuels");
    assert!(world.contains_resource::<ShopInventory>(), "la boutique");
    let handle = world.resource::<AudioBackendHandle>();
    let journal = handle.null().expect("le backend monté n'est pas le nul");
    assert_eq!(
        journal.loaded().len(),
        17,
        "la banque n'a pas chargé ses clips"
    );
    assert!(
        journal.layers_started(),
        "les quatre couches ne sont pas parties"
    );
}

/// Le son **avant** la liste : le plugin audio le refuse, par son assertion. Si
/// cet ordre passait un jour, la cible pourrait l'adopter sans que rien tombe.
#[test]
#[should_panic(expected = "`GameAudioPlugin` exige")]
fn test_audio_before_the_game_plugins_is_refused() {
    let mut app = headless_base();
    app.add_plugins(GameAudioPlugin::headless());
    add_game_plugins(&mut app);
}
