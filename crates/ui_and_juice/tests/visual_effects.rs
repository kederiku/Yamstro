//! Les effets visuels de l'Étape 7, vus de l'extérieur.
//!
//! **Sous `tests/`, comme le document source le dessine** : un fichier pour
//! toute l'étape, où chaque ticket ajoute les siens. Un test d'intégration ne
//! voit que l'API publique de la crate, et c'est exactement ce que le plugin
//! expose : lui-même, et les chemins des trois shaders.
//!
//! **Headless, sans plugin de rendu.** `MinimalPlugins` ne monte ni le serveur
//! d'assets ni le rendu ; le type `Shader` n'y est enregistré par personne.
//! Depuis TASK-84, le plugin enregistre un matériau, et `Material2dPlugin`
//! appelle `init_asset`, qui lit `AssetServer` : les tests montent donc
//! `AssetPlugin`, et toujours rien du rendu. Le plugin ne réclame rien de plus.

use std::collections::BTreeSet;
use std::path::Path;

use bevy::asset::AssetPlugin;
use bevy::mesh::{MeshPlugin, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;
use bevy::window::{PrimaryWindow, WindowPlugin, WindowResized};
use ui_and_juice::graphics::VisualEffectsPlugin;
use ui_and_juice::graphics::background::{BackgroundMaterial, BackgroundQuad, BackgroundUniform};
use ui_and_juice::graphics::plugin::SHADER_PATHS;
use ui_and_juice::settings::{CrtSettings, JuiceSettings, SafeMode};

/// Trois mises à jour, aucune panique : le plugin se monte sans rendu, avec le
/// seul serveur d'assets.
#[test]
fn test_visual_effects_plugin_boots_headless() {
    let mut app = app_headless();

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

/// Une application headless avec le plugin de l'étape : `MinimalPlugins`, le
/// serveur d'assets qu'exige tout `Material2dPlugin`, `Assets<Mesh>` et la
/// fenêtre primaire que le quad de fond lit, ce que `DefaultPlugins` monte
/// sans le rendu ni winit. La fenêtre par défaut fait 1280 × 720 logiques.
fn app_headless() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        WindowPlugin::default(),
        VisualEffectsPlugin,
    ));
    app
}

/// Le handle du maillage du fond et ses dimensions, lues sur les positions
/// des sommets.
fn quad_mesh(app: &mut App) -> (Handle<Mesh>, Vec2) {
    let handle = app
        .world_mut()
        .query_filtered::<&Mesh2d, With<BackgroundQuad>>()
        .single(app.world())
        .expect("une seule entité de fond")
        .0
        .clone();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mesh = meshes.get(&handle).expect("le maillage du fond existe");
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(VertexAttributeValues::as_float3)
        .expect("des positions de sommets");
    let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in positions {
        min = min.min(Vec2::new(p[0], p[1]));
        max = max.max(Vec2::new(p[0], p[1]));
    }
    (handle, max - min)
}

/// Écrit un `WindowResized` pour la fenêtre primaire, puis une mise à jour.
fn resize_window(app: &mut App, width: f32, height: f32) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .expect("une fenêtre primaire");
    app.world_mut().write_message(WindowResized {
        window,
        width,
        height,
    });
    app.update();
}

/// Le `Transform` du quad, copié.
fn quad_transform(app: &mut App) -> Transform {
    *app.world_mut()
        .query_filtered::<&Transform, With<BackgroundQuad>>()
        .single(app.world())
        .expect("une seule entité de fond")
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

/// Le bloc du vortex fait 64 octets, multiple de 16 : trois `LinearRgba`, deux
/// `f32`, et le remplissage explicite. Les deux assertions, pas la première
/// seule : un bloc de 80 octets est aussi un multiple de 16, et un décalage
/// d'alignement ne lève aucune erreur, il produit des couleurs fausses.
#[test]
fn test_background_uniform_is_16_byte_aligned() {
    let taille = BackgroundUniform::min_size().get();
    assert_eq!(taille % 16, 0, "bloc non aligné sur 16 octets : {taille}");
    assert_eq!(taille, 64, "bloc de {taille} octets, 64 attendus");
}

/// Le plugin enregistre le matériau du fond : `Assets<BackgroundMaterial>`
/// existe après le montage. La garde de CI voit la ligne d'enregistrement ; ce
/// test voit qu'elle compile et qu'elle s'exécute.
#[test]
fn test_background_material_asset_is_registered() {
    let mut app = app_headless();
    app.update();

    assert!(
        app.world()
            .contains_resource::<Assets<BackgroundMaterial>>()
    );
}

/// Trois mises à jour, une seule entité de fond, à `z == -100.0`, aux
/// dimensions logiques de la fenêtre primaire.
#[test]
fn test_background_quad_spawned_once() {
    let mut app = app_headless();
    app.update();
    app.update();
    app.update();

    let fonds = app
        .world_mut()
        .query_filtered::<Entity, With<BackgroundQuad>>()
        .iter(app.world())
        .count();
    assert_eq!(fonds, 1, "une seule entité de fond, pour toute la partie");
    assert_eq!(quad_transform(&mut app).translation.z, -100.0);
    let (_, taille) = quad_mesh(&mut app);
    assert_eq!(taille, Vec2::new(1280.0, 720.0));
}

/// Un `WindowResized`, une mise à jour : les dimensions du maillage ont
/// changé et suivent le message.
#[test]
fn test_window_resize_rebuilds_mesh() {
    let mut app = app_headless();
    app.update();
    let (_, avant) = quad_mesh(&mut app);

    resize_window(&mut app, 640.0, 480.0);

    let (_, apres) = quad_mesh(&mut app);
    assert_ne!(avant, apres);
    assert_eq!(apres, Vec2::new(640.0, 480.0));
}

/// Même scénario : le `Transform` du quad est bit à bit identique avant et
/// après, `scale` comprise. Le redimensionnement passe par le maillage, jamais
/// par le `Transform` (raccord A).
#[test]
fn test_window_resize_never_writes_transform() {
    let mut app = app_headless();
    app.update();
    let avant = quad_transform(&mut app);

    resize_window(&mut app, 640.0, 480.0);

    let apres = quad_transform(&mut app);
    assert_eq!(avant, apres);
    let bits = |t: Transform| {
        let mut v: Vec<u32> = t
            .translation
            .to_array()
            .iter()
            .map(|f| f.to_bits())
            .collect();
        v.extend(t.rotation.to_array().iter().map(|f| f.to_bits()));
        v.extend(t.scale.to_array().iter().map(|f| f.to_bits()));
        v
    };
    assert_eq!(bits(avant), bits(apres));
}

/// Le maillage est réécrit en place : même handle avant et après, et pas un
/// maillage de plus dans `Assets<Mesh>`. Un échange de handle fuiterait un
/// maillage par événement.
#[test]
fn test_window_resize_keeps_mesh_handle() {
    let mut app = app_headless();
    app.update();
    let (avant, _) = quad_mesh(&mut app);
    let combien = app.world().resource::<Assets<Mesh>>().len();

    resize_window(&mut app, 640.0, 480.0);
    resize_window(&mut app, 640.0, 480.0);

    let (apres, taille) = quad_mesh(&mut app);
    assert_eq!(avant, apres, "le handle du maillage ne change pas");
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), combien);
    assert_eq!(taille, Vec2::new(640.0, 480.0), "idempotent");
}
