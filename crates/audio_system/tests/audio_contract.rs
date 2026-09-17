//! Contrat de la crate audio. Tests sans périphérique : ils vérifient des
//! décisions, jamais du son. Ce fichier est celui que le document de l'Étape 8
//! nomme ; chaque ticket de l'étape y ajoute les siens.

use std::{
    collections::{BTreeSet, VecDeque},
    path::PathBuf,
    process::Command,
    time::Duration,
};

use audio_system::{
    AdaptiveMusicManager, AudioBackendHandle, AudioBusVolumes, AudioClip, BackendKind, Bus,
    GameAudioPlugin, NullBackend, PitchScaleTracker, PlayedSound, SoundEffectBank,
    backend::resolve_kind,
    bus::{layer_gain, local_gain, music_gain, sfx_volume},
    music::{
        CLIMAX_THRESHOLD_PERCENT, DEFAULT_FADE_PER_SECOND, DUCK_DURATION, STEM_PATHS,
        blind_in_play, target_gains,
    },
    pitch::{seal_tint, step_intensity},
};
use bevy::{input::InputPlugin, prelude::*, state::app::StatesPlugin, time::TimePlugin};
use core_engine::{
    blinds::{BlindContext, BlindDefinition, BlindType},
    config::RunConfig,
    cups::{CupId, definitions::cup},
    dice::{Die, DieId, DieSeal},
    hands::{HandGrid, HandLevels, YahtzeeHand},
    relics::RelicId,
    rng::RunRng,
    scoring::{ScoreAction, ScoreStep, StepSource},
};
use game_state::{
    resources::{HandContext, RunSession, ScoringStepQueue},
    states::{AppState, RunPhase},
};
use rand::Rng;
use ui_and_juice::{JuicePlugin, events::ScoreStepPlayed};

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
    // `MinimalPlugins` n'inclut ni `StatesPlugin` ni `AssetPlugin` : ils s'ajoutent à la main,
    // **avant** le plugin audio, qui exige le serveur d'assets (TASK-100) et la machine à états
    // du jeu (TASK-101).
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()));
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    app.add_message::<ScoreStepPlayed>();
    app.add_plugins(GameAudioPlugin::headless());
    for _ in 0..3 {
        app.update();
    }
}

/// TASK-105 : l'audio lit les paliers de score que la mise en scène publie. Sans leur tampon, le
/// lecteur paniquerait à la première image, sous un message anonyme : `build` le dit d'entrée.
#[test]
#[should_panic(expected = "`GameAudioPlugin` exige `JuicePlugin` déjà monté")]
fn test_audio_plugin_demands_the_juice_plugin() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()));
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    app.add_plugins(GameAudioPlugin::headless());
}

/// TASK-100 : sans `AssetPlugin`, le chargement des couches ne serait pas écarté en silence, il
/// paniquerait à la première image sous un message qui ne nomme ni le système ni le paramètre.
/// `build` le dit d'entrée, lisiblement, et pour les trois constructeurs.
#[test]
#[should_panic(expected = "`GameAudioPlugin` exige un `AssetPlugin` déjà monté")]
fn test_audio_plugin_demands_the_asset_plugin() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin));
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    app.add_plugins(GameAudioPlugin::headless());
}

/// TASK-101 : la musique lit l'état de l'application à chaque image, menu compris. Sans la
/// machine à états, son système paniquerait à la première image, sous un message anonyme :
/// `build` le dit d'entrée. Il vérifie les deux niveaux, l'état et la sous-phase de run.
#[test]
#[should_panic(expected = "`GameAudioPlugin` exige la machine à états du jeu déjà montée")]
fn test_audio_plugin_demands_the_state_machine() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()));
    app.add_plugins(GameAudioPlugin::headless());
}

#[test]
#[should_panic(expected = "`GameAudioPlugin` exige la machine à états du jeu déjà montée")]
fn test_audio_plugin_demands_the_run_phase_too() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()));
    app.init_state::<AppState>();
    app.add_plugins(GameAudioPlugin::headless());
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

// ------------------------------------------------------------------ TASK-97

/// `MinimalPlugins` n'inclut ni `AssetPlugin` ni `StatesPlugin` : la façade reçoit un
/// `AssetServer`, il faut donc le monter, même si le backend nul ne l'appelle pas ; la musique
/// lit la machine à états du jeu, initialisée ici comme le fait `GameStatePlugin` ; et le lecteur
/// de paliers lit un tampon, enregistré ici comme le fait `JuicePlugin`. Le plugin entier n'est
/// pas monté : il exigerait une file et une manche dès qu'un test entre en phase de décompte.
fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()));
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    app.add_message::<ScoreStepPlayed>();
    app.add_plugins(GameAudioPlugin::headless());
    app
}

fn journal(app: &App) -> &NullBackend {
    let handle = app.world().resource::<AudioBackendHandle>();
    handle.null().expect("le backend monté n'est pas le nul")
}

fn lire(relatif: &str) -> String {
    std::fs::read_to_string(racine().join(relatif)).expect(relatif)
}

#[test]
fn test_null_backend_records_every_play() {
    let mut app = headless_app();
    let server = app.world().resource::<AssetServer>().clone();
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    let tick = handle.0.load_clip(&server, "audio/chip_tick.ogg");
    let hit = handle.0.load_clip(&server, "audio/mult_hit.ogg");

    let attendus = [
        PlayedSound {
            clip: tick,
            bus: Bus::Sfx,
            volume: 0.8,
            pitch: 1.0,
        },
        PlayedSound {
            clip: tick,
            bus: Bus::Sfx,
            volume: 0.8,
            pitch: 1.059_463_1,
        },
        PlayedSound {
            clip: hit,
            bus: Bus::SfxReverb,
            volume: 1.0,
            pitch: 0.97,
        },
        PlayedSound {
            clip: hit,
            bus: Bus::Master,
            volume: 0.25,
            pitch: 2.0,
        },
        PlayedSound {
            clip: tick,
            bus: Bus::Music,
            volume: 0.0,
            pitch: 0.5,
        },
    ];
    for son in attendus {
        handle.0.play(son.clip, son.bus, son.volume, son.pitch);
    }

    // Exactement N entrées, dans l'ordre, les quatre champs transmis sans altération.
    assert_eq!(journal(&app).played(), attendus);

    // `clear` vide le journal des sons, pas l'état : le catalogue reste.
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    handle.null_mut().expect("backend nul").clear();
    assert!(journal(&app).played().is_empty());
    assert_eq!(journal(&app).loaded().len(), 2);
}

#[test]
fn test_null_backend_opens_no_device() {
    let mut app = headless_app();
    let server = app.world().resource::<AssetServer>().clone();
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    let clip = handle.0.load_clip(&server, "audio/coin.ogg");
    for _ in 0..100 {
        handle.0.play(clip, Bus::Sfx, 1.0, 1.0);
    }
    for _ in 0..3 {
        app.update();
    }

    assert_eq!(journal(&app).played().len(), 100);
    // Rien du backend réel n'est monté : ni contexte audio, ni un seul lecteur dans le monde.
    assert!(
        !app.world()
            .contains_resource::<bevy_seedling::prelude::AudioContext>()
    );
    let lecteurs = app
        .world_mut()
        .query::<&bevy_seedling::prelude::SamplePlayer>()
        .iter(app.world())
        .count();
    assert_eq!(lecteurs, 0, "le backend nul a créé un lecteur");
}

#[test]
fn test_facade_is_the_only_backend_frontier() {
    // Non vacuous : le fichier de la façade, lui, nomme bien le backend.
    assert!(lire("crates/audio_system/src/backend.rs").contains("bevy_seedling"));

    let noms = [
        "bevy_seedling",
        "firewheel",
        "kira",
        "bevy_audio",
        "rodio",
        "cpal",
        "Handle<",
    ];
    for fichier in ["lib.rs", "bus.rs", "music.rs", "pitch.rs", "sfx.rs"] {
        let source = lire(&format!("crates/audio_system/src/{fichier}"));
        for nom in noms {
            assert!(
                !source.contains(nom),
                "`{fichier}` nomme `{nom}` : la façade fuit"
            );
        }
    }
}

#[test]
fn test_clip_ids_are_stable_across_loads() {
    let mut app = headless_app();
    let server = app.world().resource::<AssetServer>().clone();
    let chemins: Vec<String> = (1..=6)
        .map(|n| format!("audio/dice_roll_{n:02}.ogg"))
        .collect();

    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    let premiers: Vec<AudioClip> = chemins
        .iter()
        .map(|c| handle.0.load_clip(&server, c))
        .collect();
    // D'autres chargements par-dessus, dont la même liste : **pas de déduplication**, un index
    // par appel. C'est ce qui rendra observable la substitution par le bip de TASK-102.
    let seconds: Vec<AudioClip> = chemins
        .iter()
        .map(|c| handle.0.load_clip(&server, c))
        .collect();

    let tous: BTreeSet<String> = premiers
        .iter()
        .chain(&seconds)
        .map(|c| format!("{c:?}"))
        .collect();
    assert_eq!(tous.len(), 12, "deux chargements ont partagé un index");

    // Un index rendu reste valide : il désigne toujours le même chemin.
    let loaded = journal(&app).loaded();
    assert_eq!(loaded.len(), 12);
    assert_eq!(&loaded[..6], chemins.as_slice());
    assert_eq!(&loaded[6..], chemins.as_slice());
}

#[test]
fn test_backend_handle_is_resource_only() {
    let app = headless_app();
    assert!(
        app.world().contains_resource::<AudioBackendHandle>(),
        "`build` n'insère pas la ressource"
    );

    let source = lire("crates/audio_system/src/backend.rs");
    assert!(
        source.contains(
            "#[derive(Resource)]\npub struct AudioBackendHandle(pub Box<dyn AudioBackend>);"
        ),
        "la ressource porteuse ne dérive pas `Resource` seule"
    );
}

#[test]
fn test_plugin_constructors_choose_a_kind() {
    // Le plugin porte un discriminant ; `build` construit le backend et insère la ressource.
    let nul = headless_app();
    assert!(
        nul.world()
            .resource::<AudioBackendHandle>()
            .null()
            .is_some()
    );

    let mut reel = App::new();
    reel.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()));
    reel.init_state::<AppState>().add_sub_state::<RunPhase>();
    reel.add_message::<ScoreStepPlayed>();
    reel.add_plugins(GameAudioPlugin::offline());
    assert!(
        reel.world()
            .resource::<AudioBackendHandle>()
            .null()
            .is_none()
    );

    // Une machine sans sortie audio : le jeu tourne muet, il ne panique pas.
    assert_eq!(resolve_kind(BackendKind::Real, false), BackendKind::Null);
    assert_eq!(resolve_kind(BackendKind::Real, true), BackendKind::Real);
    assert_eq!(
        resolve_kind(BackendKind::Offline, false),
        BackendKind::Offline
    );
    assert_eq!(resolve_kind(BackendKind::Null, true), BackendKind::Null);
}

#[test]
fn test_load_layer_starts_the_four_together() {
    let mut app = headless_app();
    let server = app.world().resource::<AssetServer>().clone();
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();

    // Avant le départ, recharger un index remplace son chemin.
    handle.0.load_layer(&server, "audio/brouillon.ogg", 2);
    for (index, chemin) in ["base", "melodie", "tension"].into_iter().enumerate() {
        handle
            .0
            .load_layer(&server, &format!("audio/stem_{chemin}.ogg"), index as u8);
        assert!(
            !handle.null().expect("nul").layers_started(),
            "départ avant la quatrième couche"
        );
    }
    let climax = handle.0.load_layer(&server, "audio/stem_climax.ogg", 3);
    assert!(handle.null().expect("nul").layers_started());

    // Après le départ, un rechargement est ignoré et compté.
    handle.0.load_layer(&server, "audio/intrus.ogg", 0);
    handle.0.set_layer_gain(climax, 0.75);
    handle.0.set_bus_gain(Bus::Music, 0.5);

    let nul = journal(&app);
    let attendues: Vec<(String, u8)> = ["base", "melodie", "tension", "climax"]
        .into_iter()
        .enumerate()
        .map(|(index, nom)| (format!("audio/stem_{nom}.ogg"), index as u8))
        .collect();
    assert_eq!(nul.layers(), attendues);
    assert_eq!(nul.ignored_layer_loads(), 1);
    // Les gains sont consignés tels quels : amplitude linéaire, sans conversion.
    assert_eq!(nul.layer_gain(climax), 0.75);
    assert_eq!(nul.bus_gain(Bus::Music), 0.5);
    assert_eq!(nul.bus_gain(Bus::Sfx), 1.0);
}

#[test]
#[should_panic(expected = "index de couche 4")]
fn test_load_layer_rejects_a_fifth_layer() {
    let mut app = headless_app();
    let server = app.world().resource::<AssetServer>().clone();
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    handle.0.load_layer(&server, "audio/stem_de_trop.ogg", 4);
}

// ------------------------------------------------------------------ TASK-98

fn volumes(master: f32, music: f32, sfx: f32) -> AudioBusVolumes {
    AudioBusVolumes { master, music, sfx }
}

#[test]
fn test_bus_volume_multiplication() {
    // Des valeurs dyadiques : leurs produits sont exacts en `f32`. L'attendu est un littéral,
    // jamais la formule sous test, et l'égalité est exacte.
    let cas = [
        (0.5, 0.5, 0.5, 0.125),
        (1.0, 1.0, 1.0, 1.0),
        (0.25, 0.5, 1.0, 0.125),
        (0.75, 0.5, 0.5, 0.1875),
        (1.0, 0.25, 0.25, 0.0625),
        (0.0, 1.0, 1.0, 0.0),
        (0.5, 1.0, 0.75, 0.375),
        (0.125, 0.5, 0.5, 0.03125),
        (1.0, 0.5, 0.25, 0.125),
        (0.75, 0.75, 1.0, 0.5625),
    ];
    for (sfx, master, local, attendu) in cas {
        let rendu = sfx_volume(&volumes(master, 1.0, sfx), local);
        assert_eq!(rendu, attendu, "sfx {sfx}, master {master}, local {local}");
        assert!((0.0..=1.0).contains(&rendu));
    }
    // Le curseur Music ne touche pas un son du bus SFX.
    assert_eq!(sfx_volume(&volumes(0.5, 0.0, 0.5), 0.5), 0.125);
}

#[test]
fn test_music_gain_multiplication() {
    // Quatre facteurs distincts deux à deux.
    assert_eq!(music_gain(&volumes(0.75, 0.5, 1.0), 0.25, 0.5), 0.046875);
    assert_eq!(music_gain(&volumes(1.0, 1.0, 1.0), 1.0, 1.0), 1.0);
    // Le curseur SFX ne touche pas une couche.
    assert_eq!(music_gain(&volumes(0.75, 0.5, 0.0), 0.25, 0.5), 0.046875);
    // Ce que l'on remet à la façade ne porte aucun volume utilisateur.
    assert_eq!(layer_gain(0.25, 0.5), 0.125);
    assert_eq!(local_gain(0.5), 0.5);
}

#[test]
fn test_volume_clamps_above_one() {
    let unite = volumes(1.0, 1.0, 1.0);
    assert_eq!(music_gain(&unite, 1.4, 1.0), 1.0);
    assert_eq!(sfx_volume(&unite, 2.0), 1.0);
    assert_eq!(layer_gain(1.000_000_1, 1.0), 1.0);
    assert_eq!(local_gain(2.0), 1.0);
    // Et sous zéro : le silence, jamais un gain négatif qui inverserait la phase.
    assert_eq!(sfx_volume(&unite, -0.5), 0.0);
    assert_eq!(music_gain(&volumes(-1.0, 1.0, 1.0), 1.0, 1.0), 0.0);
}

#[test]
fn test_default_volumes_are_audible() {
    assert_eq!(AudioBusVolumes::default(), volumes(1.0, 1.0, 1.0));

    // `Default` s'écrit à la main : dérivé, il rendrait trois zéros et un jeu muet sans erreur.
    let source = lire("crates/audio_system/src/bus.rs");
    assert!(source.contains("impl Default for AudioBusVolumes {"));
    let derive = source
        .lines()
        .zip(source.lines().skip(1))
        .find(|(_, suivante)| suivante.starts_with("pub struct AudioBusVolumes"))
        .map(|(derive, _)| derive)
        .expect("dérivés de la ressource");
    assert!(derive.contains("Resource"), "{derive}");
    assert!(
        !derive.contains("Default"),
        "`Default` est dérivé : {derive}"
    );
    assert!(
        !derive.contains("Component"),
        "`Component` et `Resource` : {derive}"
    );
}

#[test]
fn test_bus_volumes_is_resource_only() {
    // Le plugin insère la ressource : `GameAudioPlugin::build` appelle `bus_plugin`.
    let app = headless_app();
    assert_eq!(
        *app.world().resource::<AudioBusVolumes>(),
        volumes(1.0, 1.0, 1.0)
    );
}

#[test]
fn test_bus_gain_pushed_only_when_changed() {
    let mut app = headless_app();

    // À l'image de son insertion la ressource est « changée » : les valeurs d'ouverture partent.
    app.update();
    let ouverture = [(Bus::Master, 1.0), (Bus::Music, 1.0), (Bus::Sfx, 1.0)];
    assert_eq!(journal(&app).bus_gain_writes(), ouverture);

    // Cinq images inertes : rien de plus.
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(journal(&app).bus_gain_writes().len(), 3);

    // Une écriture de l'utilisateur : exactement trois poussées de plus, dans l'ordre.
    app.world_mut().resource_mut::<AudioBusVolumes>().music = 0.5;
    app.update();
    let ecritures = journal(&app).bus_gain_writes();
    assert_eq!(ecritures.len(), 6);
    assert_eq!(
        ecritures[3..],
        [(Bus::Master, 1.0), (Bus::Music, 0.5), (Bus::Sfx, 1.0)]
    );
    assert_eq!(journal(&app).bus_gain(Bus::Music), 0.5);

    // Le routage réverbéré rejoint le bus SFX : il n'a pas de curseur, il n'est jamais poussé.
    assert!(ecritures.iter().all(|(bus, _)| *bus != Bus::SfxReverb));
    // Et un changement de volume ne relance aucun son.
    assert!(journal(&app).played().is_empty());
}

#[test]
fn test_non_finite_volume_reads_as_the_default() {
    // `clamp` laisse passer un `NaN`, et un `NaN` poussé au backend donne des échantillons `NaN`.
    let mut app = headless_app();
    app.world_mut()
        .insert_resource(volumes(f32::NAN, f32::INFINITY, 0.25));
    app.update();
    let attendues = [(Bus::Master, 1.0), (Bus::Music, 1.0), (Bus::Sfx, 0.25)];
    assert_eq!(journal(&app).bus_gain_writes(), attendues);

    assert_eq!(sfx_volume(&volumes(f32::NAN, 1.0, 0.5), 0.5), 0.25);
    assert_eq!(
        music_gain(&volumes(0.5, f32::NEG_INFINITY, 1.0), 0.5, 1.0),
        0.25
    );
}

// ------------------------------------------------------------------ TASK-99

/// Tests purs : aucune `App`, aucun backend, aucun son.
fn manche(
    kind: BlindType,
    target_score: u64,
    current_score: u64,
    hands_remaining: u8,
) -> BlindContext {
    BlindContext {
        blind: BlindDefinition {
            kind,
            target_score,
            ..Default::default()
        },
        target_score,
        current_score,
        hands_remaining,
        used_hands: HandGrid::default(),
    }
}

const EN_MANCHE: (AppState, Option<RunPhase>) = (AppState::InRun, Some(RunPhase::Roll));

fn climax(target_score: u64, current_score: u64) -> f32 {
    let manche = manche(BlindType::Small, target_score, current_score, 3);
    target_gains(EN_MANCHE.0, EN_MANCHE.1, Some(&manche))[3]
}

#[test]
fn test_climax_threshold_is_75_percent() {
    // Le scénario du document, en littéraux : la valeur du seuil est tenue par le comportement.
    assert_eq!(climax(1000, 740), 0.0);
    assert_eq!(climax(1000, 749), 0.0);
    assert_eq!(climax(1000, 750), 1.0);
    // Petites cibles : deux tiers ne suffisent pas, et l'arithmétique reste entière.
    assert_eq!(climax(3, 2), 0.0);
    assert_eq!(climax(3, 3), 1.0);
    assert_eq!(climax(4, 3), 1.0);
    // Une cible nulle est atteinte d'emblée.
    assert_eq!(climax(0, 0), 1.0);
}

#[test]
fn test_target_gains_are_pure() {
    let boss = manche(BlindType::Boss, 1000, 800, 2);
    let petite = manche(BlindType::Small, 500, 10, 4);
    let attendu = target_gains(AppState::InRun, Some(RunPhase::Scoring), Some(&boss));
    assert_eq!(attendu, [1.0, 1.0, 1.0, 1.0]);

    // Mille appels, entrelacés avec d'autres entrées : aucun ne dépend de ceux qui le précèdent.
    for tour in 0..1_000 {
        match tour % 3 {
            0 => assert_eq!(
                target_gains(AppState::MainMenu, None, None),
                [0.6, 0.4, 0.0, 0.0]
            ),
            1 => {
                let autre = target_gains(AppState::InRun, Some(RunPhase::Roll), Some(&petite));
                assert_eq!(autre, [1.0, 1.0, 0.0, 0.0]);
            }
            _ => {}
        }
        assert_eq!(
            target_gains(AppState::InRun, Some(RunPhase::Scoring), Some(&boss)),
            attendu
        );
    }
}

#[test]
fn test_tension_on_boss_and_last_hand() {
    let tension = |kind, hands| {
        target_gains(
            EN_MANCHE.0,
            EN_MANCHE.1,
            Some(&manche(kind, 1000, 0, hands)),
        )[2]
    };
    // Les deux conditions se lèvent indépendamment.
    assert_eq!(tension(BlindType::Boss, 4), 1.0);
    assert_eq!(tension(BlindType::Small, 1), 1.0);
    assert_eq!(tension(BlindType::Small, 2), 0.0);
    assert_eq!(tension(BlindType::Big, 4), 0.0);
    // À zéro main la blind est perdue mais l'image existe encore : la Tension ne retombe pas.
    assert_eq!(tension(BlindType::Small, 0), 1.0);
}

#[test]
fn test_gains_are_zero_outside_a_run() {
    for app in [AppState::Codex, AppState::GameOver, AppState::Victory] {
        assert_eq!(target_gains(app, None, None), [0.0; 4], "{app:?}");
    }
    // Le transitoire : la run est entrée, sa phase n'est pas encore dans le monde.
    assert_eq!(target_gains(AppState::InRun, None, None), [0.0; 4]);
}

#[test]
fn test_climax_holds_at_u64_extremes() {
    // Les bornes se calculent ici, en `u128`, à partir de la constante : le plus petit score qui
    // atteint le seuil. Un flottant ne représente plus les entiers un à un à cette échelle, et
    // une multiplication en `u64` y déborde : ce test est le seul qui distingue ces écritures.
    for target in [u64::MAX, u64::MAX - 1, (1 << 60) + 1, (1 << 53) + 1] {
        let seuil = (u128::from(target) * CLIMAX_THRESHOLD_PERCENT).div_ceil(100);
        let seuil = u64::try_from(seuil).expect("le seuil tient dans un u64");
        assert_eq!(
            climax(target, seuil - 1),
            0.0,
            "cible {target}, une unité sous le seuil"
        );
        assert_eq!(climax(target, seuil), 1.0, "cible {target}, au seuil");
        assert_eq!(
            climax(target, target - 1),
            1.0,
            "cible {target}, juste sous la cible"
        );
        assert_eq!(
            climax(target, u64::MAX),
            1.0,
            "cible {target}, score maximal"
        );
    }
}

#[test]
fn test_menu_and_shop_gains_match_the_table() {
    // Les cinq lignes du tableau de l'étape, une à une.
    assert_eq!(
        target_gains(AppState::MainMenu, None, None),
        [0.6, 0.4, 0.0, 0.0]
    );
    assert_eq!(
        target_gains(AppState::CupSelect, None, None),
        [0.6, 0.4, 0.0, 0.0]
    );
    assert_eq!(
        target_gains(AppState::InRun, Some(RunPhase::Shop), None),
        [0.5, 0.5, 0.0, 0.0]
    );

    let calme = manche(BlindType::Small, 1000, 0, 4);
    for phase in [
        RunPhase::BlindSelect,
        RunPhase::Roll,
        RunPhase::Scoring,
        RunPhase::RoundEnd,
    ] {
        assert_eq!(
            target_gains(AppState::InRun, Some(phase), Some(&calme)),
            [1.0, 1.0, 0.0, 0.0]
        );
    }

    // La quatrième ligne s'ajoute à la troisième, elle ne la remplace pas.
    let boss = manche(BlindType::Boss, 1000, 0, 4);
    assert_eq!(
        target_gains(EN_MANCHE.0, EN_MANCHE.1, Some(&boss)),
        [1.0, 1.0, 1.0, 0.0]
    );
    // La cinquième laisse la Tension « inchangée » : à zéro ici, à un sur une Mise Boss.
    let euphorie = manche(BlindType::Small, 1000, 900, 3);
    assert_eq!(
        target_gains(EN_MANCHE.0, EN_MANCHE.1, Some(&euphorie)),
        [1.0, 1.0, 0.0, 1.0]
    );
    let tout = manche(BlindType::Boss, 1000, 900, 3);
    assert_eq!(
        target_gains(EN_MANCHE.0, EN_MANCHE.1, Some(&tout)),
        [1.0, 1.0, 1.0, 1.0]
    );
}

#[test]
fn test_blind_in_play_filters_stale_contexts() {
    // Le contexte de blind n'est jamais retiré du monde : en boutique, c'est celui de la blind
    // qu'on vient de battre, score au-dessus de la cible.
    let battu = manche(BlindType::Boss, 1000, 1200, 2);
    let boutique = (AppState::InRun, Some(RunPhase::Shop));

    // **Le piège** : la fonction pure fait ce qu'on lui dit. C'est une erreur d'appelant.
    assert_eq!(
        target_gains(boutique.0, boutique.1, Some(&battu)),
        [0.5, 0.5, 1.0, 1.0]
    );
    // Filtré, le mix de la boutique est celui du tableau.
    let filtre = blind_in_play(boutique.0, boutique.1, Some(&battu));
    assert!(filtre.is_none());
    assert_eq!(
        target_gains(boutique.0, boutique.1, filtre),
        [0.5, 0.5, 0.0, 0.0]
    );

    // Hors d'une run, le contexte d'une run finie ne compte pas davantage.
    for app in [
        AppState::MainMenu,
        AppState::CupSelect,
        AppState::Codex,
        AppState::GameOver,
        AppState::Victory,
    ] {
        for phase in [None, Some(RunPhase::Roll)] {
            assert!(
                blind_in_play(app, phase, Some(&battu)).is_none(),
                "{app:?} {phase:?}"
            );
        }
    }
    assert!(blind_in_play(AppState::InRun, None, Some(&battu)).is_none());

    // En jeu, le contexte passe tel quel : de la sélection de blind à la fin de manche.
    for phase in [
        RunPhase::BlindSelect,
        RunPhase::Roll,
        RunPhase::Scoring,
        RunPhase::RoundEnd,
    ] {
        let rendu = blind_in_play(AppState::InRun, Some(phase), Some(&battu));
        assert!(rendu.is_some_and(|b| std::ptr::eq(b, &battu)), "{phase:?}");
    }
    assert!(blind_in_play(EN_MANCHE.0, EN_MANCHE.1, None).is_none());
}

// ------------------------------------------------------------------ TASK-100

/// Les quatre stems de la branche retenue, **en littéraux** : la constante de la crate est ce
/// qui est sous test, elle ne sert pas d'attendu.
fn stems() -> Vec<(String, u8)> {
    [
        ("audio/stem_base.ogg", 0),
        ("audio/stem_melody.ogg", 1),
        ("audio/stem_tension.ogg", 2),
        ("audio/stem_climax.ogg", 3),
    ]
    .into_iter()
    .map(|(chemin, index)| (chemin.to_string(), index))
    .collect()
}

const QUATRE_VOIES: &str = "[LayerHandle(0), LayerHandle(1), LayerHandle(2), LayerHandle(3)]";

fn manager(app: &App) -> &AdaptiveMusicManager {
    app.world().resource::<AdaptiveMusicManager>()
}

fn entites(app: &mut App) -> Vec<Entity> {
    let mut toutes: Vec<Entity> = app
        .world_mut()
        .query::<Entity>()
        .iter(app.world())
        .collect();
    toutes.sort();
    toutes
}

#[test]
fn test_four_stems_are_loaded_and_started() {
    let mut app = headless_app();
    app.update();

    // Quatre appels, pas un de plus, chacun liant **un fichier** à son index : nommer les voies
    // sans les charger donnerait un jeu muet sous des tests verts.
    let nul = journal(&app);
    assert_eq!(nul.layer_loads(), stems());
    assert_eq!(nul.layers(), stems());
    assert!(nul.layers_started(), "les quatre voies ne sont pas parties");
    assert_eq!(nul.ignored_layer_loads(), 0);

    // `layers[i]` est le handle rendu par le chargement d'index `i`.
    assert_eq!(format!("{:?}", manager(&app).layers), QUATRE_VOIES);

    // Les chargements sont un journal, `clear` le vide ; les couches sont un état, elles restent.
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    handle.null_mut().expect("backend nul").clear();
    assert!(journal(&app).layer_loads().is_empty());
    assert_eq!(journal(&app).layers(), stems());
}

#[test]
fn test_layers_created_once() {
    let mut app = headless_app();
    for _ in 0..2 {
        app.update();
    }
    assert_eq!(journal(&app).layer_loads().len(), 4);
    let voies = manager(&app).layers;

    for _ in 0..8 {
        app.update();
    }
    let nul = journal(&app);
    assert_eq!(
        nul.layer_loads(),
        stems(),
        "des voies ont été recréées après le démarrage"
    );
    assert_eq!(nul.ignored_layer_loads(), 0);
    assert_eq!(manager(&app).layers, voies);
}

#[test]
fn test_layer_indices_are_stable() {
    let mut app = headless_app();
    app.update();
    assert_eq!(format!("{:?}", manager(&app).layers), QUATRE_VOIES);

    // L'ordre des fichiers est celui des index de `target_gains` : Base, Mélodie, Tension,
    // Climax. Une permutation ne casse aucune compilation, il faut l'oreille pour la trouver.
    assert_eq!(
        STEM_PATHS,
        [
            "audio/stem_base.ogg",
            "audio/stem_melody.ogg",
            "audio/stem_tension.ogg",
            "audio/stem_climax.ogg",
        ]
    );
    let boss = manche(BlindType::Boss, 1_000, 0, 3);
    assert_eq!(
        target_gains(EN_MANCHE.0, EN_MANCHE.1, Some(&boss)),
        [1.0, 1.0, 1.0, 0.0],
        "la Tension est l'index 2"
    );
    let gagnee = manche(BlindType::Small, 1_000, 1_000, 3);
    assert_eq!(
        target_gains(EN_MANCHE.0, EN_MANCHE.1, Some(&gagnee)),
        [1.0, 1.0, 0.0, 1.0],
        "le Climax est l'index 3"
    );
}

fn vers_phase(app: &mut App, phase: RunPhase, reflexive: bool) {
    {
        let mut next = app.world_mut().resource_mut::<NextState<RunPhase>>();
        if reflexive {
            next.set(phase);
        } else {
            // Appel qualifié : sur un `ResMut`, la méthode homonyme de la détection de
            // changement capture l'appel et ne compile pas.
            NextState::set_if_neq(&mut next, phase);
        }
    }
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(*app.world().resource::<State<RunPhase>>().get(), phase);
}

/// Sur le backend nul, qui ne crée aucune entité : ce test garde la ressource, le journal et
/// l'ensemble des entités du monde (en 0.19 une ressource en est une). Les voies du backend
/// réel, elles, sont écoutées par `test_music_survives_the_state_cycle`.
#[test]
fn test_music_survives_round_end_to_roll() {
    let mut app = headless_app();
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::InRun);
    app.update();
    vers_phase(&mut app, RunPhase::Roll, false);
    vers_phase(&mut app, RunPhase::Scoring, false);
    vers_phase(&mut app, RunPhase::RoundEnd, false);

    let voies = manager(&app).layers;
    let avant = entites(&mut app);

    // La main suivante de la même blind, sous la forme que le jeu emploie.
    vers_phase(&mut app, RunPhase::Roll, false);
    assert_eq!(manager(&app).layers, voies);
    assert_eq!(
        entites(&mut app),
        avant,
        "les entités ont changé : compare les identifiants, pas le nombre"
    );
    assert_eq!(journal(&app).layer_loads(), stems());
    assert_eq!(journal(&app).ignored_layer_loads(), 0);

    // Une transition réellement réflexive. Le témoin tombe : elle a bien eu lieu, et le piège
    // est réel. La musique, elle, ne bouge pas.
    let temoin = app.world_mut().spawn(DespawnOnExit(RunPhase::Roll)).id();
    vers_phase(&mut app, RunPhase::Roll, true);
    assert!(
        app.world().get_entity(temoin).is_err(),
        "la transition réflexive n'a pas eu lieu"
    );
    assert_eq!(manager(&app).layers, voies);
    assert_eq!(entites(&mut app), avant);
    assert_eq!(journal(&app).layer_loads(), stems());
    assert_eq!(journal(&app).ignored_layer_loads(), 0);
}

#[test]
fn test_duck_timer_is_not_zero_duration() {
    let mut app = headless_app();
    app.update();
    let manager = manager(&app);

    assert_eq!(DUCK_DURATION, Duration::from_millis(1_200));
    assert_eq!(manager.duck_timer.duration(), Duration::from_millis(1_200));
    assert_eq!(manager.duck_timer.mode(), TimerMode::Once);
    // Né terminé, et rien ne vient de se terminer : aucun ducking au lancement, et la garde de
    // réentrance de la fanfare laisse passer la première.
    assert!(manager.duck_timer.is_finished());
    assert!(!manager.duck_timer.just_finished());
    assert_eq!(manager.duck, 1.0);

    // La bande-son part du silence, à la vitesse de fondu de l'étape : à la première image le
    // pas de temps est nul, rien n'a encore bougé. Les cibles, elles, sont déjà celles du menu
    // (TASK-101).
    assert_eq!(manager.current_gains, [0.0; 4]);
    assert_eq!(manager.target_gains, [0.6, 0.4, 0.0, 0.0]);
    assert_eq!(DEFAULT_FADE_PER_SECOND, 2.0);
    assert_eq!(manager.fade_per_second, 2.0);

    // L'usage qu'en fera la fanfare : réarmé, il dure sa durée, ni plus ni moins.
    let mut enveloppe = manager.duck_timer.clone();
    enveloppe.reset();
    assert!(!enveloppe.is_finished());
    enveloppe.tick(Duration::from_millis(1_100));
    assert!(!enveloppe.is_finished(), "le ducking ne tient pas 1,1 s");
    enveloppe.tick(Duration::from_millis(150));
    assert!(enveloppe.is_finished(), "le ducking dépasse 1,25 s");
}

#[test]
fn test_manager_is_resource_only() {
    // Jamais `init_resource` : la ressource n'existe pas avant le démarrage, c'est le système
    // qui charge les couches qui l'insère.
    let mut app = headless_app();
    assert!(!app.world().contains_resource::<AdaptiveMusicManager>());
    app.update();
    assert!(app.world().contains_resource::<AdaptiveMusicManager>());

    let source = lire("crates/audio_system/src/music.rs");
    assert!(
        source.contains("#[derive(Resource)]\npub struct AdaptiveMusicManager {"),
        "la ressource ne dérive pas `Resource` seule"
    );
    assert!(!source.contains("impl Default for AdaptiveMusicManager"));
}

// ------------------------------------------------------------------ TASK-101

/// Sans `TimePlugin` : mesuré à TASK-39, il réécrit le pas à chaque image depuis l'horloge
/// réelle. Sans lui, chaque image vaut exactement le pas demandé, et une image sans avance du
/// temps a un pas nul : les changements d'état s'y posent sans faire bouger un gain.
fn timed_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.build().disable::<TimePlugin>(),
        StatesPlugin,
        AssetPlugin::default(),
    ));
    app.init_resource::<Time>();
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    app.add_message::<ScoreStepPlayed>();
    app.add_plugins(GameAudioPlugin::headless());
    app.update();
    app
}

fn avancer(app: &mut App, millis: u64) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_millis(millis));
    app.update();
}

/// Pose un état ou une phase, en une image de pas nul.
fn entrer_en_run(app: &mut App, phase: RunPhase) {
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::InRun);
    app.update();
    if phase != RunPhase::BlindSelect {
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(phase);
        app.update();
    }
    assert_eq!(*app.world().resource::<State<RunPhase>>().get(), phase);
}

/// Les quatre gains courants, en pour-mille entiers.
fn gains_pour_mille(app: &App) -> [i64; 4] {
    manager(app)
        .current_gains
        .map(|gain| (f64::from(gain) * 1000.0).round() as i64)
}

/// Les quatre dernières valeurs poussées au backend, dans l'ordre des couches.
fn pousses(app: &App) -> [f32; 4] {
    let voies = manager(app).layers;
    voies.map(|voie| journal(app).layer_gain(voie))
}

#[test]
fn test_gain_interpolation_is_framerate_independent() {
    // Les quatre cibles à 1 : un Boss, au-delà du seuil. 528 ms, en 33 pas de 16 puis 16 de 33.
    let mesure = |pas: u64, images: u32| {
        let mut app = timed_app();
        app.world_mut()
            .insert_resource(manche(BlindType::Boss, 1_000, 1_000, 3));
        entrer_en_run(&mut app, RunPhase::Roll);
        assert_eq!(manager(&app).current_gains, [0.0; 4], "un pas nul a bougé");
        let depart = app.world().resource::<Time>().elapsed();
        for _ in 0..images {
            avancer(&mut app, pas);
        }
        let ecoule = app.world().resource::<Time>().elapsed() - depart;
        assert_eq!(ecoule, Duration::from_millis(528));
        assert_eq!(manager(&app).target_gains, [1.0; 4]);
        gains_pour_mille(&app)
    };
    let (fin, grossier) = (mesure(16, 33), mesure(33, 16));

    // 1 - e^(-2 × 0,528) = 0,652 : la valeur ne dépend que de la durée, jamais du découpage.
    for couche in 0..4 {
        assert!(
            (651..=653).contains(&fin[couche]),
            "couche {couche} : {} pour mille en 33 pas",
            fin[couche]
        );
        assert!(
            (-1..=1).contains(&(fin[couche] - grossier[couche])),
            "couche {couche} : {} pour mille en 33 pas, {} en 16",
            fin[couche],
            grossier[couche]
        );
    }
}

#[test]
fn test_music_system_runs_in_main_menu() {
    let mut app = timed_app();
    // Ni sous-phase de run, ni contexte de blind : le système tourne quand même.
    assert!(!app.world().contains_resource::<State<RunPhase>>());
    assert!(!app.world().contains_resource::<BlindContext>());
    // Les trois curseurs à mi-course : ils s'appliquent sur les bus, et ne doivent rien changer
    // à ce que la musique remet à la façade.
    app.world_mut().insert_resource(volumes(0.5, 0.5, 0.5));
    let avant = journal(&app).layer_gain_pushes();

    let mut precedent = [0_i64; 4];
    for _ in 0..10 {
        avancer(&mut app, 16);
        let gains = gains_pour_mille(&app);
        assert!(gains[0] > precedent[0] && gains[1] > precedent[1]);
        assert_eq!(gains[2..], [0, 0]);
        precedent = gains;
    }
    assert_eq!(journal(&app).layer_gain_pushes() - avant, 40);
    assert_eq!(manager(&app).target_gains, [0.6, 0.4, 0.0, 0.0]);
    // 160 ms : 1 - e^(-0,32) = 0,2739 du chemin, soit 0,164 et 0,110.
    assert!((163..=165).contains(&precedent[0]), "{precedent:?}");
    assert!((109..=111).contains(&precedent[1]), "{precedent:?}");
    // Ce qui part au backend est le gain courant : aucun volume utilisateur, ducking à 1.
    assert_eq!(pousses(&app), manager(&app).current_gains);
    assert_eq!(journal(&app).bus_gain(Bus::Music), 0.5);

    // Le ducking est lu : il multiplie ce qui part, sans toucher aux gains courants.
    app.world_mut().resource_mut::<AdaptiveMusicManager>().duck = 0.5;
    avancer(&mut app, 16);
    let attendus = manager(&app).current_gains.map(|gain| gain * 0.5);
    assert_eq!(pousses(&app), attendus);
    assert!(attendus[0] > 0.0, "le test ne prouve rien");
}

#[test]
fn test_gain_is_bounded() {
    let mut app = timed_app();
    app.world_mut()
        .insert_resource(manche(BlindType::Boss, 1_000, 1_000, 3));
    entrer_en_run(&mut app, RunPhase::Roll);
    for image in 0..200 {
        avancer(&mut app, 16);
        // Une poussée par couche et par image : lire les quatre, c'est lire chaque valeur.
        for gain in pousses(&app) {
            assert!(
                (0.0..=1.0).contains(&gain),
                "image {image} : {gain} poussé au backend"
            );
        }
    }

    // Un poids sorti de ses bornes ne passe pas la façade : elle reçoit `layer_gain`.
    let mut manager_mut = app.world_mut().resource_mut::<AdaptiveMusicManager>();
    manager_mut.current_gains[0] = 1.4;
    manager_mut.current_gains[1] = -0.3;
    avancer(&mut app, 16);
    assert!(
        manager(&app).current_gains[0] > 1.0,
        "le test ne prouve rien"
    );
    assert!(
        manager(&app).current_gains[1] < 0.0,
        "le test ne prouve rien"
    );
    let pousses = pousses(&app);
    assert_eq!(pousses[0], 1.0);
    assert_eq!(pousses[1], 0.0);
}

#[test]
fn test_gains_reach_target() {
    let mut app = timed_app();
    entrer_en_run(&mut app, RunPhase::Roll);
    // Deux couches montent, deux descendent.
    app.world_mut()
        .resource_mut::<AdaptiveMusicManager>()
        .current_gains = [0.0, 0.0, 1.0, 1.0];

    let mut precedent = manager(&app).current_gains;
    for _ in 0..188 {
        avancer(&mut app, 16);
        let gains = manager(&app).current_gains;
        assert!(gains[0] > precedent[0] && gains[1] > precedent[1]);
        assert!(gains[2] < precedent[2] && gains[3] < precedent[3]);
        precedent = gains;
    }
    assert_eq!(manager(&app).target_gains, [1.0, 1.0, 0.0, 0.0]);
    // 3,008 s : l'écart vaut e^(-6,016), 2,4 pour mille. Une convergence, jamais une égalité.
    let gains = gains_pour_mille(&app);
    assert!((990..=999).contains(&gains[0]), "{gains:?}");
    assert!((990..=999).contains(&gains[1]), "{gains:?}");
    assert!((1..=10).contains(&gains[2]), "{gains:?}");
    assert!((1..=10).contains(&gains[3]), "{gains:?}");
}

#[test]
fn test_shop_softens_the_mix() {
    let mut app = timed_app();
    entrer_en_run(&mut app, RunPhase::Roll);
    app.world_mut()
        .resource_mut::<AdaptiveMusicManager>()
        .current_gains = [1.0, 1.0, 0.0, 0.0];

    // La blind vient d'être battue, et c'était un Boss : son contexte **reste dans le monde**.
    // Sans le filtre des contextes périmés, Tension et Climax monteraient en boutique.
    app.world_mut()
        .insert_resource(manche(BlindType::Boss, 1_000, 1_000, 3));
    app.world_mut()
        .resource_mut::<NextState<RunPhase>>()
        .set(RunPhase::Shop);
    app.update();

    avancer(&mut app, 16);
    let debut = manager(&app).current_gains;
    assert!(
        debut[0] < 1.0 && debut[1] < 1.0,
        "la boutique n'adoucit pas"
    );
    for _ in 0..200 {
        avancer(&mut app, 16);
        assert_eq!(manager(&app).current_gains[2..], [0.0, 0.0]);
    }
    assert!(app.world().contains_resource::<BlindContext>());
    assert_eq!(manager(&app).target_gains, [0.5, 0.5, 0.0, 0.0]);
    let gains = gains_pour_mille(&app);
    assert!((500..=502).contains(&gains[0]), "{gains:?}");
    assert!((500..=502).contains(&gains[1]), "{gains:?}");
}

#[test]
fn test_four_layers_pushed_every_frame() {
    // Horloge réelle, état inchangé : aucune garde de changement, quatre poussées par image.
    let mut app = headless_app();
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(journal(&app).layer_gain_pushes(), 40);

    // Un compteur, pas un état : `clear` le remet à zéro, les gains tenus restent.
    let tenus = pousses(&app);
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    handle.null_mut().expect("backend nul").clear();
    assert_eq!(journal(&app).layer_gain_pushes(), 0);
    assert_eq!(pousses(&app), tenus);
}

// ------------------------------------------------------------------ TASK-102

/// Les dix-sept chemins que la banque demande, **en littéraux**, dans l'ordre des champs.
const BANQUE: [&str; 17] = [
    "audio/dice_roll_01.ogg",
    "audio/dice_roll_02.ogg",
    "audio/dice_roll_03.ogg",
    "audio/dice_roll_04.ogg",
    "audio/dice_roll_05.ogg",
    "audio/dice_roll_06.ogg",
    "audio/chip_tick.ogg",
    "audio/hand_base_chord.ogg",
    "audio/relic_chord.ogg",
    "audio/mult_hit.ogg",
    "audio/seal_tick.ogg",
    "audio/die_lock.ogg",
    "audio/hand_consumed.ogg",
    "audio/ui_hover.ogg",
    "audio/ui_click.ogg",
    "audio/coin.ogg",
    "audio/victory_fanfare.ogg",
];

fn banque(app: &App) -> &SoundEffectBank {
    app.world().resource::<SoundEffectBank>()
}

/// Le rang d'un clip dans le catalogue du backend : l'index rendu par `load_clip`.
fn rang(clip: AudioClip) -> usize {
    format!("{clip:?}")
        .trim_start_matches("AudioClip(")
        .trim_end_matches(')')
        .parse()
        .expect("forme de débogage d'un clip")
}

fn session() -> RunSession {
    let deck = cup(CupId::Standard);
    RunSession {
        config: RunConfig::from_cup(&deck),
        ante: 1,
        blind_kind: BlindType::Small,
        gold: deck.starting_gold,
        cup_id: CupId::Standard,
        stake_level: 0,
        hand_levels: HandLevels::default(),
        rng: RunRng::from_seed(20_260_917),
    }
}

fn octets_du_generateur(app: &App) -> String {
    serde_json::to_string(&app.world().resource::<RunSession>().rng).expect("sérialisation")
}

#[test]
fn test_audio_never_advances_run_rng() {
    // La session vit **dans le monde où tournent les systèmes audio** : un système qui
    // l'emprunterait pour y tirer un nombre serait vu.
    let mut app = headless_app();
    app.world_mut().insert_resource(session());
    app.update();
    let avant = octets_du_generateur(&app);

    for son in 0..1_000 {
        let (clip, hauteur) = app
            .world_mut()
            .resource_mut::<SoundEffectBank>()
            .dice_roll();
        let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
        handle.0.play(clip, Bus::Sfx, 1.0, hauteur);
        if son % 50 == 0 {
            app.update();
        }
    }
    assert_eq!(journal(&app).played().len(), 1_000);

    // Les **octets de la sérialisation**, jamais deux tirages : deux tirages égaux par chance
    // passeraient, la sérialisation non.
    assert_eq!(octets_du_generateur(&app), avant);

    // Témoin : un seul tirage sur un flux de la run change ces octets. Le test sait échouer.
    app.world_mut()
        .resource_mut::<RunSession>()
        .rng
        .dice
        .next_u64();
    assert_ne!(octets_du_generateur(&app), avant);
}

#[test]
fn test_dice_roll_uses_six_variations() {
    let mut app = headless_app();
    app.update();
    let lancers: BTreeSet<usize> = banque(&app).dice_rolls.into_iter().map(rang).collect();
    assert_eq!(lancers.len(), 6, "les six variations partagent un clip");

    let mut sortis = BTreeSet::new();
    let mut hauteurs = BTreeSet::new();
    let (mut sous, mut sur) = (0, 0);
    for _ in 0..1_000 {
        let (clip, hauteur) = app
            .world_mut()
            .resource_mut::<SoundEffectBank>()
            .dice_roll();
        sortis.insert(rang(clip));
        assert!(
            (0.95..=1.05).contains(&hauteur),
            "hauteur {hauteur} hors de plus ou moins 5 %"
        );
        hauteurs.insert(hauteur.to_bits());
        if hauteur < 1.0 {
            sous += 1;
        } else {
            sur += 1;
        }
    }
    assert_eq!(sortis, lancers, "une variation ne sort jamais");
    // La hauteur varie vraiment, des deux côtés de la hauteur nominale.
    assert!(
        hauteurs.len() > 500,
        "{} hauteurs distinctes",
        hauteurs.len()
    );
    assert!(
        sous > 300 && sur > 300,
        "{sous} en dessous, {sur} au-dessus"
    );
}

/// Sur le backend nul, qui ne lit aucun fichier : ce test garde ce que la banque demande et ce
/// qu'elle rend, **qu'un fichier existe ou non** (ici, aucun n'existe). Le bip lui-même s'écoute
/// sur le backend réel, par `test_a_missing_clip_is_heard_as_a_beep`.
#[test]
fn test_missing_clip_falls_back_to_beep() {
    let mut app = headless_app();
    app.update();

    // Dix-sept chargements, dans l'ordre des champs, chaque chemin consigné.
    assert_eq!(journal(&app).loaded(), BANQUE);

    // Chaque champ désigne **son** fichier : une permutation ne casse aucune compilation.
    let banque = banque(&app);
    let champs = [
        (banque.dice_rolls[0], "audio/dice_roll_01.ogg"),
        (banque.dice_rolls[1], "audio/dice_roll_02.ogg"),
        (banque.dice_rolls[2], "audio/dice_roll_03.ogg"),
        (banque.dice_rolls[3], "audio/dice_roll_04.ogg"),
        (banque.dice_rolls[4], "audio/dice_roll_05.ogg"),
        (banque.dice_rolls[5], "audio/dice_roll_06.ogg"),
        (banque.chip_tick, "audio/chip_tick.ogg"),
        (banque.hand_base_chord, "audio/hand_base_chord.ogg"),
        (banque.relic_chord, "audio/relic_chord.ogg"),
        (banque.mult_hit, "audio/mult_hit.ogg"),
        (banque.seal_tick, "audio/seal_tick.ogg"),
        (banque.die_lock, "audio/die_lock.ogg"),
        (banque.hand_consumed, "audio/hand_consumed.ogg"),
        (banque.ui_hover, "audio/ui_hover.ogg"),
        (banque.ui_click, "audio/ui_click.ogg"),
        (banque.coin, "audio/coin.ogg"),
        (banque.victory_fanfare, "audio/victory_fanfare.ogg"),
    ];
    for (clip, chemin) in champs {
        assert_eq!(journal(&app).loaded()[rang(clip)], chemin);
    }

    // Et ils se jouent comme les autres.
    let (troisieme, accord) = (banque.dice_rolls[2], banque.hand_base_chord);
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    handle.0.play(troisieme, Bus::Sfx, 1.0, 1.0);
    handle.0.play(accord, Bus::Sfx, 1.0, 1.0);
    let joues = journal(&app).played();
    assert_eq!(joues.len(), 2);
    assert_eq!((joues[0].clip, joues[1].clip), (troisieme, accord));
}

#[test]
fn test_bank_is_resource_only() {
    // Insérée par le système de démarrage, et par lui seul.
    let mut app = headless_app();
    assert!(!app.world().contains_resource::<SoundEffectBank>());
    app.update();
    assert!(app.world().contains_resource::<SoundEffectBank>());

    // Dix frames de plus : la banque n'est pas rechargée.
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(journal(&app).loaded().len(), 17);

    let source = lire("crates/audio_system/src/sfx.rs");
    assert!(
        source.contains("#[derive(Resource)]\npub struct SoundEffectBank {"),
        "la banque ne dérive pas `Resource` seule"
    );
}

/// La banque et son générateur restent hors de toute sauvegarde : la crate audio ne dépend
/// d'aucune crate de sérialisation. `serde_json` n'y entre que pour les tests, qui comparent
/// les octets du générateur de la run. C'est la sortie de l'outil qui fait foi, pas le manifeste.
#[test]
fn test_bank_is_never_serialized() {
    let directes = arbre(&["-p", "audio_system", "-e", "normal", "--depth", "1"]);
    assert!(directes.contains("rand v"), "arbre inattendu : {directes}");
    assert!(
        !directes.contains("serde"),
        "la crate audio dépend d'une crate de sérialisation : {directes}"
    );
}

// ------------------------------------------------------------------ TASK-103

/// Une application démarrée : la banque est chargée, et les curseurs sont à mi-course, pour
/// qu'un volume utilisateur remis à la façade se voie.
fn app_des_sons_d_etat() -> App {
    let mut app = headless_app();
    app.world_mut().insert_resource(volumes(0.5, 0.5, 0.5));
    app.update();
    app
}

fn marquer(app: &mut App, figure: YahtzeeHand) {
    app.world_mut()
        .resource_mut::<BlindContext>()
        .used_hands
        .mark(figure);
}

fn images(app: &mut App, combien: usize) {
    for _ in 0..combien {
        app.update();
    }
}

fn loquets_de_case(app: &App) -> Vec<PlayedSound> {
    let clip = banque(app).hand_consumed;
    let joues = journal(app).played().iter();
    joues.filter(|son| son.clip == clip).copied().collect()
}

fn lancers(app: &App) -> Vec<PlayedSound> {
    let clips = banque(app).dice_rolls;
    let joues = journal(app).played().iter();
    joues
        .filter(|son| clips.contains(&son.clip))
        .copied()
        .collect()
}

fn loquets_de_de(app: &App) -> Vec<PlayedSound> {
    let clip = banque(app).die_lock;
    let joues = journal(app).played().iter();
    joues.filter(|son| son.clip == clip).copied().collect()
}

fn relances(app: &mut App, restantes: u8) {
    app.world_mut().insert_resource(HandContext {
        rerolls_left: restantes,
        active_evaluations: Vec::new(),
        selected_hand: None,
    });
}

fn verrouiller(app: &mut App, de: Entity, verrou: bool) {
    app.world_mut().get_mut::<Die>(de).expect("dé").locked = verrou;
}

#[test]
fn test_hand_consumed_sound_fires_once_per_case() {
    let mut app = app_des_sons_d_etat();
    app.world_mut()
        .insert_resource(manche(BlindType::Small, 1_000, 0, 4));

    marquer(&mut app, YahtzeeHand::Yahtzee);
    images(&mut app, 10);
    assert_eq!(
        loquets_de_case(&app).len(),
        1,
        "un par case, jamais un par image"
    );

    marquer(&mut app, YahtzeeHand::Chance);
    images(&mut app, 10);
    assert_eq!(loquets_de_case(&app).len(), 2);
    // Rien d'autre n'a sonné.
    assert_eq!(journal(&app).played().len(), 2);
}

#[test]
fn test_grid_clear_allows_the_same_hand_to_sound_again() {
    let mut app = app_des_sons_d_etat();
    app.world_mut()
        .insert_resource(manche(BlindType::Small, 1_000, 0, 4));
    marquer(&mut app, YahtzeeHand::Yahtzee);
    images(&mut app, 1);

    // La grille se vide : aucun son, et la copie se remet en phase en une image.
    app.world_mut()
        .resource_mut::<BlindContext>()
        .used_hands
        .clear();
    images(&mut app, 2);
    assert_eq!(loquets_de_case(&app).len(), 1, "le vidage a sonné");
    marquer(&mut app, YahtzeeHand::Yahtzee);
    images(&mut app, 1);
    assert_eq!(loquets_de_case(&app).len(), 2);

    // Le geste réel de la blind suivante : le contexte est **remplacé** par un contexte neuf.
    app.world_mut()
        .insert_resource(manche(BlindType::Big, 2_000, 0, 4));
    images(&mut app, 2);
    assert_eq!(loquets_de_case(&app).len(), 2, "le contexte neuf a sonné");
    marquer(&mut app, YahtzeeHand::Yahtzee);
    images(&mut app, 1);
    assert_eq!(loquets_de_case(&app).len(), 3);
}

#[test]
fn test_no_sound_outside_a_blind() {
    let mut app = app_des_sons_d_etat();
    assert!(!app.world().contains_resource::<BlindContext>());
    images(&mut app, 50);
    assert!(journal(&app).played().is_empty());

    app.world_mut()
        .insert_resource(manche(BlindType::Small, 1_000, 0, 4));
    images(&mut app, 10);
    assert!(
        journal(&app).played().is_empty(),
        "un contexte neuf a sonné"
    );

    // Le système **tourne** : un bit posé ensuite s'entend. Un journal vide ne le prouvait pas.
    marquer(&mut app, YahtzeeHand::FullHouse);
    images(&mut app, 1);
    assert_eq!(loquets_de_case(&app).len(), 1);
}

#[test]
fn test_two_hands_marked_in_one_frame_produce_two_sounds() {
    let mut app = app_des_sons_d_etat();
    app.world_mut()
        .insert_resource(manche(BlindType::Small, 1_000, 0, 4));
    images(&mut app, 1);
    marquer(&mut app, YahtzeeHand::Aces);
    marquer(&mut app, YahtzeeHand::LargeStraight);
    images(&mut app, 3);
    assert_eq!(loquets_de_case(&app).len(), 2);
}

#[test]
fn test_hand_consumed_is_not_pitched() {
    let mut app = app_des_sons_d_etat();
    app.world_mut()
        .insert_resource(manche(BlindType::Small, 1_000, 0, 4));
    marquer(&mut app, YahtzeeHand::Sixes);
    images(&mut app, 1);

    // Hauteur fixe, bus SFX, et la part locale du volume seule : les curseurs sont à 0,5, et ce
    // qui part à la façade vaut 1,0.
    let attendu = PlayedSound {
        clip: banque(&app).hand_consumed,
        bus: Bus::Sfx,
        volume: 1.0,
        pitch: 1.0,
    };
    assert_eq!(journal(&app).played(), [attendu]);
}

#[test]
fn test_dice_roll_sound_fires_once_per_roll() {
    let mut app = app_des_sons_d_etat();
    relances(&mut app, 2);
    entrer_en_run(&mut app, RunPhase::Roll);
    images(&mut app, 10);
    assert_eq!(lancers(&app).len(), 1, "l'entrée dans la phase de lancer");

    relances(&mut app, 1);
    images(&mut app, 10);
    assert_eq!(lancers(&app).len(), 2, "première relance");
    relances(&mut app, 0);
    images(&mut app, 10);
    assert_eq!(lancers(&app).len(), 3, "seconde relance");

    // Une relance refusée ne décrémente rien : le contexte est réécrit à l'identique.
    relances(&mut app, 0);
    images(&mut app, 10);
    assert_eq!(lancers(&app).len(), 3, "la relance refusée a sonné");

    // Rien d'autre n'a sonné, et chaque lancer vient de la banque : clip, hauteur, part locale.
    assert_eq!(journal(&app).played().len(), 3);
    for son in lancers(&app) {
        assert!((0.95..=1.05).contains(&son.pitch), "hauteur {}", son.pitch);
        assert_eq!((son.bus, son.volume), (Bus::Sfx, 1.0));
    }
}

#[test]
fn test_dice_roll_sounds_on_phase_entry_alone() {
    // Un gobelet à zéro relance : le compteur ne bouge jamais, seul l'entrée dans la phase parle.
    let mut app = app_des_sons_d_etat();
    relances(&mut app, 0);
    entrer_en_run(&mut app, RunPhase::Roll);
    images(&mut app, 10);
    assert_eq!(lancers(&app).len(), 1);

    // La main suivante : le compteur **remonte**, dans l'image même de l'entrée. Un son, pas deux.
    vers_phase(&mut app, RunPhase::Scoring, false);
    vers_phase(&mut app, RunPhase::RoundEnd, false);
    assert_eq!(lancers(&app).len(), 1, "une autre phase a sonné");
    relances(&mut app, 2);
    vers_phase(&mut app, RunPhase::Roll, false);
    assert_eq!(lancers(&app).len(), 2);

    // Une baisse hors de la phase de lancer est muette ; une transition réflexive aussi.
    vers_phase(&mut app, RunPhase::Scoring, false);
    relances(&mut app, 1);
    images(&mut app, 5);
    assert_eq!(lancers(&app).len(), 2, "une baisse hors du lancer a sonné");
    vers_phase(&mut app, RunPhase::RoundEnd, false);
    vers_phase(&mut app, RunPhase::Roll, false);
    assert_eq!(lancers(&app).len(), 3);
    vers_phase(&mut app, RunPhase::Roll, true);
    assert_eq!(lancers(&app).len(), 3, "la transition réflexive a sonné");
}

#[test]
fn test_die_lock_sound_fires_on_lock_only() {
    let mut app = app_des_sons_d_etat();
    let des: Vec<Entity> = (0..3)
        .map(|rang| app.world_mut().spawn(Die::new(DieId(rang), 6)).id())
        .collect();
    images(&mut app, 2);
    assert!(journal(&app).played().is_empty());

    verrouiller(&mut app, des[0], true);
    images(&mut app, 1);
    assert_eq!(loquets_de_de(&app).len(), 1);
    images(&mut app, 10);
    assert_eq!(loquets_de_de(&app).len(), 1, "jamais un par image");

    // Deux dés dans la même image : deux loquets.
    verrouiller(&mut app, des[1], true);
    verrouiller(&mut app, des[2], true);
    images(&mut app, 1);
    assert_eq!(loquets_de_de(&app).len(), 3);

    // Le déverrouillage est muet, et reverrouiller sonne de nouveau.
    for de in &des {
        verrouiller(&mut app, *de, false);
    }
    images(&mut app, 10);
    assert_eq!(loquets_de_de(&app).len(), 3, "le déverrouillage a sonné");
    verrouiller(&mut app, des[0], true);
    images(&mut app, 1);
    assert_eq!(loquets_de_de(&app).len(), 4);

    assert_eq!(journal(&app).played().len(), 4);
    for son in loquets_de_de(&app) {
        assert_eq!((son.bus, son.volume, son.pitch), (Bus::Sfx, 1.0, 1.0));
    }
}

#[test]
fn test_state_driven_sounds_never_advance_run_rng() {
    let mut app = app_des_sons_d_etat();
    app.world_mut().insert_resource(session());
    app.world_mut()
        .insert_resource(manche(BlindType::Small, 1_000, 0, 4));
    let de = app.world_mut().spawn(Die::new(DieId(0), 6)).id();
    relances(&mut app, 3);
    entrer_en_run(&mut app, RunPhase::Roll);
    let avant = octets_du_generateur(&app);

    for tour in 0..100_usize {
        // Une case, un verrouillage une image sur deux, une relance à chaque tour.
        let figure = YahtzeeHand::ALL[tour % 13];
        if tour % 13 == 0 {
            app.world_mut()
                .resource_mut::<BlindContext>()
                .used_hands
                .clear();
            app.update();
        }
        marquer(&mut app, figure);
        verrouiller(&mut app, de, tour % 2 == 0);
        relances(&mut app, 3);
        app.update();
        relances(&mut app, 2);
        app.update();
    }

    // Les trois systèmes ont bien sonné, et beaucoup : le test ne passe pas à vide.
    assert_eq!(loquets_de_case(&app).len(), 100);
    assert_eq!(loquets_de_de(&app).len(), 50);
    assert_eq!(lancers(&app).len(), 101);
    // Le lancer passe bien par le tirage de la banque : les hauteurs varient, les clips aussi.
    let hauteurs: BTreeSet<u32> = lancers(&app).iter().map(|s| s.pitch.to_bits()).collect();
    let clips: BTreeSet<usize> = lancers(&app).iter().map(|s| rang(s.clip)).collect();
    assert!(
        hauteurs.len() > 50,
        "{} hauteurs distinctes",
        hauteurs.len()
    );
    assert_eq!(clips.len(), 6, "les six variations ne sortent pas");
    assert_eq!(octets_du_generateur(&app), avant);

    // Témoin : un seul tirage sur un flux de la run change ces octets.
    app.world_mut()
        .resource_mut::<RunSession>()
        .rng
        .dice
        .next_u64();
    assert_ne!(octets_du_generateur(&app), avant);
}

// ------------------------------------------------------------------ TASK-104

/// Une hauteur, en millionièmes entiers : les références s'écrivent en clair, jamais recalculées
/// par la formule sous test, et se comparent à des entiers.
fn millioniemes(hauteur: f32) -> i64 {
    (f64::from(hauteur) * 1_000_000.0).round() as i64
}

fn a_la_hauteur(demi_tons: i32) -> PitchScaleTracker {
    PitchScaleTracker {
        current_semitone: demi_tons,
        ..Default::default()
    }
}

fn hauteur_courante(app: &App) -> i32 {
    app.world().resource::<PitchScaleTracker>().current_semitone
}

fn monter(app: &mut App, paliers: usize) {
    let mut tracker = app.world_mut().resource_mut::<PitchScaleTracker>();
    for _ in 0..paliers {
        tracker.advance();
    }
}

#[test]
fn test_pitch_semitone_formula() {
    // L'unisson, l'octave, la double octave : exacts.
    assert_eq!(millioniemes(a_la_hauteur(0).pitch()), 1_000_000);
    assert_eq!(millioniemes(a_la_hauteur(12).pitch()), 2_000_000);
    assert_eq!(millioniemes(a_la_hauteur(24).pitch()), 4_000_000);

    // **Entre deux octaves** : le demi-ton et la quinte. Les trois points ci-dessus sont aveugles
    // à une division entière de l'exposant, qui rend 1, 2 et 4 aux octaves et reste à 1 entre.
    let demi_ton = millioniemes(a_la_hauteur(1).pitch());
    let quinte = millioniemes(a_la_hauteur(7).pitch());
    assert!((1_059_462..=1_059_464).contains(&demi_ton), "{demi_ton}");
    assert!((1_498_306..=1_498_308).contains(&quinte), "{quinte}");

    // La hauteur de base multiplie : à une octave d'une base de 0,5, on retrouve 1.
    let grave = PitchScaleTracker {
        base_pitch: 0.5,
        ..a_la_hauteur(12)
    };
    assert_eq!(millioniemes(grave.pitch()), 1_000_000);
}

#[test]
fn test_pitch_is_capped() {
    let mut tracker = PitchScaleTracker::default();
    let mut precedente = millioniemes(tracker.pitch());
    for palier in 1..=40 {
        tracker.advance();
        let hauteur = millioniemes(tracker.pitch());
        if palier <= 24 {
            assert!(
                hauteur > precedente,
                "palier {palier} : la gamme ne monte pas"
            );
        } else {
            assert_eq!(hauteur, precedente, "palier {palier} : au-delà du plafond");
        }
        precedente = hauteur;
    }
    assert_eq!(tracker.current_semitone, 24);
    assert_eq!(millioniemes(tracker.pitch()), 4_000_000);
    for _ in 0..40 {
        tracker.advance();
    }
    assert_eq!(tracker.current_semitone, 24);

    // Le plafond est le **champ**, lu par `advance()`, pas un littéral recopié.
    let mut court = PitchScaleTracker {
        max_semitone: 12,
        ..Default::default()
    };
    for _ in 0..40 {
        court.advance();
    }
    assert_eq!(court.current_semitone, 12);
}

#[test]
fn test_pitch_resets_on_scoring_entry() {
    let mut app = headless_app();
    entrer_en_run(&mut app, RunPhase::Roll);
    // La seconde main part de ce que la première a laissé : trois demi-tons, voir plus bas.
    for (main, depart) in [(0, 0), (1, 3)] {
        assert_eq!(hauteur_courante(&app), depart);
        monter(&mut app, 7);
        images(&mut app, 3);
        assert_eq!(
            hauteur_courante(&app),
            depart + 7,
            "main {main} : remise à zéro hors du décompte"
        );

        vers_phase(&mut app, RunPhase::Scoring, false);
        assert_eq!(
            hauteur_courante(&app),
            0,
            "main {main} : le décompte ne repart pas de zéro"
        );

        // La montée du décompte survit à sa **sortie** : le reset est à l'entrée, pas à la fin.
        monter(&mut app, 3);
        vers_phase(&mut app, RunPhase::RoundEnd, false);
        assert_eq!(
            hauteur_courante(&app),
            3,
            "main {main} : remise à zéro à la sortie"
        );
        vers_phase(&mut app, RunPhase::Roll, false);
        assert_eq!(hauteur_courante(&app), 3);
    }
}

#[test]
fn test_reset_is_idempotent() {
    let mut tracker = a_la_hauteur(9);
    for _ in 0..5 {
        tracker.reset();
        assert_eq!(tracker.current_semitone, 0);
        assert_eq!((tracker.base_pitch, tracker.max_semitone), (1.0, 24));
    }

    // Une entrée réflexive dans le décompte rejoue le reset : inoffensif, et aucune garde de
    // réentrance ne l'en empêche. Chaque entrée remet à zéro, la première comme les suivantes.
    let mut app = headless_app();
    entrer_en_run(&mut app, RunPhase::Scoring);
    for _ in 0..3 {
        monter(&mut app, 5);
        assert_eq!(hauteur_courante(&app), 5);
        vers_phase(&mut app, RunPhase::Scoring, true);
        assert_eq!(hauteur_courante(&app), 0);
    }
}

#[test]
fn test_tracker_is_resource_only() {
    // Insérée par le plugin, dès sa construction, avec ses valeurs d'ouverture.
    let app = headless_app();
    let tracker = app.world().resource::<PitchScaleTracker>();
    assert_eq!(
        (
            tracker.base_pitch,
            tracker.current_semitone,
            tracker.max_semitone
        ),
        (1.0, 0, 24)
    );

    let source = lire("crates/audio_system/src/pitch.rs");
    assert!(
        source.contains("#[derive(Resource)]\npub struct PitchScaleTracker {"),
        "la ressource ne dérive pas `Resource` seule"
    );
    assert!(source.contains("impl Default for PitchScaleTracker {"));
}

// ------------------------------------------------------------------ TASK-105

const BASE: StepSource = StepSource::HandBase {
    hand: YahtzeeHand::FullHouse,
};
const DE: StepSource = StepSource::Die {
    die_id: DieId(2),
    value: 5,
};
const RELIQUE: StepSource = StepSource::Relic {
    uid: 7,
    def: RelicId::CrackedDie,
};

fn sceau(seal: DieSeal) -> StepSource {
    StepSource::Seal {
        die_id: DieId(4),
        seal,
    }
}

/// Publie un palier, comme le fait le dépileur de la mise en scène, puis avance d'une image.
fn publier(app: &mut App, source: StepSource, action: ScoreAction) {
    app.world_mut()
        .write_message(ScoreStepPlayed { source, action });
    app.update();
}

fn remettre_a_zero(app: &mut App) {
    app.world_mut().resource_mut::<PitchScaleTracker>().reset();
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    handle.null_mut().expect("backend nul").clear();
}

fn pour_mille(valeur: f32) -> i64 {
    (f64::from(valeur) * 1000.0).round() as i64
}

#[test]
fn test_one_semitone_per_event_whatever_the_source() {
    let mut app = app_des_sons_d_etat();
    let sources = [BASE, DE, RELIQUE, sceau(DieSeal::Gold)];
    let actions = [
        ScoreAction::AddChips(10),
        ScoreAction::AddMult(100),
        ScoreAction::MultiplyMult(150),
    ];
    for palier in 0..10 {
        publier(&mut app, sources[palier % 4], actions[palier % 3]);
        assert_eq!(hauteur_courante(&app), palier as i32 + 1);
    }
    assert_eq!(journal(&app).played().len(), 10, "un son par palier");

    // Plusieurs paliers dans la même image : chacun avance d'un demi-ton.
    for _ in 0..5 {
        app.world_mut().write_message(ScoreStepPlayed {
            source: DE,
            action: ScoreAction::AddChips(1),
        });
    }
    app.update();
    assert_eq!(hauteur_courante(&app), 15);

    // Le plafond est celui du compteur : quarante paliers de plus s'y arrêtent.
    for palier in 0..40 {
        publier(&mut app, sources[palier % 4], actions[palier % 3]);
    }
    assert_eq!(hauteur_courante(&app), 24);
    assert_eq!(journal(&app).played().len(), 55);
}

#[test]
fn test_timbre_matches_step_source() {
    let mut app = app_des_sons_d_etat();
    let banque = banque(&app);
    let attendus = [
        banque.hand_base_chord,
        banque.chip_tick,
        banque.relic_chord,
        banque.seal_tick,
    ];
    let distincts: BTreeSet<usize> = attendus.into_iter().map(rang).collect();
    assert_eq!(distincts.len(), 4, "deux sources partagent un échantillon");

    // Quatre sources, quatre clips. L'assertion porte sur les **clips**, jamais sur les hauteurs.
    for (source, clip) in [BASE, DE, RELIQUE, sceau(DieSeal::Gold)]
        .into_iter()
        .zip(attendus)
    {
        remettre_a_zero(&mut app);
        publier(&mut app, source, ScoreAction::AddChips(10));
        let joues = journal(&app).played();
        assert_eq!(joues.len(), 1);
        assert_eq!(joues[0].clip, clip, "{source:?}");
        assert_eq!(joues[0].bus, Bus::Sfx);
    }

    // La figure de base sonne à la hauteur nominale, **même en cours de séquence** ; un dé, lui,
    // suit la gamme : lire, jouer, puis avancer.
    remettre_a_zero(&mut app);
    publier(&mut app, DE, ScoreAction::AddChips(10));
    publier(&mut app, DE, ScoreAction::AddChips(10));
    publier(&mut app, BASE, ScoreAction::AddChips(10));
    let hauteurs: Vec<i64> = journal(&app)
        .played()
        .iter()
        .map(|son| millioniemes(son.pitch))
        .collect();
    assert_eq!(hauteurs[0], 1_000_000, "le premier palier sonne au nominal");
    assert!(
        (1_059_462..=1_059_464).contains(&hauteurs[1]),
        "{hauteurs:?}"
    );
    assert_eq!(hauteurs[2], 1_000_000, "la figure de base a suivi la gamme");

    // Les quatre sceaux, **au même demi-ton zéro** : la seule variable restante est la teinte.
    let mut teintes = BTreeSet::new();
    for (seal, attendue) in [
        (DieSeal::Gold, 1_020_000),
        (DieSeal::Blue, 1_010_000),
        (DieSeal::Purple, 990_000),
        (DieSeal::Red, 980_000),
    ] {
        remettre_a_zero(&mut app);
        publier(&mut app, sceau(seal), ScoreAction::AddChips(10));
        let son = journal(&app).played()[0];
        assert_eq!(son.clip, attendus[3]);
        let hauteur = millioniemes(son.pitch);
        assert!(
            (attendue - 1..=attendue + 1).contains(&hauteur),
            "{seal:?} : {hauteur}"
        );
        assert_eq!(millioniemes(seal_tint(seal)), hauteur);
        teintes.insert(hauteur);
    }
    assert_eq!(teintes.len(), 4, "deux sceaux partagent une teinte");
}

#[test]
fn test_multiply_mult_routes_to_the_reverb_bus() {
    let mut app = app_des_sons_d_etat();
    let (coup, tic) = (banque(&app).mult_hit, banque(&app).chip_tick);
    for source in [BASE, DE, RELIQUE, sceau(DieSeal::Red)] {
        remettre_a_zero(&mut app);
        publier(&mut app, source, ScoreAction::MultiplyMult(300));
        let son = journal(&app).played()[0];
        assert_eq!((son.clip, son.bus), (coup, Bus::SfxReverb), "{source:?}");
    }

    // La hauteur suit la source, quel que soit le clip : un coup multiplicatif continue la gamme.
    remettre_a_zero(&mut app);
    publier(&mut app, DE, ScoreAction::AddChips(10));
    publier(&mut app, DE, ScoreAction::MultiplyMult(300));
    let second = millioniemes(journal(&app).played()[1].pitch);
    assert!((1_059_462..=1_059_464).contains(&second), "{second}");

    // Les deux autres actions ne le produisent jamais, et restent sur le bus sec.
    for action in [ScoreAction::AddChips(9_999), ScoreAction::AddMult(9_999)] {
        remettre_a_zero(&mut app);
        publier(&mut app, DE, action);
        let son = journal(&app).played()[0];
        assert_eq!((son.clip, son.bus), (tic, Bus::Sfx), "{action:?}");
    }
}

#[test]
fn test_intensity_comes_from_the_action_alone() {
    // Pur : un plancher audible, une part bornée, **chaque action dans son unité**.
    let cas = [
        (ScoreAction::AddChips(0), 600),
        (ScoreAction::AddChips(50), 800),
        (ScoreAction::AddChips(100), 1_000),
        (ScoreAction::AddChips(u64::MAX), 1_000),
        (ScoreAction::AddMult(0), 600),
        (ScoreAction::AddMult(400), 760),
        (ScoreAction::AddMult(-400), 760),
        (ScoreAction::AddMult(1_000), 1_000),
        (ScoreAction::AddMult(i64::MIN), 1_000),
        (ScoreAction::MultiplyMult(50), 800),
        (ScoreAction::MultiplyMult(100), 800),
        (ScoreAction::MultiplyMult(150), 850),
        (ScoreAction::MultiplyMult(300), 1_000),
        (ScoreAction::MultiplyMult(u32::MAX), 1_000),
    ];
    for (action, attendue) in cas {
        assert_eq!(pour_mille(step_intensity(action)), attendue, "{action:?}");
    }
    // Quatre points de multiplicateur ne sonnent pas comme quatre cents jetons.
    assert_ne!(
        pour_mille(step_intensity(ScoreAction::AddMult(400))),
        pour_mille(step_intensity(ScoreAction::AddChips(400)))
    );

    // Dans le système : l'intensité est la part **locale**, la source n'y change rien, et les
    // curseurs, ici à 0,5, ne sont pas remis à la façade.
    let mut app = app_des_sons_d_etat();
    for source in [BASE, DE, RELIQUE, sceau(DieSeal::Blue)] {
        remettre_a_zero(&mut app);
        publier(&mut app, source, ScoreAction::AddChips(50));
        assert_eq!(
            pour_mille(journal(&app).played()[0].volume),
            800,
            "{source:?}"
        );
    }
}

fn palier(source: StepSource, action: ScoreAction) -> ScoreStep {
    ScoreStep {
        source,
        action,
        chips_after: 10,
        mult_after: 100,
        score_after: 10,
    }
}

/// La **chaîne réelle** de la mise en scène, pas un émetteur factice : c'est l'arête d'ordre qui
/// est le sujet. `JuicePlugin` est monté en entier, avec ce qu'il exige en phase de décompte.
#[test]
fn test_reader_runs_after_the_drainer() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.build().disable::<TimePlugin>(),
        StatesPlugin,
        InputPlugin,
        AssetPlugin::default(),
        JuicePlugin,
    ));
    app.init_resource::<Time>();
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    app.add_plugins(GameAudioPlugin::headless());
    app.insert_resource(manche(BlindType::Small, 1_000, 0, 3));
    app.insert_resource(ScoringStepQueue::new(
        VecDeque::from([
            palier(BASE, ScoreAction::AddChips(30)),
            palier(DE, ScoreAction::AddChips(5)),
        ]),
        YahtzeeHand::FullHouse,
        35,
    ));
    entrer_en_run(&mut app, RunPhase::Scoring);
    images(&mut app, 3);
    assert!(
        journal(&app).played().is_empty(),
        "un son avant tout dépilement"
    );
    assert_eq!(app.world().resource::<ScoringStepQueue>().steps.len(), 2);

    // Une image qui dépile un palier : **dans la même image**, le son est parti.
    let pas = app
        .world()
        .resource::<ScoringStepQueue>()
        .step_timer
        .duration();
    app.world_mut().resource_mut::<Time>().advance_by(pas);
    app.update();
    assert_eq!(app.world().resource::<ScoringStepQueue>().steps.len(), 1);
    let joues = journal(&app).played();
    assert_eq!(
        joues.len(),
        1,
        "le son part une image après l'impulsion visuelle"
    );
    assert_eq!(joues[0].clip, banque(&app).hand_base_chord);

    // Le second palier, à l'image de son dépilement aussi, un demi-ton plus haut.
    app.world_mut().resource_mut::<Time>().advance_by(pas);
    app.update();
    assert!(app.world().resource::<ScoringStepQueue>().steps.is_empty());
    let joues = journal(&app).played();
    assert_eq!(joues.len(), 2);
    assert_eq!(joues[1].clip, banque(&app).chip_tick);
    assert!((1_059_462..=1_059_464).contains(&millioniemes(joues[1].pitch)));

    // Le lecteur consomme **sans condition d'état**. Hors de la phase de décompte, les ensembles
    // du dépileur ne tournent plus ; un palier publié s'y entend encore. Placé *dans* l'ensemble
    // du dépileur au lieu d'être ordonné *après* lui, le lecteur hériterait de sa condition et se
    // tairait ici, ce que seule la mise en scène montée en entier permet de voir.
    vers_phase(&mut app, RunPhase::RoundEnd, false);
    publier(&mut app, DE, ScoreAction::AddChips(1));
    assert_eq!(
        journal(&app).played().len(),
        3,
        "hors du décompte, le lecteur s'est tu"
    );
}
