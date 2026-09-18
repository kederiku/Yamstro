//! Pilote hors ligne des tests du backend réel : le contexte audio est activé sans
//! périphérique, et le test appelle `process` lui-même, bloc par bloc, en gardant la sortie.
//! Patron du banc de TASK-95 (`tools/audio_probe/src/offline.rs`), lui-même repris du mock que
//! le backend réserve à ses propres tests. Déterministe, et compatible avec un runner sans son.

use std::{num::NonZeroU32, sync::Mutex, time::Duration};

use audio_system::GameAudioPlugin;
use audioadapter_buffers::direct::InterleavedSlice;
use bevy::{
    asset::LoadState, platform::time::Instant, prelude::*, state::app::StatesPlugin,
    time::TimeUpdateStrategy,
};
use bevy_seedling::{
    context::SampleRate,
    firewheel::{
        ActivateInfo, backend::BackendProcessInfo, node::StreamStatus,
        processor::FirewheelProcessor,
    },
    platform::initialize_stream,
    prelude::*,
};
use game_state::states::{AppState, RunPhase};

pub const RATE: usize = 48_000;
const BLOCK: usize = 128;
/// Six blocs de 128 trames : 768 trames, 16 ms pile à 48 kHz, le pas d'une image.
const BLOCKS_PER_UPDATE: usize = 6;
const CHANNELS: usize = 2;

#[derive(Resource)]
struct OfflineDriver {
    processor: Mutex<Option<FirewheelProcessor>>,
    frames_done: u64,
    /// Un instant que l'horloge n'atteint jamais, remis au moteur comme date de chaque bloc.
    ///
    /// Le moteur **corrige son horloge audio par le temps d'horloge écoulé** depuis le dernier
    /// bloc rendu : en direct, c'est ce qui date un son au bon endroit entre deux blocs. Hors
    /// ligne, ce temps ne veut rien dire, et sous charge il vaut des dizaines de millisecondes :
    /// un son demandé à une image sortait alors une à trois images plus tard, d'un rendu à
    /// l'autre. Le temps écoulé depuis un instant à venir vaut zéro : l'horloge lue par le jeu
    /// est exactement celle des trames rendues, et un rendu ne dépend plus de la machine.
    never: Instant,
}

impl Default for OfflineDriver {
    fn default() -> Self {
        Self {
            processor: Mutex::default(),
            frames_done: 0,
            never: Instant::now() + Duration::from_secs(86_400),
        }
    }
}

struct OfflinePlatformPlugin;

impl Plugin for OfflinePlatformPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OfflineDriver>().add_systems(
            PostStartup,
            start_stream.in_set(SeedlingStartupSystems::StreamInitialization),
        );
    }
}

fn start_stream(
    mut context: ResMut<AudioContext>,
    driver: ResMut<OfflineDriver>,
    commands: Commands,
) {
    let rate = NonZeroU32::new(RATE as u32).expect("fréquence non nulle");
    let processor = context.with(move |ctx| {
        ctx.activate(ActivateInfo {
            sample_rate: rate,
            max_block_frames: NonZeroU32::new(BLOCK as u32).expect("bloc non nul"),
            num_stream_in_channels: CHANNELS as u32,
            num_stream_out_channels: CHANNELS as u32,
            input_to_output_latency_seconds: 0.0,
        })
        .expect("activation du contexte audio")
    });
    *driver.processor.lock().expect("verrou") = Some(processor);
    initialize_stream(SampleRate::new(rate), commands);
}

/// Une application sans fenêtre ni périphérique, avec le **backend réel**, la machine à états
/// du jeu, que le plugin audio exige, et les fichiers de la racine `root`, relative à la crate.
/// Les quatre couches se chargent sous leurs noms de production : le jeu les charge lui-même au
/// démarrage, aucun test ne le fait à sa place. Une racine sans dossier `audio` est le terrain
/// des fichiers manquants ; `../../assets` est la racine de production.
pub fn offline_app_at(root: &str) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        AssetPlugin {
            file_path: root.to_string(),
            ..Default::default()
        },
    ));
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    // Le tampon des paliers de score, enregistré comme le fait `JuicePlugin`, que le plugin
    // audio exige.
    app.add_message::<ui_and_juice::events::ScoreStepPlayed>();
    app.add_plugins((GameAudioPlugin::offline(), OfflinePlatformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )));
    app.finish();
    app.cleanup();
    app
}

/// Fait avancer l'application d'une image et rend 16 ms de son, voie gauche ajoutée à `left`.
pub fn step(app: &mut App, left: &mut Vec<f32>) {
    app.update();
    let mut driver = app.world_mut().resource_mut::<OfflineDriver>();
    let driver = &mut *driver;
    let mut guard = driver.processor.lock().expect("verrou");
    let Some(processor) = guard.as_mut() else {
        return;
    };
    let input = [0.0_f32; BLOCK * CHANNELS];
    let mut output = [0.0_f32; BLOCK * CHANNELS];
    for _ in 0..BLOCKS_PER_UPDATE {
        {
            let input = InterleavedSlice::new(&input, CHANNELS, BLOCK).expect("entrée");
            let mut output =
                InterleavedSlice::new_mut(&mut output, CHANNELS, BLOCK).expect("sortie");
            processor.process(
                &input,
                &mut output,
                BackendProcessInfo {
                    frames: BLOCK,
                    process_timestamp: Some(driver.never),
                    duration_since_stream_start: Duration::from_secs_f64(
                        driver.frames_done as f64 / RATE as f64,
                    ),
                    input_stream_status: StreamStatus::empty(),
                    output_stream_status: StreamStatus::empty(),
                    dropped_frames: 0,
                    process_to_playback_delay: None,
                },
            );
        }
        left.extend(output.chunks_exact(CHANNELS).map(|frame| frame[0]));
        driver.frames_done += BLOCK as u64;
    }
}

/// Rend `seconds` de son.
pub fn render(app: &mut App, seconds: f64, left: &mut Vec<f32>) {
    let target = left.len() + (seconds * RATE as f64) as usize;
    while left.len() < target {
        step(app, left);
    }
}

/// Avance jusqu'à ce que `path` soit chargé. Le chemin se redemande au serveur d'assets : il rend
/// le même handle que celui que la façade garde pour elle.
pub fn wait_loaded(app: &mut App, paths: &[&str], left: &mut Vec<f32>) {
    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<Handle<AudioSample>> =
        paths.iter().map(|p| server.load(p.to_string())).collect();
    for _ in 0..2_000 {
        if handles
            .iter()
            .all(|h| server.is_loaded_with_dependencies(h))
        {
            return;
        }
        step(app, left);
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("les fichiers de test ne se chargent pas : {paths:?}");
}

/// Avance jusqu'à ce que chaque chemin soit **résolu**, chargé ou en échec, puis deux images de
/// plus, le temps que le backend remplace un fichier manquant par le bip.
///
/// Un fichier manquant se résout par une vraie entrée-sortie, en temps d'horloge, alors que les
/// images d'un rendu hors ligne s'enchaînent sans attendre : un test qui joue un clip absent et
/// mesure **l'instant** où il sonne doit d'abord attendre ici, sans quoi le son part quand
/// l'échec arrive, et cet instant dépend de la charge de la machine.
pub fn wait_settled(app: &mut App, paths: &[&str], left: &mut Vec<f32>) {
    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<Handle<AudioSample>> =
        paths.iter().map(|p| server.load(p.to_string())).collect();
    let settled = |handle: &Handle<AudioSample>| {
        matches!(
            server.load_state(handle),
            LoadState::Loaded | LoadState::Failed(_)
        )
    };
    for _ in 0..2_000 {
        if handles.iter().all(settled) {
            step(app, left);
            step(app, left);
            return;
        }
        step(app, left);
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("les fichiers de test ne se résolvent pas : {paths:?}");
}
