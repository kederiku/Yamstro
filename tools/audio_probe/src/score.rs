//! La partition d'un rendu : quels stems, quel coup, quelle couche en fondu, et le départ
//! échantillon-exact. Tous les lecteurs reçoivent le même `InstantSeconds` : c'est le seul
//! mécanisme de mise en phase que `bevy_seedling` offre, et c'est lui que le banc mesure.
//!
//! Deux façons d'armer le départ, mesurées toutes les deux (voir `Start`). Une troisième est
//! un piège : créé en pause puis lancé par `play_at(None, t)`, un lecteur laisse fuir une image
//! de son à la création, puis **reprend** à `t` depuis la position de la fuite, donc décalé.

use bevy::prelude::*;
use bevy_seedling::{context::SampleRate, node::DiffTimestamp, prelude::*};

use crate::{
    graph::{MusicLayers, SfxPool},
    sounds::{HIT, Layer, STEMS},
};

/// Délai d'un départ programmé : l'instant visé est dans le futur, au-delà du lookahead.
pub const LEAD_IN_SECONDS: f64 = 0.5;
/// Le fondu du critère 2 : il part 0,5 s après le départ et dure 1,5 s.
pub const FADE_DELAY_SECONDS: f64 = 0.5;
pub const FADE_SECONDS: f64 = 1.5;

/// Comment le départ est armé.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Start {
    /// Lecteurs créés en lecture, tous porteurs du même `DiffTimestamp` : la forme de l'exemple
    /// `precise_scheduling` de la crate. Départ à l'horodatage, sans fuite. Voie des critères.
    #[default]
    Stamped,
    /// Lecteurs créés en pause, lancés par `play_at(Some(PlayFrom::BEGINNING), t)` : départ à
    /// l'échantillon visé, mais une image de son fuit à la création. Relevé à part.
    Scheduled,
}

/// Comment le gain d'une couche monte du silence à l'unité.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fade {
    /// `VolumeFade::fade_at` : une suite d'événements programmés, un par décibel environ.
    Scheduled,
    /// Un gain écrit à chaque image dans le `VolumeNode` de la couche, lissé par le nœud :
    /// la forme que prendra `update_music_gains` (TASK-101).
    PerFrame,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct Score {
    pub start: Start,
    pub stems: Vec<usize>,
    pub hit: bool,
    /// Couche qui part du silence et monte à l'unité, et la façon dont elle monte.
    pub fade_layer: Option<(usize, Fade)>,
    /// Fichiers chargés pour le seul relevé du décodeur, jamais joués.
    pub decode_only: Vec<&'static str>,
}

/// Ce qu'un fichier décodé déclare, relevé sur l'asset chargé.
#[derive(Clone, Debug)]
pub struct Decoded {
    pub path: &'static str,
    pub frames: u64,
    pub channels: usize,
    pub sample_rate: Option<u32>,
    pub original_sample_rate: u32,
}

#[derive(Resource, Default)]
pub struct Launch {
    stems: Vec<(usize, Handle<AudioSample>)>,
    hit: Option<Handle<AudioSample>>,
    decode_only: Vec<(&'static str, Handle<AudioSample>)>,
    /// Échantillon d'horloge visé par le départ, tel que Firewheel le convertit.
    pub start_sample: Option<i64>,
    /// Début du fondu écrit image par image, sur l'horloge audio.
    fade_from: Option<InstantSeconds>,
    pub decoded: Vec<Decoded>,
}

pub fn load_assets(server: Res<AssetServer>, score: Res<Score>, mut launch: ResMut<Launch>) {
    launch.stems = score
        .stems
        .iter()
        .map(|&i| (i, server.load(STEMS[i])))
        .collect();
    launch.hit = score.hit.then(|| server.load(HIT));
    launch.decode_only = score
        .decode_only
        .iter()
        .map(|&p| (p, server.load(p)))
        .collect();
}

/// Une couche qui part du silence et rejoint l'unité entre deux instants de l'horloge audio.
fn fading_layer(time: &Time<Audio>, start: InstantSeconds, end: InstantSeconds) -> impl Bundle {
    let mut events = AudioEvents::new(time);
    let volume = VolumeNode {
        volume: Volume::SILENT,
        ..Default::default()
    };
    volume.fade_at(Volume::UNITY_GAIN, start, end, &mut events);
    (volume, events)
}

/// Lance tout au même instant, une fois tous les fichiers chargés.
pub fn start_when_loaded(
    server: Res<AssetServer>,
    samples: Res<Assets<AudioSample>>,
    time: Res<Time<Audio>>,
    rate: Res<SampleRate>,
    score: Res<Score>,
    mut launch: ResMut<Launch>,
    mut commands: Commands,
) {
    if launch.start_sample.is_some() {
        return;
    }
    let all = launch
        .stems
        .iter()
        .map(|(_, h)| h)
        .chain(launch.hit.iter())
        .chain(launch.decode_only.iter().map(|(_, h)| h));
    if !all.clone().all(|h| server.is_loaded_with_dependencies(h)) {
        return;
    }

    launch.decoded = launch
        .decode_only
        .iter()
        .filter_map(|(path, handle)| {
            let sample = samples.get(handle)?;
            let resource = sample.get();
            Some(Decoded {
                path,
                frames: resource.len_frames(),
                channels: resource.num_channels().get(),
                sample_rate: resource.sample_rate().map(|r| r.get()),
                original_sample_rate: sample.original_sample_rate().get(),
            })
        })
        .collect();

    let start = match score.start {
        Start::Stamped => time.now(),
        Start::Scheduled => time.delay(DurationSeconds(LEAD_IN_SECONDS)),
    };
    launch.start_sample = Some(start.to_samples(rate.get()).0);
    let armed = |events: &mut AudioEvents| match score.start {
        Start::Stamped => {
            let settings = PlaybackSettings::default();
            settings.play_at(None, start, events);
            settings
        }
        Start::Scheduled => {
            let settings = PlaybackSettings::default().with_playback(false);
            settings.play_at(Some(PlayFrom::BEGINNING), start, events);
            settings
        }
    };

    for (index, handle) in &launch.stems {
        let mut events = AudioEvents::new(&time);
        let settings = armed(&mut events);
        let player = (
            events,
            settings,
            SamplePlayer::new(handle.clone()).looping(),
            MusicLayers,
            Layer(*index),
            DiffTimestamp::new(&time),
        );
        let fade_start = start + DurationSeconds(FADE_DELAY_SECONDS);
        match score.fade_layer {
            Some((layer, Fade::Scheduled)) if layer == *index => {
                let fade_end = fade_start + DurationSeconds(FADE_SECONDS);
                commands.spawn((
                    player,
                    sample_effects![fading_layer(&time, fade_start, fade_end)],
                ));
            }
            Some((layer, Fade::PerFrame)) if layer == *index => {
                let silent = VolumeNode {
                    volume: Volume::SILENT,
                    ..Default::default()
                };
                commands.spawn((player, sample_effects![silent]));
            }
            _ => {
                commands.spawn(player);
            }
        }
    }

    if matches!(score.fade_layer, Some((_, Fade::PerFrame))) {
        launch.fade_from = Some(start + DurationSeconds(FADE_DELAY_SECONDS));
    }

    if let Some(handle) = &launch.hit {
        let mut events = AudioEvents::new(&time);
        let settings = armed(&mut events);
        commands.spawn((
            events,
            settings,
            SamplePlayer::new(handle.clone()),
            SfxPool,
            DiffTimestamp::new(&time),
        ));
    }
}

/// Le fondu écrit image par image : une rampe d'amplitude linéaire de 0 à 1 en 1,5 s, poussée
/// dans le `VolumeNode` de la couche. `Volume::Linear(v)` vaut `v * v` en amplitude, d'où la racine.
pub fn drive_per_frame_fade(
    time: Res<Time<Audio>>,
    score: Res<Score>,
    launch: Res<Launch>,
    layers: Query<(&Layer, &SampleEffects)>,
    mut volumes: Query<&mut VolumeNode>,
) {
    let (Some((target, Fade::PerFrame)), Some(from)) = (score.fade_layer, launch.fade_from) else {
        return;
    };
    let progress = ((time.now().0 - from.0) / FADE_SECONDS).clamp(0.0, 1.0) as f32;
    for (layer, effects) in &layers {
        if layer.0 == target
            && let Ok(mut node) = volumes.get_effect_mut(effects)
        {
            let volume = Volume::Linear(progress.sqrt());
            if node.volume != volume {
                node.volume = volume;
            }
        }
    }
}
