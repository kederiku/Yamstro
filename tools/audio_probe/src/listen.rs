//! Mode d'écoute : la sortie audio réelle, une touche par critère, pour l'oreille.
//!
//! Les touches se lisent sur l'entrée standard (une touche puis Entrée) : le banc n'ouvre
//! aucune fenêtre. Les valeurs de l'addendum ne viennent pas d'ici, elles viennent de `--measure`.
//! Sur le Web il n'y a pas d'entrée standard : les couches et le coup partent seuls une fois
//! chargés, ce qui suffit à lier le chemin de lecture pour la pesée du backend.

use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, prelude::*};
use bevy_seedling::{node::DiffTimestamp, prelude::*};

use crate::{
    graph::{GraphPlan, MusicLayers, SfxPool, build_graph},
    sounds::{HIT, Layer, STEMS},
};

#[derive(Resource)]
struct Sounds {
    stems: Vec<Handle<AudioSample>>,
    hit: Handle<AudioSample>,
}

pub fn run() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
        LogPlugin::default(),
        AssetPlugin {
            file_path: "assets".to_string(),
            ..Default::default()
        },
        SeedlingPlugins,
    ))
    .insert_resource(AudioGraphTemplate::Empty)
    .insert_resource(GraphPlan::UNITY)
    .add_systems(Startup, (build_graph, load_sounds));

    #[cfg(not(target_arch = "wasm32"))]
    keyboard::install(&mut app);
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, (autoplay, autofade).chain());

    app.run();
}

fn load_sounds(server: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(Sounds {
        stems: STEMS.iter().map(|name| server.load(*name)).collect(),
        hit: server.load(HIT),
    });
}

/// Les quatre couches, en phase : créées en lecture, toutes porteuses du même horodatage.
/// La couche 3 part muette ; son `VolumeNode` porte ses propres événements, pour le fondu.
fn spawn_layers(sounds: &Sounds, time: &Time<Audio>, commands: &mut Commands) {
    let start = time.now();
    for (index, handle) in sounds.stems.iter().enumerate() {
        let mut events = AudioEvents::new(time);
        let settings = PlaybackSettings::default();
        settings.play_at(None, start, &mut events);
        let volume = if index == 3 {
            Volume::SILENT
        } else {
            Volume::UNITY_GAIN
        };
        let layer_gain = VolumeNode {
            volume,
            ..Default::default()
        };
        commands.spawn((
            events,
            settings,
            SamplePlayer::new(handle.clone()).looping(),
            MusicLayers,
            Layer(index),
            DiffTimestamp::new(time),
            sample_effects![(layer_gain, AudioEvents::new(time))],
        ));
    }
}

fn spawn_hit(sounds: &Sounds, commands: &mut Commands) {
    commands.spawn((SamplePlayer::new(sounds.hit.clone()), SfxPool));
}

type LayerGains<'w, 's> =
    Query<'w, 's, (&'static VolumeNode, &'static mut AudioEvents), Without<SampleEffects>>;

/// Monte la couche 3 du silence à l'unité en 1,5 s. Rend `true` si la couche existait.
fn fade_in_last_layer(layers: &Query<(&Layer, &SampleEffects)>, gains: &mut LayerGains) -> bool {
    let mut found = false;
    for (layer, effects) in layers {
        if layer.0 == 3
            && let Ok((volume, mut events)) = gains.get_effect_mut(effects)
        {
            volume.fade_to(Volume::UNITY_GAIN, DurationSeconds(1.5), &mut events);
            found = true;
        }
    }
    found
}

#[cfg(target_arch = "wasm32")]
fn autoplay(
    sounds: Res<Sounds>,
    server: Res<AssetServer>,
    time: Res<Time<Audio>>,
    mut started: Local<bool>,
    mut commands: Commands,
) {
    let loaded = sounds
        .stems
        .iter()
        .chain(std::iter::once(&sounds.hit))
        .all(|handle| server.is_loaded_with_dependencies(handle));
    if loaded && !*started {
        *started = true;
        spawn_layers(&sounds, &time, &mut commands);
        spawn_hit(&sounds, &mut commands);
    }
}

#[cfg(target_arch = "wasm32")]
fn autofade(
    layers: Query<(&Layer, &SampleEffects)>,
    mut gains: LayerGains,
    mut faded: Local<bool>,
) {
    if !*faded {
        *faded = fade_in_last_layer(&layers, &mut gains);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod keyboard {
    use std::sync::{Mutex, mpsc};

    use bevy::prelude::*;
    use bevy_seedling::prelude::*;

    use super::{LayerGains, Sounds, fade_in_last_layer, spawn_hit, spawn_layers};
    use crate::{
        graph::{MusicBus, SfxBus, bus_volume},
        sounds::Layer,
    };

    #[derive(Resource)]
    struct Keys(Mutex<mpsc::Receiver<char>>);

    /// La touche lue à cette image, s'il y en a une.
    #[derive(Resource, Default)]
    struct Pressed(Option<char>);

    #[derive(Resource, Default)]
    struct Attenuated {
        master: bool,
        music: bool,
        sfx: bool,
    }

    pub fn install(app: &mut App) {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::stdin().lines().map_while(Result::ok) {
                if let Some(key) = line.trim().chars().next() {
                    let _ = sender.send(key);
                }
            }
        });
        println!("1 : quatre stems en phase, en boucle (la couche 3 part muette)");
        println!("2 : fondu de la couche 3, du silence à l'unité en 1,5 s");
        println!("3 : mult_hit sur le bus SFX, avec sa réverbération");
        println!("m, u, s : -20 dB sur Master, Music, SFX (bascule)   q : quitter");
        app.insert_resource(Keys(Mutex::new(receiver)))
            .init_resource::<Pressed>()
            .init_resource::<Attenuated>()
            .add_systems(Update, (read_key, play_keys, mix_keys).chain());
    }

    fn read_key(keys: Res<Keys>, mut pressed: ResMut<Pressed>) {
        pressed.0 = keys.0.lock().expect("verrou des touches").try_recv().ok();
    }

    /// `1` lance les quatre stems en phase, `3` joue le coup sur le bus SFX, `q` quitte.
    fn play_keys(
        pressed: Res<Pressed>,
        sounds: Res<Sounds>,
        time: Res<Time<Audio>>,
        mut commands: Commands,
        mut exit: MessageWriter<AppExit>,
    ) {
        match pressed.0 {
            Some('1') => spawn_layers(&sounds, &time, &mut commands),
            Some('3') => spawn_hit(&sounds, &mut commands),
            Some('q') => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }

    type BusGains<'w, 's> = Query<
        'w,
        's,
        (
            &'static mut VolumeNode,
            Has<MainBus>,
            Has<MusicBus>,
            Has<SfxBus>,
        ),
    >;

    /// `2` monte la couche 3 du silence à l'unité en 1,5 s ; `m`, `u`, `s` basculent -20 dB.
    fn mix_keys(
        pressed: Res<Pressed>,
        mut attenuated: ResMut<Attenuated>,
        layers: Query<(&Layer, &SampleEffects)>,
        mut nodes: ParamSet<(LayerGains, BusGains)>,
    ) {
        match pressed.0 {
            Some('2') => {
                fade_in_last_layer(&layers, &mut nodes.p0());
            }
            Some(key @ ('m' | 'u' | 's')) => {
                let flag = match key {
                    'm' => &mut attenuated.master,
                    'u' => &mut attenuated.music,
                    _ => &mut attenuated.sfx,
                };
                *flag = !*flag;
                let db = if *flag { -20.0 } else { 0.0 };
                for (mut node, master, music, sfx) in &mut nodes.p1() {
                    if (key == 'm' && master) || (key == 'u' && music) || (key == 's' && sfx) {
                        node.volume = bus_volume(db);
                    }
                }
            }
            _ => {}
        }
    }
}
