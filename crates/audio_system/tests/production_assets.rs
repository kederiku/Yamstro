//! Les fichiers de production, lus par **le décodeur du jeu** : TASK-107.
//!
//! `tests/assets_contract.rs` regarde des noms, des octets et un registre. Rien de cela ne dit
//! qu'un fichier se décode, ni ce que le jeu en tire. Ici l'application est montée sur la racine
//! de production, `assets/`, avec le backend réel, hors ligne.
//!
//! # La durée se mesure sur ce que le jeu joue
//!
//! Quatre couches de durées « presque » égales dérivent à chaque bouclage, et le défaut passe
//! pour une panne du backend alors qu'il est dans les fichiers. Tolérance : zéro trame. Le nombre
//! qui compte est celui des trames **décodées par le jeu** : un décodeur qui ne rognerait pas la
//! fin comme l'encodeur l'a déclarée ferait dériver quatre fichiers de même durée nominale. Aucun
//! outil externe : ni la machine de développement ni le runner n'ont `ffprobe`.

#[path = "support/offline.rs"]
mod offline;

use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    sync::{Mutex, Once},
};

use audio_system::music::STEM_PATHS;
use bevy::{asset::LoadState, prelude::*};
use bevy_seedling::prelude::*;
use offline::{offline_app_at, render, wait_loaded, wait_settled};

/// La racine de production, vue de la crate.
const PRODUCTION_ROOT: &str = "../../assets";

static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            WARNINGS
                .lock()
                .expect("verrou")
                .push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

fn capture_warnings() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_logger(&Capture).expect("un seul journaliseur par processus");
        log::set_max_level(log::LevelFilter::Warn);
    });
}

/// Les chemins d'asset des fichiers livrés, `audio/<nom>`. Lus sur le disque : le contrat des
/// fichiers tient déjà que ce sont exactement ceux que le jeu charge.
fn shipped() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(PRODUCTION_ROOT)
        .join("audio");
    let mut paths: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} : {e}", dir.display()))
        .map(|entry| {
            entry
                .expect("entrée")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name != ".DS_Store")
        .map(|name| format!("audio/{name}"))
        .collect();
    paths.sort();
    paths
}

/// TASK-107 : **les quatre couches ont la même durée, à la trame près**, le même nombre de voies
/// et la même fréquence, telles que le jeu les décode.
#[test]
fn test_stems_have_identical_duration() {
    let stems: Vec<String> = shipped()
        .into_iter()
        .filter(|path| path.starts_with("audio/stem_"))
        .collect();
    assert_eq!(stems.len(), 4, "quatre couches attendues : {stems:?}");
    let expected: BTreeSet<&str> = STEM_PATHS.into_iter().collect();
    let found: BTreeSet<&str> = stems.iter().map(String::as_str).collect();
    assert_eq!(
        found, expected,
        "les couches livrées ne sont pas celles que le jeu charge"
    );

    let mut app = offline_app_at(PRODUCTION_ROOT);
    let mut left = Vec::new();
    wait_loaded(&mut app, &STEM_PATHS, &mut left);

    let server = app.world().resource::<AssetServer>().clone();
    let samples = app.world().resource::<Assets<AudioSample>>();
    let measures: Vec<(u64, usize, u32)> = STEM_PATHS
        .iter()
        .map(|path| {
            let handle: Handle<AudioSample> = server.load(path.to_string());
            let sample = samples.get(&handle).expect("couche chargée");
            let resource = sample.get();
            (
                resource.len_frames(),
                resource.num_channels().get(),
                sample.original_sample_rate().get(),
            )
        })
        .collect();
    // Visible sous `--nocapture` : ce que le jeu tire des quatre fichiers.
    println!("(trames décodées, voies, fréquence d'origine) : {measures:?}");
    let distinct: BTreeSet<&(u64, usize, u32)> = measures.iter().collect();
    assert_eq!(
        distinct.len(),
        1,
        "(trames, voies, fréquence) par couche, tolérance zéro trame : {measures:?}"
    );
    let (frames, _, rate) = measures[0];
    assert!(
        frames > u64::from(rate),
        "une boucle de moins d'une seconde : {frames} trames"
    );
}

/// TASK-107 : **le jeu démarre sur ses vrais fichiers sans un bip.** Les 21 chemins se décodent,
/// aucun avertissement de repli ne sort, les quatre voies partent, et la bande-son s'entend. Un
/// `.ogg` illisible par le jeu passerait tous les contrôles de fichiers.
#[test]
fn test_production_assets_load_without_a_beep() {
    capture_warnings();
    let paths = shipped();
    assert_eq!(paths.len(), 21, "{paths:?}");
    let names: Vec<&str> = paths.iter().map(String::as_str).collect();

    let mut app = offline_app_at(PRODUCTION_ROOT);
    let mut left = Vec::new();
    wait_settled(&mut app, &names, &mut left);

    let server = app.world().resource::<AssetServer>().clone();
    for path in &paths {
        let handle: Handle<AudioSample> = server.load(path.clone());
        assert!(
            matches!(server.load_state(&handle), LoadState::Loaded),
            "{path} ne se décode pas : {:?}",
            server.load_state(&handle)
        );
    }

    // Au menu, la Base et la Mélodie montent en fondu : la bande-son de production s'entend.
    render(&mut app, 3.0, &mut left);
    let peak = left.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    assert!(
        peak > 0.01,
        "la bande-son de production ne sort pas : crête {peak}"
    );

    let voices = app
        .world_mut()
        .query_filtered::<Entity, With<SamplePlayer>>()
        .iter(app.world())
        .count();
    assert_eq!(
        voices, 4,
        "les quatre voies de la musique ne sont pas parties"
    );

    let said = WARNINGS.lock().expect("verrou");
    let fallbacks: Vec<&String> = said
        .iter()
        .filter(|message| message.contains("bip") || message.contains("introuvable"))
        .collect();
    assert!(fallbacks.is_empty(), "{fallbacks:#?}");
}
