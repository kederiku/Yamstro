//! Contrat de la crate audio. Tests sans périphérique : ils vérifient des
//! décisions, jamais du son. Ce fichier est celui que le document de l'Étape 8
//! nomme ; chaque ticket de l'étape y ajoute les siens.

use std::{collections::BTreeSet, path::PathBuf, process::Command, time::Duration};

use audio_system::{
    AdaptiveMusicManager, AudioBackendHandle, AudioBusVolumes, AudioClip, BackendKind, Bus,
    GameAudioPlugin, NullBackend, PlayedSound, SoundEffectBank,
    backend::resolve_kind,
    bus::{layer_gain, local_gain, music_gain, sfx_volume},
    music::{
        CLIMAX_THRESHOLD_PERCENT, DEFAULT_FADE_PER_SECOND, DUCK_DURATION, STEM_PATHS,
        blind_in_play, target_gains,
    },
};
use bevy::{prelude::*, state::app::StatesPlugin, time::TimePlugin};
use core_engine::{
    blinds::{BlindContext, BlindDefinition, BlindType},
    config::RunConfig,
    cups::{CupId, definitions::cup},
    hands::{HandGrid, HandLevels},
    rng::RunRng,
};
use game_state::{
    resources::RunSession,
    states::{AppState, RunPhase},
};
use rand::Rng;
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
    // `MinimalPlugins` n'inclut ni `StatesPlugin` ni `AssetPlugin` : ils s'ajoutent à la main,
    // **avant** le plugin audio, qui exige le serveur d'assets (TASK-100) et la machine à états
    // du jeu (TASK-101).
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()));
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    app.add_plugins(GameAudioPlugin::headless());
    for _ in 0..3 {
        app.update();
    }
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
/// `AssetServer`, il faut donc le monter, même si le backend nul ne l'appelle pas ; et la
/// musique lit la machine à états du jeu, initialisée ici comme le fait `GameStatePlugin`.
fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()));
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
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
