//! Contrat des assets, TASK-107 : ce que `assets/` contient, ce que `assets/CREDITS.md` en dit,
//! et ce que le jeu en charge.
//!
//! Les chemins partent de `CARGO_MANIFEST_DIR`, jamais du répertoire courant : `cargo test
//! --workspace` part de la racine et `cargo test -p audio_system` de la crate.
//!
//! # Trois listes, et elles doivent dire la même chose
//!
//! Les fichiers de `assets/audio/`, les lignes du registre, et les chemins que le jeu charge.
//! Un fichier livré que personne ne charge pèse sur le budget sans jamais sonner ; un chemin
//! chargé qui n'existe pas donne un bip, et tous les autres tests passent. Un fichier sans ligne
//! au registre interdit la sortie. La troisième liste se lit **dans le jeu**, sur le journal du
//! backend nul, pas dans une copie écrite ici : une copie de plus dériverait sans que rien tombe.
//!
//! # Une liste blanche, pas une liste noire
//!
//! La colonne Licence n'admet que les valeurs de la politique écrite en tête du registre. Une
//! liste noire laisserait passer `CC-BY-SA-4.0` et toute licence inconnue. Elle se lit **dans la
//! colonne**, ligne par ligne : l'en-tête du registre, lui, doit pouvoir nommer ce qu'il refuse.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use audio_system::{AudioBackendHandle, GameAudioPlugin};
use bevy::{prelude::*, state::app::StatesPlugin};
use game_state::states::{AppState, RunPhase};
use ui_and_juice::events::ScoreStepPlayed;

/// Ce que les 21 fichiers peuvent peser ensemble. **La seule écriture de ce plafond.**
///
/// La porte de CI compare le `.wasm` optimisé **plus `assets/`** à 25 Mo : il reste 621 363
/// octets pour tout le son une fois le backend lié (addendum de l'ADR-006). Elle laisserait
/// passer trois mégaoctets aujourd'hui, et c'est l'intégration qui le découvrirait.
const AUDIO_BUDGET_BYTES: u64 = 500_000;

const PROJECT_LICENCE: &str = "Propriétaire, Projet Yamstro (tous droits réservés)";
const PROOFS_DIR: &str = "docs/licences-assets";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("racine du dépôt")
}

/// Les fichiers sous `dir`, en chemins relatifs à la racine du dépôt. `.DS_Store` est ignoré :
/// le Finder en pose un dès qu'on ouvre le dossier pour écouter, et le dépôt l'ignore déjà.
fn files_under(dir: &str) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, found: &mut BTreeSet<String>) {
        for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("{} : {e}", dir.display())) {
            let path = entry.expect("entrée").path();
            if path.is_dir() {
                walk(root, &path, found);
            } else if path.file_name().is_some_and(|name| name != ".DS_Store") {
                let relative = path.strip_prefix(root).expect("sous la racine");
                found.insert(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let root = root();
    let mut found = BTreeSet::new();
    walk(&root, &root.join(dir), &mut found);
    found
}

/// Une ligne du registre : fichier, source, auteur, licence, date.
type Row = [String; 5];

/// Les lignes de la table qui suit le titre `## Registre`. Les tables de la politique, plus
/// haut, ne sont pas des lignes du registre.
fn parse_register(text: &str) -> Vec<Row> {
    let body = text
        .split_once("\n## Registre\n")
        .expect("le registre a un titre `## Registre`")
        .1;
    body.lines()
        .filter(|line| line.starts_with("| `"))
        .map(|line| {
            let cells: Vec<String> = line
                .trim()
                .trim_start_matches('|')
                .trim_end_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect();
            let count = cells.len();
            let mut row: Row = cells
                .try_into()
                .unwrap_or_else(|_| panic!("{count} colonnes au lieu de cinq : {line}"));
            row[0] = row[0].trim_matches('`').to_string();
            row
        })
        .collect()
}

fn register() -> Vec<Row> {
    let text = fs::read_to_string(root().join("assets/CREDITS.md")).expect("assets/CREDITS.md");
    parse_register(&text)
}

/// Les chemins du registre, avec leur nombre de lignes : un fichier en a exactement une.
fn register_paths(rows: &[Row], keep: impl Fn(&str) -> bool) -> BTreeMap<String, usize> {
    let mut paths = BTreeMap::new();
    for row in rows.iter().filter(|row| keep(&row[0])) {
        *paths.entry(row[0].clone()).or_insert(0) += 1;
    }
    paths
}

/// Compare des **ensembles**, dans les deux sens : deux listes de même taille peuvent différer
/// d'un fichier chacune, et deux erreurs qui s'annulent passeraient un test de longueur.
fn assert_same_sets(what: &str, on_disk: &BTreeSet<String>, credited: &BTreeMap<String, usize>) {
    let credited_set: BTreeSet<String> = credited.keys().cloned().collect();
    let uncredited: Vec<_> = on_disk.difference(&credited_set).collect();
    let orphans: Vec<_> = credited_set.difference(on_disk).collect();
    let repeated: Vec<_> = credited.iter().filter(|(_, n)| **n != 1).collect();
    assert!(
        uncredited.is_empty() && orphans.is_empty() && repeated.is_empty(),
        "{what} : sans ligne au registre {uncredited:?} ; lignes sans fichier {orphans:?} ; \
         lignes répétées {repeated:?}"
    );
}

// ------------------------------------------------------------------ le registre

#[test]
fn test_every_audio_asset_has_a_credit() {
    let rows = register();
    let on_disk = files_under("assets/audio");
    assert!(!on_disk.is_empty(), "`assets/audio/` est vide");
    let credited = register_paths(&rows, |path| path.starts_with("assets/audio/"));
    assert_same_sets("audio", &on_disk, &credited);

    // Aucune ligne orpheline, où qu'elle pointe, et cinq colonnes renseignées.
    let root = root();
    for row in &rows {
        assert!(
            root.join(&row[0]).is_file(),
            "ligne sans fichier : {}",
            row[0]
        );
        for (cell, column) in
            row.iter()
                .zip(["Fichier", "Source", "Auteur", "Licence", "Ajouté le"])
        {
            assert!(
                !cell.is_empty() && !cell.contains("TODO"),
                "{} : colonne « {column} » vide ou à faire",
                row[0]
            );
        }
        let date: Vec<&str> = row[4].split('-').collect();
        assert!(
            date.len() == 3 && date.iter().all(|part| part.parse::<u32>().is_ok()),
            "{} : date « {} », attendue AAAA-MM-JJ",
            row[0],
            row[4]
        );
    }
}

/// Le registre couvre **tout `assets/`**, pas seulement le son : les shaders aujourd'hui, les
/// traductions demain. Seul le registre lui-même n'a pas de ligne.
#[test]
fn test_project_written_assets_are_credited() {
    let rows = register();
    let on_disk: BTreeSet<String> = files_under("assets")
        .into_iter()
        .filter(|path| !path.starts_with("assets/audio/") && path != "assets/CREDITS.md")
        .collect();
    assert!(!on_disk.is_empty(), "les shaders ont disparu de `assets/`");
    let credited = register_paths(&rows, |path| !path.starts_with("assets/audio/"));
    assert_same_sets("hors audio", &on_disk, &credited);
    assert!(
        rows.iter().all(|row| row[0] != "assets/CREDITS.md"),
        "le registre ne se couvre pas lui-même"
    );
}

// ------------------------------------------------------------------ les licences

/// Ce que la politique refuse dans un registre, ligne par ligne. `proofs` : les fichiers de
/// preuve archivés, par leur nom.
fn licence_problems(rows: &[Row], proofs: &BTreeSet<String>) -> Vec<String> {
    let mut problems = Vec::new();
    for row in rows {
        let (path, licence) = (&row[0], row[3].as_str());
        let commercial = licence
            .strip_prefix("Commerciale : ")
            .is_some_and(|name| !name.trim().is_empty());
        let allowed = licence == "CC0-1.0"
            || licence == "CC-BY-4.0"
            || licence == PROJECT_LICENCE
            || commercial;
        if !allowed {
            problems.push(format!(
                "{path} : licence « {licence} » hors de la liste blanche"
            ));
        }
        // Un lien meurt : hors CC0 et hors projet, la preuve est archivée au dépôt, à son nom.
        if licence == "CC-BY-4.0" || commercial {
            let name = path.rsplit('/').next().expect("nom de fichier");
            if !proofs.iter().any(|proof| proof.starts_with(name)) {
                problems.push(format!(
                    "{path} : aucune preuve sous `{PROOFS_DIR}/{name}.*`"
                ));
            }
        }
    }
    problems
}

fn row(path: &str, licence: &str) -> Row {
    [
        path,
        "https://exemple.org/pack",
        "Quelqu'un",
        licence,
        "2026-09-18",
    ]
    .map(str::to_string)
}

/// La règle, éprouvée sur des registres fabriqués : le registre réel ne porte aujourd'hui que
/// des fichiers du projet, et ne ferait jamais tomber ces branches.
#[test]
fn test_licence_rule_refuses_what_the_policy_refuses() {
    let none = BTreeSet::new();
    for refused in [
        "CC-BY-NC-4.0",
        "CC-BY-ND-4.0",
        "CC-BY-SA-4.0",
        "GPL-3.0-only",
        "AGPL-3.0-only",
        "free for personal use",
        "licence du dépôt",
        "Commerciale : ",
        "cc0-1.0",
    ] {
        let problems = licence_problems(&[row("assets/audio/coin.ogg", refused)], &none);
        assert_eq!(problems.len(), 1, "« {refused} » : {problems:?}");
        assert!(problems[0].contains("liste blanche"), "{problems:?}");
    }

    for allowed in ["CC0-1.0", PROJECT_LICENCE] {
        let problems = licence_problems(&[row("assets/audio/coin.ogg", allowed)], &none);
        assert!(problems.is_empty(), "« {allowed} » : {problems:?}");
    }

    // Attribution et licence commerciale : admises, **preuve à l'appui**, au nom du fichier.
    let proof: BTreeSet<String> = ["coin.ogg.txt".to_string()].into();
    let other: BTreeSet<String> = ["die_lock.ogg.txt".to_string()].into();
    for proven in ["CC-BY-4.0", "Commerciale : Pack Sonore Pro"] {
        let rows = [row("assets/audio/coin.ogg", proven)];
        assert!(licence_problems(&rows, &proof).is_empty(), "« {proven} »");
        for missing in [&none, &other] {
            let problems = licence_problems(&rows, missing);
            assert_eq!(problems.len(), 1, "« {proven} » : {problems:?}");
            assert!(problems[0].contains("preuve"), "{problems:?}");
        }
    }
}

#[test]
fn test_every_licence_is_allowed() {
    let proofs_dir = root().join(PROOFS_DIR);
    let proofs: BTreeSet<String> = match fs::read_dir(&proofs_dir) {
        Ok(entries) => entries
            .map(|entry| {
                entry
                    .expect("entrée")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect(),
        Err(_) => BTreeSet::new(),
    };
    let rows = register();
    assert!(
        rows.len() >= 21,
        "le registre ne porte que {} lignes",
        rows.len()
    );
    let problems = licence_problems(&rows, &proofs);
    assert!(problems.is_empty(), "{problems:#?}");
}

// ------------------------------------------------------------------ les fichiers

/// Tout est de l'OGG **Vorbis**, par l'extension et par le contenu : un WAV renommé, ou de
/// l'Opus dans un conteneur Ogg, passerait un contrôle d'extension et donnerait un bip en jeu.
#[test]
fn test_all_audio_is_ogg() {
    let root = root();
    for path in files_under("assets/audio") {
        assert!(path.ends_with(".ogg"), "{path} n'est pas un `.ogg`");
        assert_eq!(path.matches('/').count(), 2, "{path} : pas de sous-dossier");
        let bytes = fs::read(root.join(&path)).expect("lecture");
        let head = &bytes[..bytes.len().min(64)];
        assert!(
            head.starts_with(b"OggS"),
            "{path} n'est pas un conteneur Ogg"
        );
        assert!(
            head.windows(7).any(|window| window == b"\x01vorbis"),
            "{path} n'est pas du Vorbis"
        );
    }
}

/// Le bip qui remplace un son manquant est produit par le code, jamais par un fichier : un
/// fichier de repli ferait taire l'avertissement qui signale l'asset manquant.
#[test]
fn test_no_placeholder_asset_ships() {
    for path in files_under("assets/audio") {
        let name = path.to_lowercase();
        for marker in [
            "beep",
            "bip",
            "placeholder",
            "silence",
            "tmp",
            "test",
            "todo",
        ] {
            assert!(
                !name.contains(marker),
                "{path} : nom de fichier de repli (« {marker} »)"
            );
        }
    }
}

#[test]
fn test_audio_fits_the_wasm_budget() {
    let root = root();
    let total: u64 = files_under("assets/audio")
        .iter()
        .map(|path| fs::metadata(root.join(path)).expect("métadonnées").len())
        .sum();
    assert!(
        total <= AUDIO_BUDGET_BYTES,
        "`assets/audio/` pèse {total} octets, plafond {AUDIO_BUDGET_BYTES}"
    );
}

// ------------------------------------------------------------------ ce que le jeu charge

/// **Les chemins que le jeu charge sont exactement les fichiers livrés**, dans les deux sens.
/// Lus sur le journal du backend nul, après le démarrage : dix-sept clips par la banque, quatre
/// couches par la musique.
#[test]
fn test_the_game_loads_exactly_the_shipped_audio() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()));
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    app.add_message::<ScoreStepPlayed>();
    app.add_plugins(GameAudioPlugin::headless());
    app.update();

    let handle = app.world().resource::<AudioBackendHandle>();
    let journal = handle.null().expect("le backend monté n'est pas le nul");
    let clips: Vec<&String> = journal.loaded().iter().collect();
    let layers: Vec<&String> = journal.layer_loads().iter().map(|(path, _)| path).collect();
    assert_eq!((clips.len(), layers.len()), (17, 4));

    let loaded: BTreeSet<String> = clips
        .iter()
        .chain(&layers)
        .map(|path| format!("assets/{path}"))
        .collect();
    assert_eq!(loaded.len(), 21, "un chemin est chargé deux fois");
    let shipped = files_under("assets/audio");
    let silent: Vec<_> = loaded.difference(&shipped).collect();
    let dead_weight: Vec<_> = shipped.difference(&loaded).collect();
    assert!(
        silent.is_empty() && dead_weight.is_empty(),
        "chargés mais absents, donc remplacés par un bip : {silent:?} ; livrés mais jamais \
         chargés : {dead_weight:?}"
    );
}
