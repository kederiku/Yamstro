//! Contrat de la crate audio. Tests sans périphérique : ils vérifient des
//! décisions, jamais du son. Ce fichier est celui que le document de l'Étape 8
//! nomme ; chaque ticket de l'étape y ajoute les siens.

use std::{collections::BTreeSet, path::PathBuf, process::Command};

use audio_system::GameAudioPlugin;
use bevy::{prelude::*, state::app::StatesPlugin};
use ui_and_juice::JuicePlugin;

fn racine() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn arbre(arguments: &[&str]) -> String {
    let sortie = Command::new(env!("CARGO"))
        .arg("tree")
        .args(arguments)
        .current_dir(racine())
        .output()
        .expect("cargo tree");
    assert!(
        sortie.status.success(),
        "cargo tree a échoué : {arguments:?}"
    );
    String::from_utf8_lossy(&sortie.stdout).into_owned()
}

// ------------------------------------------------------------------ TASK-96

#[test]
fn test_audio_plugin_boots_headless() {
    // `MinimalPlugins` n'inclut pas `StatesPlugin` : il s'ajoute à la main,
    // comme dans tous les tests sans fenêtre depuis l'Étape 3.
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, GameAudioPlugin));
    for _ in 0..3 {
        app.update();
    }
}

#[test]
fn test_no_symphonia_vorbis_in_tree() {
    // **Le contrôle porte sur la feature de Bevy, jamais sur le nom des crates.**
    // Cette feature fait passer le lecteur de Bevy sur un autre décodeur ; le
    // backend retenu, lui, décode par des crates dont le nom commence de la même
    // façon, et elles sont **attendues** dans cet arbre. Un filtre sur le nom
    // conclurait à une violation et ferait retirer le backend qu'on vient de
    // retenir : c'est le seul faux positif de l'étape.
    let arbre = arbre(&["-e", "features", "-p", "audio_system"]);
    assert!(arbre.contains("bevy_seedling"), "l'arbre n'est pas lu");
    assert!(
        arbre.contains("symphonia-codec-vorbis"),
        "le décodeur du backend a quitté l'arbre : la branche A ne lit plus ses fichiers"
    );

    let proscrite = format!("feature \"{}-{}\"", "symphonia", "vorbis");
    assert!(
        !arbre.contains(&proscrite),
        "la feature proscrite est activée"
    );
    // Branche A : ni le lecteur de Bevy, ni ce qu'il tire. Le backend les refuse.
    for absent in ["bevy_audio", "rodio", "lewton"] {
        assert!(!arbre.contains(absent), "`{absent}` est entré dans l'arbre");
    }
}

#[test]
fn test_juice_plugin_still_registers_no_audio_resource() {
    // L'inventaire de la mise en scène, vu d'ici : trois ressources, celles que
    // TASK-46 compte, et pas une de plus. La crate audio peut nommer ce plugin ;
    // l'inverse fermerait un cycle.
    let mut base = App::new();
    base.add_plugins(MinimalPlugins);
    let avant = base.world().iter_resources().count();

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, JuicePlugin));
    let apres = app.world().iter_resources().count();

    assert_eq!(apres - avant, 3, "inventaire de JuicePlugin");
}

#[test]
fn test_crate_graph_has_no_cycle() {
    let audio = arbre(&["-p", "audio_system", "-e", "normal"]);
    assert!(
        audio.contains("ui_and_juice"),
        "l'arbre de la crate audio n'est pas lu"
    );
    assert!(
        !audio.contains("shop_system"),
        "la crate audio dépend de la boutique : elles sont sœurs"
    );

    for amont in ["core_engine", "game_state", "ui_and_juice", "shop_system"] {
        let arbre = arbre(&["-p", amont, "-e", "normal"]);
        assert!(
            arbre.contains("core_engine"),
            "l'arbre de `{amont}` n'est pas lu"
        );
        assert!(
            !arbre.contains("audio_system"),
            "`{amont}` dépend de la crate audio : le sens est inversé"
        );
    }
}

#[test]
fn test_audio_module_files_are_the_six() {
    let sources = racine().join("crates/audio_system/src");
    let fichiers: BTreeSet<String> = std::fs::read_dir(&sources)
        .expect("répertoire des sources")
        .map(|entree| {
            entree
                .expect("entrée")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let attendus: BTreeSet<String> = [
        "backend.rs",
        "bus.rs",
        "lib.rs",
        "music.rs",
        "pitch.rs",
        "sfx.rs",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(
        fichiers, attendus,
        "l'arborescence de l'étape est de six fichiers"
    );

    let lib = std::fs::read_to_string(sources.join("lib.rs")).expect("lib.rs");
    for module in ["backend", "bus", "music", "pitch", "sfx"] {
        assert!(
            lib.contains(&format!("\npub mod {module};\n")),
            "`pub mod {module};` manque dans lib.rs"
        );
    }
}
