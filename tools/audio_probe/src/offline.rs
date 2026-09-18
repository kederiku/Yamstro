//! Pilote hors ligne : le contexte Firewheel est activé sans périphérique et le banc appelle
//! `process` lui-même, bloc par bloc, en gardant la sortie.
//!
//! `bevy_seedling` 0.8.0 n'offre ni rendu hors ligne ni backend factice public : son
//! `MockBackendPlugin` est réservé à ses tests. Un backend Firewheel n'est que du code qui
//! appelle `FirewheelContext::activate` puis `FirewheelProcessor::process` ; ce fichier fait
//! ce que fait `platform/mock.rs`, sans fil et sans attente, donc de façon déterministe.

use std::{num::NonZeroU32, sync::Mutex, time::Duration};

use audioadapter_buffers::direct::InterleavedSlice;
use bevy::prelude::*;
use bevy_seedling::{
    context::SampleRate,
    firewheel::{
        ActivateInfo, backend::BackendProcessInfo, node::StreamStatus,
        processor::FirewheelProcessor,
    },
    platform::initialize_stream,
    prelude::*,
};

pub const RATE: u32 = 48_000;
pub const BLOCK: usize = 128;
/// Six blocs de 128 trames : 768 trames, soit 16 ms pile à 48 kHz, le pas d'une image.
pub const BLOCKS_PER_UPDATE: usize = 6;
pub const UPDATE_STEP: Duration = Duration::from_millis(16);
const CHANNELS: usize = 2;

#[derive(Resource, Default)]
pub struct OfflineDriver {
    processor: Mutex<Option<FirewheelProcessor>>,
    frames_done: u64,
    /// Trames où la voie droite diffère de la gauche : les stems sont mono, elle doit valoir zéro.
    pub stereo_mismatches: u64,
}

impl OfflineDriver {
    /// Rend `blocks` blocs et ajoute la voie gauche à `left`. Sans processeur, ne rend rien.
    pub fn pump(&mut self, blocks: usize, left: &mut Vec<f32>) {
        let mut guard = self.processor.lock().expect("verrou du processeur");
        let Some(processor) = guard.as_mut() else {
            return;
        };
        let input = [0.0_f32; BLOCK * CHANNELS];
        let mut output = [0.0_f32; BLOCK * CHANNELS];
        for _ in 0..blocks {
            {
                let input =
                    InterleavedSlice::new(&input, CHANNELS, BLOCK).expect("tampon d'entrée");
                let mut output = InterleavedSlice::new_mut(&mut output, CHANNELS, BLOCK)
                    .expect("tampon de sortie");
                processor.process(
                    &input,
                    &mut output,
                    BackendProcessInfo {
                        frames: BLOCK,
                        process_timestamp: None,
                        duration_since_stream_start: Duration::from_secs_f64(
                            self.frames_done as f64 / f64::from(RATE),
                        ),
                        input_stream_status: StreamStatus::empty(),
                        output_stream_status: StreamStatus::empty(),
                        dropped_frames: 0,
                        process_to_playback_delay: None,
                    },
                );
            }
            for frame in output.chunks_exact(CHANNELS) {
                left.push(frame[0]);
                if frame[0] != frame[1] {
                    self.stereo_mismatches += 1;
                }
            }
            self.frames_done += BLOCK as u64;
        }
    }
}

/// Remplace `CpalPlatformPlugin` : même place dans le démarrage, aucun périphérique.
pub struct OfflinePlatformPlugin;

impl Plugin for OfflinePlatformPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OfflineDriver>().add_systems(
            PostStartup,
            start_offline_stream.in_set(SeedlingStartupSystems::StreamInitialization),
        );
    }
}

fn start_offline_stream(
    mut context: ResMut<AudioContext>,
    driver: ResMut<OfflineDriver>,
    commands: Commands,
) {
    let rate = NonZeroU32::new(RATE).expect("fréquence non nulle");
    let processor = context.with(move |ctx| {
        ctx.activate(ActivateInfo {
            sample_rate: rate,
            max_block_frames: NonZeroU32::new(BLOCK as u32).expect("bloc non nul"),
            num_stream_in_channels: CHANNELS as u32,
            num_stream_out_channels: CHANNELS as u32,
            input_to_output_latency_seconds: 0.0,
        })
        .expect("activation du contexte Firewheel")
    });
    *driver.processor.lock().expect("verrou du processeur") = Some(processor);
    initialize_stream(SampleRate::new(rate), commands);
}
