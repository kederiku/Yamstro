//! Cible de mesure de la taille du binaire WebAssembly.
//!
//! Elle monte l'`App` comme le jeu expédié la montera : `DefaultPlugins` avec la
//! configuration de rendu du jeu (`render_plugin`, TASK-93), les plugins de jeu
//! (`add_game_plugins`), le son, et une caméra 2D d'observation, pour que ce
//! qu'elle rend soit visible dans un navigateur ; le jeu montera la sienne.
//!
//! # Pourquoi une cible binaire, et pourquoi une crate à elle
//!
//! Une bibliothèque compilée pour `wasm32-unknown-unknown` produit une `rlib`,
//! pas un `.wasm` : sans binaire lié, l'éditeur de liens n'élimine rien et la
//! mesure porte sur du code mort. Et ce binaire doit dépendre de **tout** : voir
//! le manifeste.
//!
//! # Le backend réel, parce que c'est le jeu expédié
//!
//! La cible monte `GameAudioPlugin::new()` : c'est ce que le jeu montera, et une
//! garde de CI refuse ici le backend nul. **Ce n'est pas la balance qui le
//! verrait.** Le ticket craignait que, sur le backend nul, l'éditeur de liens
//! élimine le moteur et ses décodeurs, et que la mesure retombe sur un `.wasm`
//! sans son. Mesuré à TASK-108 : le binaire pèse **le même poids à l'octet
//! près**. Le plugin choisit son backend à l'exécution, dans son `build` : les
//! deux sont référencés, donc liés, quel que soit le constructeur appelé ici.
//!
//! Ce que la balance voit, et que le texte ne voit pas : un son qui ne serait
//! **plus lié du tout**, feature retirée par mégarde, dépendance devenue morte,
//! drapeau qui ne mord plus. La porte de taille construit donc la cible deux
//! fois, sans le son puis avec lui, et tient un **plancher** sur la différence.

use bevy::prelude::*;
use ui_and_juice::graphics::plugin::render_plugin;
use wasm_size::add_game_plugins;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(render_plugin()));
    add_game_plugins(&mut app);
    #[cfg(feature = "audio")]
    app.add_plugins(audio_system::GameAudioPlugin::new());
    app.add_systems(Startup, spawn_camera).run();
}

/// La caméra d'observation : sans elle, rien n'est rendu et rien ne se
/// vérifie dans un navigateur.
fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
