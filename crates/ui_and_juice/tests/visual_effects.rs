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
use ui_and_juice::settings::{CrtSettings, JuiceSettings, SafeMode};

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

/// Une application headless avec le seul plugin de l'étape.
fn app_headless() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, VisualEffectsPlugin));
    app
}

/// Le plugin pose `CrtSettings`, et le défaut est le filtre allumé, à pleine
/// intensité, avec quatre intensités unitaires strictement positives. Un
/// `derive(Default)` aurait donné un filtre éteint, sans la moindre erreur.
#[test]
fn test_crt_settings_defaults() {
    let mut app = app_headless();
    app.update();

    let crt = app.world().resource::<CrtSettings>();
    assert!(crt.enabled);
    assert_eq!(crt.intensity, 1.0);
    assert!(crt.curvature > 0.0);
    assert!(crt.scanline_intensity > 0.0);
    assert!(crt.vignette_roundness > 0.0);
    assert!(crt.chromatic_aberration > 0.0);
}

/// Le plugin pose `SafeMode`, et le mode dégradé ne s'active jamais tout seul
/// au premier lancement.
#[test]
fn test_safe_mode_defaults_to_disabled() {
    let mut app = app_headless();
    app.update();

    assert!(!app.world().resource::<SafeMode>().enabled);
}

/// Les deux ressources insérées avec des valeurs modifiées se relisent à
/// l'identique après trois mises à jour, et `JuiceSettings`, posée à côté,
/// n'écrase ni l'une ni l'autre : trois ressources, trois valeurs, aucun
/// système ne les réécrit.
#[test]
fn test_visual_settings_roundtrip_headless() {
    let mut app = app_headless();
    app.insert_resource(CrtSettings {
        enabled: false,
        intensity: 0.4,
        curvature: 0.2,
        scanline_intensity: 0.3,
        vignette_roundness: 0.6,
        chromatic_aberration: 0.01,
    });
    app.insert_resource(SafeMode { enabled: true });
    app.insert_resource(JuiceSettings {
        flash_intensity: 0.5,
        shake_intensity: 0.0,
    });

    app.update();
    app.update();
    app.update();

    let crt = app.world().resource::<CrtSettings>();
    assert!(!crt.enabled);
    assert_eq!(crt.intensity, 0.4);
    assert_eq!(crt.curvature, 0.2);
    assert_eq!(crt.scanline_intensity, 0.3);
    assert_eq!(crt.vignette_roundness, 0.6);
    assert_eq!(crt.chromatic_aberration, 0.01);
    assert!(app.world().resource::<SafeMode>().enabled);
    let juice = app.world().resource::<JuiceSettings>();
    assert_eq!(juice.flash_intensity, 0.5);
    assert_eq!(juice.shake_intensity, 0.0);
}

/// Contrat documenté : `intensity == 0.0` et `enabled == false` produisent le
/// **même** résultat d'ordonnancement, une passe absente, et le mode dégradé
/// aussi. Ici c'est le prédicat qui est asséré ; l'ordonnancement lui-même
/// est `test_crt_pass_not_scheduled_when_disabled` (TASK-88).
#[test]
fn test_zero_intensity_equals_disabled() {
    let hors_mode_degrade = SafeMode::default();
    let nominal = CrtSettings::default();
    assert!(nominal.is_active(&hors_mode_degrade));

    let coupe = CrtSettings {
        enabled: false,
        ..CrtSettings::default()
    };
    let nulle = CrtSettings {
        intensity: 0.0,
        ..CrtSettings::default()
    };
    assert!(!coupe.is_active(&hors_mode_degrade));
    assert!(!nulle.is_active(&hors_mode_degrade));
    assert_eq!(
        coupe.is_active(&hors_mode_degrade),
        nulle.is_active(&hors_mode_degrade)
    );

    assert!(!nominal.is_active(&SafeMode { enabled: true }));
}
