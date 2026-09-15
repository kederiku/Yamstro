//! Les effets visuels de l'Étape 7, vus de l'extérieur.
//!
//! **Sous `tests/`, comme le document source le dessine** : un fichier pour
//! toute l'étape, où chaque ticket ajoute les siens. Un test d'intégration ne
//! voit que l'API publique de la crate, et c'est exactement ce que le plugin
//! expose : lui-même, et les chemins des trois shaders.
//!
//! **Headless, sans plugin de rendu.** `MinimalPlugins` ne monte ni le serveur
//! d'assets ni le rendu ; le type `Shader` n'y est enregistré par personne. Le
//! plugin doit donc démarrer sans rien réclamer de ce qu'il n'a pas.

use std::collections::BTreeSet;
use std::path::Path;

use bevy::prelude::*;
use ui_and_juice::graphics::VisualEffectsPlugin;
use ui_and_juice::graphics::plugin::SHADER_PATHS;

/// Trois mises à jour, aucune panique : le plugin se monte seul, sans état,
/// sans assets et sans rendu.
#[test]
fn test_visual_effects_plugin_boots_headless() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, VisualEffectsPlugin));

    app.update();
    app.update();
    app.update();
}

/// Les trois fichiers existent, **aux chemins que le plugin publie**, sous la
/// racine `assets/` du dépôt.
///
/// Le test lit les constantes et non trois littéraux recopiés : renommer un
/// fichier sans toucher la constante, ou l'inverse, le fait tomber. Les noms
/// attendus sont ceux du document source, et ils sont assérés en plus, sinon
/// trois constantes pointant sur trois fichiers quelconques passeraient.
#[test]
fn test_three_shader_files_exist() {
    let racine = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets");

    let mut noms = BTreeSet::new();
    for chemin in SHADER_PATHS {
        assert!(
            chemin.starts_with("shaders/") && chemin.ends_with(".wgsl"),
            "chemin d'asset attendu sous `shaders/`, en `.wgsl` : {chemin}"
        );
        let fichier = racine.join(chemin);
        assert!(fichier.is_file(), "shader absent : {}", fichier.display());
        noms.insert(chemin.trim_start_matches("shaders/"));
    }

    assert_eq!(
        noms,
        BTreeSet::from([
            "crt_postprocess.wgsl",
            "holo_card.wgsl",
            "psyche_background.wgsl"
        ])
    );
}
