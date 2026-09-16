//! Cible de mesure de la taille du binaire WebAssembly.
//!
//! **De l'outillage, pas du code de jeu.** Aucune règle, aucune constante de
//! gameplay, aucun système propre : elle monte l'`App` avec la liste de plugins
//! et de features réelles, et c'est tout.
//!
//! # Elle vit dans la crate du sommet du graphe, et elle y remonte quand le sommet change
//!
//! Elle doit monter le jeu **entier**, et le jeu entier n'est visible que
//! depuis la crate dont toutes les autres sont des dépendances. `shop_system`
//! est aujourd'hui la seule crate qui dépende des trois autres. La laisser en
//! dessous ferait mesurer un binaire amputé, et l'écart jouerait **à la
//! baisse**, donc sans déclencher la garde de CI. C'est ce qui est arrivé :
//! posée dans la crate de mise en scène, la seule au sommet à l'Étape 4, la
//! cible a ignoré toute la boutique de TASK-78 à TASK-82 — mesuré à TASK-81,
//! pas un octet de `shop_system` dans les 21 214 608 de la clôture d'Étape 6.
//! **Toute crate ajoutée au-dessus hérite de cette cible**, avec sa feature,
//! son `required-features` et la ligne `-p` de `ci/measure-wasm.sh`.
//!
//! Les cinq plugins sont montés, et la CI exige chacun d'eux ici : un plugin
//! absent de cette liste est un plugin que la mesure ne voit pas.
//!
//! # Pourquoi une cible binaire
//!
//! Une crate bibliothèque compilée pour `wasm32-unknown-unknown` produit une
//! `rlib`, pas un `.wasm`. Sans binaire lié, le linker n'élimine rien : la
//! mesure porterait sur du code mort et ne voudrait rien dire. C'est le graphe
//! de systèmes **réel** qui décide de ce que le linker conserve.
//!
//! # Pourquoi elle est derrière une feature
//!
//! Sans garde, cette cible serait construite par `cargo build --workspace` et
//! `cargo test --workspace`, à chaque ticket, pour lier un binaire Bevy complet
//! dont personne n'a besoin. `required-features` la réserve à qui la demande.

use bevy::prelude::*;
use game_state::GameStatePlugin;
use shop_system::ShopPlugin;
use ui_and_juice::JuicePlugin;
use ui_and_juice::graphics::VisualEffectsPlugin;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            GameStatePlugin,
            JuicePlugin,
            VisualEffectsPlugin,
            ShopPlugin,
        ))
        .run();
}
