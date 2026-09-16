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

use std::time::Duration;

use bevy::asset::{AssetEvent, AssetPlugin};
use bevy::mesh::{MeshPlugin, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;
use bevy::render::sync_world::SyncWorldPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PrimaryWindow, WindowPlugin, WindowResized};
use core_engine::blinds::{BlindContext, BlindDefinition, BlindType};
use core_engine::hands::HandGrid;
use game_state::RunPhase;
use naga_oil::compose::{
    ComposableModuleDescriptor, Composer, NagaModuleDescriptor, ShaderDefValue, ShaderLanguage,
};
use ui_and_juice::graphics::VisualEffectsPlugin;
use ui_and_juice::graphics::background::{BackgroundMaterial, BackgroundQuad, BackgroundUniform};
use ui_and_juice::graphics::crt::{CrtMaterial, CrtUniform, crt_pass_wanted};
use ui_and_juice::graphics::holo::{HoloMaterials, HoloOutlineMaterial, HoloUniform};
use ui_and_juice::graphics::plugin::SHADER_PATHS;
use ui_and_juice::graphics::theme::{BOSS, SHOP, SMALL, ThemePalette, VisualThemeController};
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
/// fenêtre primaire que le quad de fond lit, et la synchronisation des
/// entités vers le monde de rendu, dont `ExtractComponentPlugin` pose les
/// crochets : sans elle, retirer un `CrtMaterial` panique sur la ressource
/// `PendingSyncEntity` absente (`bevy_render-0.19.1/src/sync_component.rs:55`).
/// C'est ce que `DefaultPlugins` monte, sans le rendu ni winit ; la fenêtre
/// par défaut fait 1280 × 720 logiques.
fn app_headless() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        WindowPlugin::default(),
        SyncWorldPlugin,
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

/// Le pas de temps des tests du thème : 10 ms par mise à jour, exacts en
/// nanosecondes, donc 150 pas font exactement 1,5 s.
const STEP: Duration = Duration::from_millis(10);

/// Le nombre de `Modified` du matériau du fond lus depuis la dernière remise
/// à zéro, par un lecteur dédié qui voit chaque message exactement une fois.
///
/// Mesuré : lire `Messages` « de la frame courante » ne marche pas ici, le
/// renouvellement des messages étant cadencé par `TimePlugin` sur le pas fixe
/// (`bevy_time-0.19.1/src/lib.rs:98`), si bien que les mêmes messages
/// restent visibles plusieurs frames. Un lecteur, lui, a son curseur.
#[derive(Resource, Default)]
struct ModifiedCount(usize);

fn count_modified(
    mut reader: MessageReader<AssetEvent<BackgroundMaterial>>,
    mut count: ResMut<ModifiedCount>,
) {
    count.0 += reader
        .read()
        .filter(|e| matches!(e, AssetEvent::Modified { .. }))
        .count();
}

/// Une application headless, démarrée, au temps piloté par pas de 10 ms, avec
/// le compteur de `Modified` remis à zéro après la frame de démarrage : la
/// création du matériau émet un `Added` **et** un `Modified`
/// (`bevy_asset-0.19.1/src/assets.rs:391`), qui ne comptent pas comme une
/// écriture au repos.
fn app_stepped() -> App {
    let mut app = app_headless();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
    app.init_resource::<ModifiedCount>();
    app.add_systems(Last, count_modified);
    app.update();
    app.world_mut().resource_mut::<ModifiedCount>().0 = 0;
    app
}

/// La palette portée par le matériau du fond, lue dans `Assets`.
fn material_palette(app: &App) -> ThemePalette {
    let controller = app.world().resource::<VisualThemeController>();
    let materials = app.world().resource::<Assets<BackgroundMaterial>>();
    let params = &materials
        .get(&controller.handle)
        .expect("le matériau du fond existe")
        .params;
    ThemePalette {
        primary: params.primary_color,
        secondary: params.secondary_color,
        accent: params.accent_color,
        speed: params.speed,
        swirl_factor: params.swirl_factor,
    }
}

/// Entre en run avec une manche du type donné, en phase `Roll`.
fn enter_blind(app: &mut App, kind: BlindType) {
    app.insert_resource(State::new(RunPhase::Roll));
    app.insert_resource(BlindContext {
        blind: BlindDefinition {
            kind,
            target_score: 300,
            reward: 3,
            modifier: None,
        },
        target_score: 300,
        current_score: 0,
        hands_remaining: 4,
        used_hands: HandGrid::default(),
    });
}

/// Les `Modified` comptés depuis le dernier appel, et remise à zéro.
fn take_modified(app: &mut App) -> usize {
    std::mem::take(&mut app.world_mut().resource_mut::<ModifiedCount>().0)
}

fn elapsed_secs(app: &App) -> f32 {
    app.world()
        .resource::<VisualThemeController>()
        .timer
        .elapsed_secs()
}

/// Cent vingt frames sans changement de manche : zéro `Modified` sur le
/// matériau du fond. Le contrôleur naît au repos, le minuteur terminé, et le
/// système rend la main avant tout `get_mut`.
#[test]
fn test_no_material_write_when_idle() {
    let mut app = app_stepped();
    for _ in 0..120 {
        app.update();
    }
    assert_eq!(take_modified(&mut app), 0);
    assert!(
        app.world()
            .resource::<VisualThemeController>()
            .timer
            .is_finished()
    );
}

/// Cible changée à `t = 0`, frames avancées : à 1,49 s la palette diffère de
/// la cible ; à exactement 1,5 s elle l'atteint ; ensuite, plus une écriture.
#[test]
fn test_palette_transition_lasts_1_5s() {
    let mut app = app_stepped();
    enter_blind(&mut app, BlindType::Boss);

    for _ in 0..149 {
        app.update();
    }
    assert!((elapsed_secs(&app) - 1.49).abs() < 1e-6);
    assert_ne!(material_palette(&app), *BOSS, "pas avant 1,5 s");
    assert_eq!(
        take_modified(&mut app),
        149,
        "une écriture par frame de transition"
    );

    app.update();
    assert!((elapsed_secs(&app) - 1.5).abs() < 1e-6);
    assert_eq!(material_palette(&app), *BOSS, "exactement à 1,5 s");
    assert_eq!(take_modified(&mut app), 1, "la dernière écriture");

    for _ in 0..60 {
        app.update();
    }
    assert_eq!(
        take_modified(&mut app),
        0,
        "plus aucune écriture une fois le minuteur terminé"
    );
}

/// Au menu principal, sans `BlindContext` ni `State<RunPhase>`, le système
/// tourne et retombe sur la Petite Mise : parti d'une Boss, il y revient.
#[test]
fn test_theme_controller_survives_missing_blind_context() {
    let mut app = app_stepped();
    assert!(!app.world().contains_resource::<BlindContext>());
    assert!(!app.world().contains_resource::<State<RunPhase>>());
    {
        let mut controller = app.world_mut().resource_mut::<VisualThemeController>();
        controller.from = *BOSS;
        controller.to = *BOSS;
    }

    app.update();
    let controller = app.world().resource::<VisualThemeController>();
    assert_eq!(
        controller.to, *SMALL,
        "le système a tourné et visé la Petite Mise"
    );
    assert_eq!(controller.from, *BOSS);

    for _ in 0..160 {
        app.update();
    }
    assert_eq!(material_palette(&app), *SMALL);
}

/// Petite → Boss, 0,5 s, puis → Boutique : le nouveau `from` est la palette
/// courante à 0,5 s, pas l'ancien `from`, et le matériau ne saute pas.
#[test]
fn test_interrupted_transition_restarts_from_current() {
    let mut app = app_stepped();
    enter_blind(&mut app, BlindType::Boss);
    for _ in 0..50 {
        app.update();
    }
    assert!((elapsed_secs(&app) - 0.5).abs() < 1e-6);
    let courante = app.world().resource::<VisualThemeController>().current();
    assert_ne!(courante, *SMALL);
    assert_ne!(courante, *BOSS);
    assert_eq!(material_palette(&app), courante);

    app.insert_resource(State::new(RunPhase::Shop));
    app.update();

    let controller = app.world().resource::<VisualThemeController>();
    assert_eq!(controller.to, *SHOP);
    assert_eq!(controller.from, courante, "repart de la palette affichée");
    let ecart = (material_palette(&app).speed - courante.speed).abs();
    assert!(ecart < 0.01, "aucun saut : {ecart}");
}

/// Petite → Boss, transition complète : `swirl_factor` passe de 0,9 à 2,1 et
/// vaut 1,5 à mi-parcours.
#[test]
fn test_small_to_boss_varies_swirl() {
    let mut app = app_stepped();
    assert_eq!(material_palette(&app).swirl_factor, 0.9);
    enter_blind(&mut app, BlindType::Boss);
    for _ in 0..75 {
        app.update();
    }
    assert!((elapsed_secs(&app) - 0.75).abs() < 1e-6);
    assert!((material_palette(&app).swirl_factor - 1.5).abs() <= 1e-6);
    for _ in 0..75 {
        app.update();
    }
    assert_eq!(material_palette(&app).swirl_factor, 2.1);
}

/// À `t = 0,5`, chaque canal et chaque `f32` du matériau valent la moyenne
/// de `from` et `to`, en espace linéaire.
#[test]
fn test_interpolation_is_linear_in_linear_space() {
    let mut app = app_stepped();
    enter_blind(&mut app, BlindType::Boss);
    for _ in 0..75 {
        app.update();
    }
    let milieu = material_palette(&app);
    let moyenne = |a: f32, b: f32| (a + b) / 2.0;
    let canaux = |c: LinearRgba| [c.red, c.green, c.blue, c.alpha];
    for (couleur, de, vers) in [
        (milieu.primary, SMALL.primary, BOSS.primary),
        (milieu.secondary, SMALL.secondary, BOSS.secondary),
        (milieu.accent, SMALL.accent, BOSS.accent),
    ] {
        for ((x, a), b) in canaux(couleur).iter().zip(canaux(de)).zip(canaux(vers)) {
            assert!(
                (x - moyenne(a, b)).abs() <= 1e-6,
                "{x} contre {}",
                moyenne(a, b)
            );
        }
    }
    assert!((milieu.speed - moyenne(SMALL.speed, BOSS.speed)).abs() <= 1e-6);
    assert!((milieu.swirl_factor - moyenne(SMALL.swirl_factor, BOSS.swirl_factor)).abs() <= 1e-6);
}

/// Le nombre de frames où une sonde sous `run_if(crt_pass_wanted)` a tourné.
#[derive(Resource, Default)]
struct ProbeRuns(usize);

fn crt_probe(mut runs: ResMut<ProbeRuns>) {
    runs.0 += 1;
}

/// Le nombre de frames où le matériau du filtre a changé sur une caméra,
/// insertion comprise.
#[derive(Resource, Default)]
struct CrtWrites(usize);

fn count_crt_writes(changed: Query<(), Changed<CrtMaterial>>, mut writes: ResMut<CrtWrites>) {
    writes.0 += changed.iter().count();
}

/// L'application de l'étape avec une caméra 2D, la sonde de passe et le
/// compteur d'écritures, **avant** le démarrage : les réglages se posent
/// ensuite, puis `update()`.
fn app_with_camera() -> App {
    let mut app = app_headless();
    app.init_resource::<ProbeRuns>();
    app.init_resource::<CrtWrites>();
    app.add_systems(Update, crt_probe.run_if(crt_pass_wanted));
    app.add_systems(Last, count_crt_writes);
    app.world_mut().spawn(Camera2d);
    app
}

fn camera_crt(app: &mut App) -> Option<CrtMaterial> {
    app.world_mut()
        .query_filtered::<Option<&CrtMaterial>, With<Camera2d>>()
        .single(app.world())
        .expect("une seule caméra")
        .copied()
}

/// Seize octets pile pour le bloc, et pour le matériau qui le porte :
/// quatre `f32`, aucun padding, pas seulement un multiple de seize.
#[test]
fn test_crt_uniform_is_exactly_16_bytes() {
    assert_eq!(CrtUniform::min_size().get(), 16);
    assert_eq!(CrtMaterial::min_size().get(), 16);
}

/// Filtre coupé, intensité nulle, ou mode dégradé : dans les trois cas la
/// condition rend faux, la sonde ne tourne pas sur 60 frames, et la caméra ne
/// porte aucun matériau : la passe n'est pas ordonnancée.
#[test]
fn test_crt_pass_not_scheduled_when_disabled() {
    let coupe = CrtSettings {
        enabled: false,
        ..CrtSettings::default()
    };
    let nulle = CrtSettings {
        intensity: 0.0,
        ..CrtSettings::default()
    };
    let cas = [
        (coupe, SafeMode::default()),
        (nulle, SafeMode::default()),
        (CrtSettings::default(), SafeMode { enabled: true }),
    ];
    for (settings, safe_mode) in cas {
        assert!(!settings.is_active(&safe_mode));
        let mut app = app_with_camera();
        app.insert_resource(settings);
        app.insert_resource(safe_mode);
        for _ in 0..60 {
            app.update();
        }
        assert_eq!(app.world().resource::<ProbeRuns>().0, 0);
        assert!(
            camera_crt(&mut app).is_none(),
            "aucun matériau sur la caméra"
        );
    }
}

/// Réglages nominaux, hors mode dégradé : la sonde tourne à chaque frame et
/// la caméra porte le matériau écrit depuis les réglages.
#[test]
fn test_crt_pass_scheduled_when_enabled() {
    let mut app = app_with_camera();
    for _ in 0..60 {
        app.update();
    }
    assert_eq!(app.world().resource::<ProbeRuns>().0, 60);
    let material = camera_crt(&mut app).expect("le matériau est sur la caméra");
    let attendu = CrtUniform::from_settings(&CrtSettings::default());
    assert_eq!(material.params.curvature, attendu.curvature);
    assert_eq!(
        material.params.scanline_intensity,
        attendu.scanline_intensity
    );
    assert_eq!(
        material.params.vignette_roundness,
        attendu.vignette_roundness
    );
    assert_eq!(
        material.params.chromatic_aberration,
        attendu.chromatic_aberration
    );
    assert!(attendu.curvature > 0.0);
}

/// `intensity == 0.5` : les quatre champs du matériau valent la moitié des
/// réglages ; `intensity == 0.0` les met tous à zéro.
#[test]
fn test_intensity_scales_all_four_fields() {
    let mut app = app_with_camera();
    app.insert_resource(CrtSettings {
        intensity: 0.5,
        ..CrtSettings::default()
    });
    app.update();
    let reglages = CrtSettings::default();
    let params = camera_crt(&mut app).expect("matériau présent").params;
    assert_eq!(params.curvature, reglages.curvature * 0.5);
    assert_eq!(params.scanline_intensity, reglages.scanline_intensity * 0.5);
    assert_eq!(params.vignette_roundness, reglages.vignette_roundness * 0.5);
    assert_eq!(
        params.chromatic_aberration,
        reglages.chromatic_aberration * 0.5
    );

    let zero = CrtUniform::from_settings(&CrtSettings {
        intensity: 0.0,
        ..CrtSettings::default()
    });
    assert_eq!(zero.curvature, 0.0);
    assert_eq!(zero.scanline_intensity, 0.0);
    assert_eq!(zero.vignette_roundness, 0.0);
    assert_eq!(zero.chromatic_aberration, 0.0);
}

/// Cent vingt frames sans toucher aux réglages : le matériau, écrit une fois
/// à son insertion, n'est plus jamais réécrit.
#[test]
fn test_no_uniform_write_when_settings_unchanged() {
    let mut app = app_with_camera();
    app.update();
    assert_eq!(app.world().resource::<CrtWrites>().0, 1, "l'insertion");
    app.world_mut().resource_mut::<CrtWrites>().0 = 0;
    for _ in 0..120 {
        app.update();
    }
    assert_eq!(app.world().resource::<CrtWrites>().0, 0);
}

/// Un changement d'`intensity` : exactement une frame où le matériau change,
/// et les quatre champs suivent.
#[test]
fn test_uniform_written_once_on_change() {
    let mut app = app_with_camera();
    app.update();
    app.world_mut().resource_mut::<CrtWrites>().0 = 0;

    app.world_mut().resource_mut::<CrtSettings>().intensity = 0.5;
    app.update();
    assert_eq!(app.world().resource::<CrtWrites>().0, 1);
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<CrtWrites>().0,
        1,
        "une seule écriture"
    );
    let params = camera_crt(&mut app).expect("matériau présent").params;
    assert_eq!(params.curvature, CrtSettings::default().curvature * 0.5);
}

/// Activé puis coupé, le matériau disparaît de la caméra ; réactivé, il
/// revient.
#[test]
fn test_crt_material_removed_when_disabled() {
    let mut app = app_with_camera();
    app.update();
    assert!(camera_crt(&mut app).is_some());

    app.world_mut().resource_mut::<SafeMode>().enabled = true;
    app.update();
    assert!(camera_crt(&mut app).is_none(), "retiré en mode dégradé");

    app.world_mut().resource_mut::<SafeMode>().enabled = false;
    app.update();
    assert!(camera_crt(&mut app).is_some(), "revenu");
}

/// Le bouchon de `bevy_core_pipeline::fullscreen_vertex_shader` : la struct
/// exacte de `fullscreen.wgsl:3-8`, deux champs.
const FULLSCREEN_VERTEX_STUB: &str =
    "#define_import_path bevy_core_pipeline::fullscreen_vertex_shader
struct FullscreenVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
";

/// Le bouchon de `bevy_sprite::mesh2d_vertex_output` : les quatre champs
/// inconditionnels de `mesh2d_vertex_output.wgsl`.
const MESH2D_VERTEX_OUTPUT_STUB: &str = "#define_import_path bevy_sprite::mesh2d_vertex_output
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};
";

/// Le bouchon de `bevy_sprite::mesh2d_view_bindings`, réduit à ce que le
/// vortex lit : `view.viewport` et `globals.time`, aux bindings réels.
const MESH2D_VIEW_BINDINGS_STUB: &str = "#define_import_path bevy_sprite::mesh2d_view_bindings
struct View { viewport: vec4<f32>, };
struct Globals { time: f32, };
@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> globals: Globals;
";

/// Compose un shader de `assets/shaders/` avec des bouchons des bibliothèques
/// de Bevy, comme Bevy le fait à la création du pipeline, puis le valide par
/// naga. Les bouchons déclarent les interfaces que le shader emploie, pas la
/// disposition réelle de Bevy : c'est la syntaxe, les types et l'uniformité
/// du shader qui sont validés, avec les versions de naga que Bevy embarque.
fn compose_and_validate(shader: &str, stubs: &[&str], defs: &[(&str, u32)]) -> naga::Module {
    let chemin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets")
        .join(shader);
    let source = std::fs::read_to_string(&chemin).expect("le shader existe");
    let mut composer = Composer::default();
    for stub in stubs {
        let resultat = composer.add_composable_module(ComposableModuleDescriptor {
            source: stub,
            file_path: "bouchon.wgsl",
            language: ShaderLanguage::Wgsl,
            ..Default::default()
        });
        if let Err(e) = resultat {
            panic!("bouchon : {}", e.emit_to_string(&composer));
        }
    }
    let shader_defs = defs
        .iter()
        .map(|(nom, valeur)| ((*nom).to_string(), ShaderDefValue::UInt(*valeur)))
        .collect();
    let module = match composer.make_naga_module(NagaModuleDescriptor {
        source: &source,
        file_path: shader,
        shader_defs,
        ..Default::default()
    }) {
        Ok(module) => module,
        Err(e) => panic!("{shader} : {}", e.emit_to_string(&composer)),
    };
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{shader} invalide : {e:?}"));
    module
}

fn has_fragment_entry(module: &naga::Module) -> bool {
    module
        .entry_points
        .iter()
        .any(|e| e.stage == naga::ShaderStage::Fragment && e.name == "fragment")
}

/// Le shader du filtre compose avec la sortie du triangle plein écran et
/// passe le validateur de naga : syntaxe, types, uniformité, un point
/// d'entrée fragment.
#[test]
fn test_crt_shader_composes_and_validates() {
    let module = compose_and_validate(
        "shaders/crt_postprocess.wgsl",
        &[FULLSCREEN_VERTEX_STUB],
        &[],
    );
    assert!(has_fragment_entry(&module));
}

/// Le shader du vortex compose avec la sortie du vertex 2D et les bindings de
/// vue, le def `MATERIAL_BIND_GROUP` valant 2 comme Bevy le passe, et passe
/// le validateur.
#[test]
fn test_background_shader_composes_and_validates() {
    let module = compose_and_validate(
        "shaders/psyche_background.wgsl",
        &[MESH2D_VERTEX_OUTPUT_STUB, MESH2D_VIEW_BINDINGS_STUB],
        &[("MATERIAL_BIND_GROUP", 2)],
    );
    assert!(has_fragment_entry(&module));
}

/// Le facteur de scanline du shader, répliqué : à `scanline_intensity == 0`,
/// il vaut exactement 1, bit à bit, sur tout un échantillon de `uv.y`. Pas
/// d'atténuation résiduelle quand le réglage est nul.
#[test]
fn test_scanline_is_identity_at_zero() {
    // Le littéral `3.14159265` du shader est `PI` une fois arrondi en f32.
    let facteur = |uv_y: f32, hauteur: f32, intensite: f32| {
        let s = (uv_y * hauteur * core::f32::consts::PI).sin();
        1.0 - intensite * 0.5 * (1.0 - s * s)
    };
    for i in 0..=100 {
        let uv_y = i as f32 / 100.0;
        assert_eq!(facteur(uv_y, 720.0, 0.0).to_bits(), 1.0_f32.to_bits());
    }
    assert!(facteur(0.5, 720.0, 0.25) <= 1.0);
}

/// Les `Added` et `Modified` des contours lus par un lecteur dédié, chaque
/// message une fois.
#[derive(Resource, Default)]
struct HoloEvents {
    added: usize,
    modified: usize,
}

fn count_holo_events(
    mut reader: MessageReader<AssetEvent<HoloOutlineMaterial>>,
    mut count: ResMut<HoloEvents>,
) {
    for event in reader.read() {
        match event {
            AssetEvent::Added { .. } => count.added += 1,
            AssetEvent::Modified { .. } => count.modified += 1,
            _ => {}
        }
    }
}

/// Le bloc du contour fait 32 octets, multiple de 16 : une couleur, trois
/// `f32`, et le remplissage explicite. Les deux assertions, pas la première
/// seule.
#[test]
fn test_holo_uniform_is_16_byte_aligned() {
    let taille = HoloUniform::min_size().get();
    assert_eq!(taille % 16, 0, "bloc non aligné sur 16 octets : {taille}");
    assert_eq!(taille, 32, "bloc de {taille} octets, 32 attendus");
}

/// Après le démarrage, la banque porte exactement huit handles deux à deux
/// distincts, tous présents dans `Assets`, indexés par état et balayage : la
/// seule variante masquée est `HIDDEN`, et l'irisé ne diffère que par
/// `rainbow_shift`.
#[test]
fn test_holo_bank_has_eight_distinct_die_handles() {
    let mut app = app_headless();
    app.update();

    let bank = app.world().resource::<HoloMaterials>().clone();
    let ids: BTreeSet<_> = bank.dice.iter().map(|h| h.id()).collect();
    assert_eq!(ids.len(), 8, "huit handles deux à deux distincts");

    let materials = app.world().resource::<Assets<HoloOutlineMaterial>>();
    assert_eq!(materials.len(), 8);
    for state in HoloMaterials::IDLE..=HoloMaterials::HIDDEN {
        for iridescent in [false, true] {
            let material = materials
                .get(bank.die(state, iridescent))
                .expect("chaque variante est dans Assets");
            let params = &material.params;
            assert_eq!(params.rainbow_shift, if iridescent { 1.0 } else { 0.0 });
            assert_eq!(
                params.mask_face,
                if state == HoloMaterials::HIDDEN {
                    1.0
                } else {
                    0.0
                }
            );
            assert_eq!(params._pad, 0.0);
            assert!(params.outline_width > 0.0);
            assert_eq!(params.outline_color.alpha, 1.0);
            assert!(material.texture.is_none(), "aucun atlas n'existe encore");
        }
    }
    let uni = &materials
        .get(bank.die(HoloMaterials::SCORING, false))
        .unwrap()
        .params;
    let irise = &materials
        .get(bank.die(HoloMaterials::SCORING, true))
        .unwrap()
        .params;
    assert_eq!(uni.outline_color, irise.outline_color);
    assert_eq!(uni.outline_width, irise.outline_width);
}

/// Huit `Added` au démarrage, et zéro `Modified` sur 120 frames : la banque
/// est construite une fois et jamais réécrite.
#[test]
fn test_holo_bank_is_never_modified() {
    let mut app = app_headless();
    app.init_resource::<HoloEvents>();
    app.add_systems(Last, count_holo_events);
    app.update();
    assert_eq!(app.world().resource::<HoloEvents>().added, 8);
    for _ in 0..120 {
        app.update();
    }
    let events = app.world().resource::<HoloEvents>();
    assert_eq!(events.added, 8);
    assert_eq!(events.modified, 0);
}
