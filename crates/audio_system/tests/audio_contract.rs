//! Contrat de la crate audio. Tests sans périphérique : ils vérifient des
//! décisions, jamais du son. Ce fichier est celui que le document de l'Étape 8
//! nomme ; chaque ticket de l'étape y ajoute les siens.

use std::{collections::BTreeSet, path::PathBuf, process::Command};

use audio_system::{
    AudioBackendHandle, AudioBusVolumes, AudioClip, BackendKind, Bus, GameAudioPlugin, NullBackend,
    PlayedSound,
    backend::resolve_kind,
    bus::{layer_gain, local_gain, music_gain, sfx_volume},
};
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
    app.add_plugins((MinimalPlugins, StatesPlugin, GameAudioPlugin::headless()));
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

// ------------------------------------------------------------------ TASK-97

/// `MinimalPlugins` n'inclut ni `AssetPlugin` ni `StatesPlugin` : la façade reçoit un
/// `AssetServer`, il faut donc le monter, même si le backend nul ne l'appelle pas.
fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        AssetPlugin::default(),
        GameAudioPlugin::headless(),
    ));
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
    reel.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        AssetPlugin::default(),
        GameAudioPlugin::offline(),
    ));
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
