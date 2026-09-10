//! Cible de mesure de la taille du binaire WebAssembly.
//!
//! **De l'outillage, pas du code de jeu.** Aucune règle, aucune constante de
//! gameplay, aucun système propre : elle monte l'`App` avec la liste de plugins
//! et de features réelles, et c'est tout.
//!
//! # Elle vit dans la crate du sommet
//!
//! Elle doit monter le jeu **entier**. `ui_and_juice` est la seule crate qui
//! dépende des deux autres ; la laisser dans `game_state` ferait mesurer un
//! binaire amputé de toute la mise en scène, et l'écart jouerait **à la
//! baisse**, donc sans déclencher la garde de CI.
//!
//! # Pourquoi une cible binaire
//!
//! Une crate bibliothèque compilée pour `wasm32-unknown-unknown` produit une
//! `rlib`, pas un `.wasm`. Sans binaire lié, le linker n'élimine rien : la
//! mesure porterait sur du code mort et ne voudrait rien dire. C'est le graphe
//! de systèmes **réel** qui décide de ce que le linker conserve, et c'est
//! pourquoi cette mesure attend que la boucle d'états soit complète.
//!
//! # Pourquoi elle est derrière une feature
//!
//! Sans garde, cette cible serait construite par `cargo build --workspace` et
//! `cargo test --workspace`, à chaque ticket, pour lier un binaire Bevy complet
//! dont personne n'a besoin. `required-features` la réserve à qui la demande.

use bevy::prelude::*;
use game_state::GameStatePlugin;
use ui_and_juice::JuicePlugin;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, GameStatePlugin, JuicePlugin))
        .run();
}
