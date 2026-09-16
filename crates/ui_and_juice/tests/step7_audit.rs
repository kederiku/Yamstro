//! Audit de fin d'Étape 7 (TASK-94) : ce qui se vérifie mécaniquement, une
//! fois, en plus des gardes de CI de l'étape.
//!
//! **Ce fichier ne code pas, il vérifie.** Les invariants textuels du volet 1
//! du ticket sont des gardes ancrées du bloc « Invariants normatifs Étape 7 »
//! de la CI, sous-bloc par sous-bloc, TASK-82 à TASK-93 ; ils ne sont pas
//! dupliqués ici. Ce qui l'est : que les onze tests nommés du § 3 existent,
//! une fois chacun, et lisent leurs valeurs de référence dans leur corps ;
//! qu'aucun type de la crate ne dérive à la fois `Component` et `Resource` ;
//! que les écrivains de `Transform` sont toujours les deux de `animation.rs`
//! (raccord A) ; que le bloc de CI de l'étape porte ses douze sous-blocs et
//! que les gardes de l'Étape 4 sur `Transform` sont restées à la lettre ; et
//! que chaque type du bloc de glossaire livré (`reports/glossaire-etape-7.md`)
//! est déclaré une fois, et une seule, dans le code.
//!
//! Les recherches passent par `grep -rnE`, comme l'audit de l'Étape 6 bis :
//! `rg` n'est pas installé sur les exécuteurs de la CI. **Chaque orthographe
//! proscrite et chaque motif de garde s'assemblent de deux moitiés**, et aucun
//! n'est épelé : ce fichier est balayé par les gardes qu'il relit.

use std::path::{Path, PathBuf};
use std::process::Command;

const CE_FICHIER: &str = "crates/ui_and_juice/tests/step7_audit.rs";
const TESTS_DE_L_ETAPE: &str = "crates/ui_and_juice/tests/visual_effects.rs";
const THEME: &str = "crates/ui_and_juice/src/graphics/theme.rs";

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn lire(relatif: &str) -> String {
    let chemin = racine().join(relatif);
    std::fs::read_to_string(&chemin).unwrap_or_else(|erreur| panic!("{relatif} : {erreur}"))
}

/// Les lignes `chemin:numéro:texte` qui portent le motif, ce fichier exclu.
fn chercher(motif: &str, chemins: &[&str]) -> Vec<String> {
    let mut arguments = vec!["-rnE".to_string(), motif.to_string()];
    arguments.extend(chemins.iter().map(|chemin| (*chemin).to_string()));
    let sortie = Command::new("grep")
        .args(&arguments)
        .current_dir(racine())
        .output()
        .expect("grep");
    String::from_utf8_lossy(&sortie.stdout)
        .lines()
        .filter(|ligne| !ligne.starts_with(CE_FICHIER))
        .map(str::to_string)
        .collect()
}

/// Le corps d'un pas nommé du workflow, jusqu'au pas suivant.
fn bloc_ci(nom: &str) -> String {
    let ci = lire(".github/workflows/ci.yml");
    let marqueur = format!("- name: {nom}");
    let debut = ci
        .find(&marqueur)
        .unwrap_or_else(|| panic!("le pas « {nom} » a disparu du workflow"));
    let reste = &ci[debut + marqueur.len()..];
    let fin = reste.find("\n      - name:").unwrap_or(reste.len());
    reste[..fin].to_string()
}

/// Le motif d'une requête mutable sur `Transform`, assemblé pour ne pas être
/// épelé dans un fichier que la garde balaie.
fn motif_ecrivain() -> String {
    format!("Query<[^>]*&mut Trans{}", "form")
}

const TESTS_IMPOSES: [&str; 11] = [
    "test_background_uniform_is_16_byte_aligned",
    "test_crt_uniform_is_exactly_16_bytes",
    "test_no_material_write_when_idle",
    "test_palette_transition_lasts_1_5s",
    "test_palette_for_each_blind_type",
    "test_theme_controller_survives_missing_blind_context",
    "test_crt_pass_not_scheduled_when_disabled",
    "test_safe_mode_spawns_no_custom_material",
    "test_shader_load_failure_enables_safe_mode",
    "test_scoring_marker_switches_material_handle",
    "test_hidden_die_uses_back_variant",
];

/// Les types de l'étape, ceux du bloc de glossaire livré.
const TYPES_DE_L_ETAPE: [&str; 13] = [
    "VisualEffectsPlugin",
    "CrtSettings",
    "SafeMode",
    "BackgroundMaterial",
    "BackgroundUniform",
    "BackgroundQuad",
    "ThemePalette",
    "VisualThemeController",
    "CrtMaterial",
    "CrtUniform",
    "HoloOutlineMaterial",
    "HoloUniform",
    "HoloMaterials",
];

// -------------------------------------------------------------- volet 2

/// Les onze tests nommés existent, une fois chacun, dans le fichier de tests
/// de l'étape ou dans le module du thème, sous `#[test]` ; aucun test de la
/// crate n'est ignoré. Un `cargo test --workspace` vert ne prouve pas qu'un
/// test nommé existe.
#[test]
fn test_step7_named_tests_all_exist() {
    for nom in TESTS_IMPOSES {
        let trouve = chercher(&format!("fn {nom}\\b"), &["crates/"]);
        assert_eq!(
            trouve.len(),
            1,
            "« {nom} » existe {} fois ; un test absent ne fait échouer aucune commande",
            trouve.len()
        );
        let (fichier, reste) = trouve[0].split_once(':').expect("chemin:numéro:texte");
        assert!(
            fichier == TESTS_DE_L_ETAPE || fichier == THEME,
            "« {nom} » a changé de fichier : {fichier}"
        );
        let numero: usize = reste
            .split_once(':')
            .expect("numéro:texte")
            .0
            .parse()
            .expect("un numéro de ligne");
        let contenu = lire(fichier);
        let lignes: Vec<&str> = contenu.lines().map(str::trim).collect();
        assert_eq!(
            lignes.get(numero - 2).copied(),
            Some("#[test]"),
            "« {nom} » n'est pas sous `#[test]`"
        );
    }
    let ignores = chercher("#\\[ignore", &["crates/ui_and_juice"]);
    assert!(
        ignores.is_empty(),
        "un test est marqué ignoré : {ignores:?}"
    );
}

/// Les valeurs de référence se relisent dans le corps des tests : 64, 16 et
/// 32 octets pile, 1 500 ms exactement, 1,3 et 2,1 pour la Boss, 120 frames
/// et zéro `Modified`. Un test « ajusté » passe au vert et vide l'étape de son
/// sens.
#[test]
fn test_reference_values_are_read_in_the_tests() {
    let tests = lire(TESTS_DE_L_ETAPE);
    let theme = lire(THEME);
    for (source, texte) in [
        (&tests, "let taille = BackgroundUniform::min_size().get();"),
        (&tests, "assert_eq!(taille, 64"),
        (&tests, "CrtUniform::min_size().get(), 16"),
        (&tests, "CrtMaterial::min_size().get(), 16"),
        (&tests, "let taille = HoloUniform::min_size().get();"),
        (&tests, "assert_eq!(taille, 32"),
        (&tests, "elapsed_millis(&app), 1500"),
        (&tests, "material_palette(&app).swirl_factor, 2.1"),
        (&tests, "for _ in 0..120"),
        (&tests, "modified, 0"),
        (&theme, "BOSS.speed, 1.3"),
        (&theme, "BOSS.swirl_factor, 2.1"),
    ] {
        assert!(
            source.contains(texte),
            "la valeur de référence « {texte} » a bougé"
        );
    }
    // Les tests de taille ne se relâchent pas.
    for relache in [">= 16", ">= 64", ">= 32", "% 4 == 0"] {
        assert!(
            !tests.contains(relache),
            "un test de taille est relâché : « {relache} »"
        );
    }
}

// -------------------------------------------------------------- critère 9

/// Aucun type des deux crates de jeu ne dérive à la fois `Component` et
/// `Resource` : en 0.19, `Resource` est un sous-trait de `Component`. Dans
/// `graphics/`, deux composants et deux seulement : le marqueur du quad et le
/// filtre cathodique, que `FullscreenMaterial` exige.
#[test]
fn test_no_type_derives_component_and_resource() {
    let derives = chercher(
        "#\\[derive\\(.*\\bComponent\\b",
        &["crates/ui_and_juice/src", "crates/game_state/src"],
    );
    assert!(!derives.is_empty(), "plus aucun composant dérivé ?");
    for ligne in &derives {
        assert!(
            !ligne.contains("Resource"),
            "un type dérive Component et Resource : {ligne}"
        );
    }
    let graphics = chercher(
        "#\\[derive\\(.*\\bComponent\\b",
        &["crates/ui_and_juice/src/graphics"],
    );
    assert_eq!(
        graphics.len(),
        2,
        "deux composants dans graphics/, le marqueur du quad et le filtre : {graphics:?}"
    );
}

// -------------------------------------------------------------- raccord A

/// Les écrivains de `Transform` sont toujours les deux de `animation.rs`, le
/// ressort et la secousse de caméra ; zéro dans `graphics/`, zéro dans les
/// états. Le quad de fond se redimensionne par son maillage.
#[test]
fn test_transform_writers_are_still_the_two_of_animation() {
    let ecrivains = chercher(&motif_ecrivain(), &["crates/"]);
    assert_eq!(ecrivains.len(), 2, "écrivains de Transform : {ecrivains:?}");
    for ligne in &ecrivains {
        assert!(
            ligne.starts_with("crates/ui_and_juice/src/animation.rs"),
            "un écrivain de Transform hors de animation.rs : {ligne}"
        );
    }
    assert!(
        ecrivains.iter().any(|l| l.contains("&mut PunchScale")),
        "le ressort n'écrit plus le Transform"
    );
    assert!(
        ecrivains.iter().any(|l| l.contains("With<Camera2d>")),
        "la secousse n'écrit plus la caméra"
    );
    let graphics = chercher(
        &format!("&mut Trans{}", "form"),
        &["crates/ui_and_juice/src/graphics"],
    );
    assert!(
        graphics.is_empty(),
        "graphics/ touche un Transform : {graphics:?}"
    );
}

// -------------------------------------------------------------- CI

/// Le bloc de CI de l'étape porte ses douze sous-blocs, un par ticket, et les
/// gardes de l'Étape 4 sur `Transform` sont restées à la lettre : l'Étape 7
/// ne les a pas amendées, elle a vérifié qu'elle n'avait pas à le faire.
#[test]
fn test_ci_block_of_step7_carries_every_ticket_unamended() {
    let bloc = bloc_ci("Invariants normatifs Étape 7");
    for ticket in 82..=93 {
        let marqueur = format!("# TASK-{ticket} :");
        assert!(
            bloc.contains(&marqueur),
            "le sous-bloc « {marqueur} » a disparu"
        );
    }
    for garde in [
        "critère 1  priorité wgpu renommée",
        "critère 1  après le tonemapping",
        "critère 7  la valeur du dé lue par le rendu",
        "critère 8  hors webgl2",
        "critère 4  écriture de Transform dans graphics",
        "TASK-92  panique dans le plugin",
        "TASK-93  les limites de l'adaptateur",
    ] {
        assert!(bloc.contains(garde), "la garde « {garde} » a disparu");
    }

    // Les trois gardes de l'Étape 4, à la lettre ; leurs motifs sont assemblés
    // pour ne pas être épelés ici.
    let volet1 = bloc_ci("Invariants normatifs (volet 1)");
    let unique = format!(
        "forbid  \"critère 4  unique écrivain de Transform\" -n '{}' crates/ -g '!crates/ui_and_juice/src/animation.rs'",
        motif_ecrivain()
    );
    let ressort = format!(
        "require \"critère 4  l'animation écrit le Transform\" -n 'Query<\\(Entity, &mut Trans{}, &mut PunchScale\\)>' crates/ui_and_juice/src/animation.rs",
        "form"
    );
    let secousse = format!(
        "require \"critère 4  la secousse écrit la caméra\" -n 'Query<&mut Trans{}, With<Camera2d>>' crates/ui_and_juice/src/animation.rs",
        "form"
    );
    for ligne in [unique, ressort, secousse] {
        assert!(
            volet1.contains(&ligne),
            "la garde de l'Étape 4 a été amendée : {ligne}"
        );
    }
}

// -------------------------------------------------------------- glossaire

/// Chaque type du bloc de glossaire livré est déclaré une fois, et une seule,
/// dans la crate, et le bloc le nomme ; la fonction de configuration de rendu
/// aussi. Le glossaire lui-même vit sur le corpus.
#[test]
fn test_glossary_block_covers_every_step7_type() {
    let bloc = lire("reports/glossaire-etape-7.md");
    for nom in TYPES_DE_L_ETAPE {
        let declaration = chercher(
            &format!("pub (struct|enum) {nom}\\b"),
            &["crates/ui_and_juice/src"],
        );
        assert_eq!(
            declaration.len(),
            1,
            "« {nom} » est déclaré {} fois dans la crate : {declaration:?}",
            declaration.len()
        );
        assert!(
            bloc.contains(&format!("`{nom}`")),
            "le bloc de glossaire ne porte pas « {nom} »"
        );
    }
    let configuration = chercher("pub fn render_plugin\\b", &["crates/ui_and_juice/src"]);
    assert_eq!(configuration.len(), 1, "render_plugin : {configuration:?}");
    assert!(bloc.contains("`render_plugin()`"));
    for phrase in [
        "scindée",
        "l'exception mesurée",
        "WgpuSettingsPriority::Functionality",
        "nom proscrit",
    ] {
        assert!(bloc.contains(phrase), "le bloc ne porte pas « {phrase} »");
    }
}
