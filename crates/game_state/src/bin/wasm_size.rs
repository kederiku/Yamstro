//! Cible de mesure de la taille du binaire WebAssembly.
//!
//! **De l'outillage, pas du code de jeu.** Aucune règle, aucune constante de
//! gameplay, aucun système propre : elle monte l'`App` avec la liste de plugins
//! et de features réelles, et c'est tout.
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

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, GameStatePlugin))
        .run();
}
