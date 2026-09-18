//! La liste des plugins du jeu, telle que la cible de mesure la monte.
//!
//! **De l'outillage, pas du code de jeu.** Aucune règle, aucune constante de
//! gameplay, aucun composant, aucune ressource, aucun système : une liste, dans
//! un ordre. Elle vit dans une fonction pour une seule raison : le binaire mesuré
//! n'est **jamais exécuté**, il est compilé pour `wasm32-unknown-unknown` et pesé.
//! Une erreur d'ordre ou un double enregistrement entre plugins donnerait une
//! mesure parfaitement stable pour un jeu qui panique à l'ouverture. Le test de
//! démarrage (`tests/boot.rs`) monte **cette même liste**, et la fait tourner.
//!
//! Le son n'est pas dans la liste : la cible l'ajoute après elle, derrière la
//! feature `audio`, sur son backend réel ; le test l'ajoute après elle, sur son
//! backend nul. L'ordre est donc le même des deux côtés, par construction : le
//! plugin audio exige les états et la mise en scène déjà montés, et le dit par
//! une assertion dans son `build`.

use bevy::prelude::*;
use game_state::GameStatePlugin;
use shop_system::ShopPlugin;
use ui_and_juice::JuicePlugin;
use ui_and_juice::graphics::VisualEffectsPlugin;

/// Monte les quatre plugins de jeu, dans l'ordre. La CI exige chacun d'eux ici :
/// un plugin absent de cette liste est un plugin que la mesure ne voit pas.
pub fn add_game_plugins(app: &mut App) -> &mut App {
    app.add_plugins(GameStatePlugin)
        .add_plugins(JuicePlugin)
        .add_plugins(VisualEffectsPlugin)
        .add_plugins(ShopPlugin)
}
