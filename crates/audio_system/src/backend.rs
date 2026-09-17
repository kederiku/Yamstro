//! Façade du backend audio : **la seule frontière de la crate avec le backend**.
//!
//! `music.rs`, `sfx.rs`, `pitch.rs`, `bus.rs` et `lib.rs` ne connaissent que ce que ce fichier
//! exporte : [`AudioClip`], [`LayerHandle`], [`Bus`], le trait [`AudioBackend`] et la ressource
//! [`AudioBackendHandle`]. Aucun d'eux ne nomme le backend retenu par l'addendum à l'ADR-006.
//!
//! # La forme du trait, et qui l'appelle
//!
//! | Méthode | Appelée par | Ce qu'elle garantit |
//! | :-- | :-- | :-- |
//! | [`AudioBackend::load_clip`] | TASK-102 (banque de sons) | un index par appel, jamais dédupliqué, valide pour toujours |
//! | [`AudioBackend::play`] | TASK-102 à TASK-106 | un son joué une fois sur un routage, à un volume et une hauteur |
//! | [`AudioBackend::load_layer`] | TASK-100 (quatre couches) | la voie est chargée **et** démarrée, en phase avec les trois autres |
//! | [`AudioBackend::set_layer_gain`] | TASK-101, TASK-106 | le gain d'une couche, poussé à chaque image |
//! | [`AudioBackend::set_bus_gain`] | TASK-98 (volumes), TASK-106 (ducking) | le gain d'un bus, jamais un son relancé |
//!
//! Ces noms ne changent plus : neuf tickets s'appuient dessus.
//!
//! # L'unité : amplitude linéaire, partout
//!
//! Tout `f32` de la façade, volume ou gain, est une **amplitude linéaire** : `0.5` vaut −6 dB,
//! `1.0` l'unité, `0.0` le silence. C'est le sens des formules de l'étape (`sfx * master * local`).
//! Le backend réel convertit : dans son moteur, un volume « linéaire » vaut son carré en
//! amplitude, et un gain écrit tel quel sortirait au carré sans que rien ne le signale.
//!
//! # Le contrat des couches
//!
//! **C'est le backend qui garantit la mise en phase, jamais l'appelant.** Les quatre voies
//! démarrent ensemble, sous un même horodatage, quand les quatre sont enregistrées et chargées ;
//! jamais une par une. Un index hors de `0..LAYER_COUNT` est une erreur de programmation et
//! panique. Recharger un index avant le départ remplace son chemin ; après le départ, l'appel est
//! ignoré et compté. Les deux implémentations tiennent le même contrat.
//!
//! # Les routages
//!
//! [`Bus`] nomme un **routage**, pas un volume utilisateur (ceux-là sont `AudioBusVolumes`,
//! TASK-98, et restent trois). [`Bus::SfxReverb`] rejoint le bus SFX pour le son sec, dont le
//! volume s'applique donc, et alimente seul la réverbération, dont la queue revient aussi par le
//! bus SFX. Le nœud de réverbération ne rend que le signal humide : il est monté en parallèle,
//! jamais en série. Aucune ligne de DSP n'est écrite ici.
//!
//! # La tuyauterie
//!
//! La façade est impérative, le backend réel est piloté par l'ECS : jouer un son y revient à
//! créer une entité. [`SeedlingBackend`] range donc ce qu'on lui demande, et un système privé à
//! ce fichier l'applique au monde en `PostUpdate`, avant les systèmes du backend, qui tournent en
//! `Last` : aucune image de retard. Ce n'est pas un système de jeu.
//!
//! Les deux implémentations sont **toujours compilées** : ni attribut de test, ni feature Cargo.
//! Le choix se fait à la construction du plugin, par [`BackendKind`].

use std::any::Any;

use bevy::prelude::*;
use bevy_seedling::{node::DiffTimestamp, prelude::*};
use firewheel::{
    cpal::cpal::{self, traits::HostTrait},
    nodes::freeverb::FreeverbNode,
};
use log::warn;

/// Les quatre couches de la bande-son : Base, Mélodie, Tension, Climax.
pub const LAYER_COUNT: usize = 4;

/// Index d'un son dans le catalogue du backend. Se construit ici, se lit partout.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AudioClip(pub(crate) u16);

/// Une des quatre couches, `0..LAYER_COUNT`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LayerHandle(pub(crate) u8);

/// Un routage. Voir la doc de tête pour [`Bus::SfxReverb`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bus {
    Master,
    Music,
    Sfx,
    SfxReverb,
}

impl Bus {
    pub const ALL: [Bus; 4] = [Bus::Master, Bus::Music, Bus::Sfx, Bus::SfxReverb];

    const fn index(self) -> usize {
        match self {
            Bus::Master => 0,
            Bus::Music => 1,
            Bus::Sfx => 2,
            Bus::SfxReverb => 3,
        }
    }
}

/// La façade. Objet-sûre : elle est boxée, aucune méthode générique ne s'y ajoute.
pub trait AudioBackend: Any + Send + Sync {
    fn load_clip(&mut self, assets: &AssetServer, path: &str) -> AudioClip;
    fn play(&mut self, clip: AudioClip, bus: Bus, volume: f32, pitch: f32);
    fn load_layer(&mut self, assets: &AssetServer, path: &str, index: u8) -> LayerHandle;
    fn set_layer_gain(&mut self, layer: LayerHandle, gain: f32);
    fn set_bus_gain(&mut self, bus: Bus, gain: f32);
}

/// Ce que les deux implémentations partagent du contrat des couches.
#[derive(Default)]
struct LayerSlots<T> {
    slots: [Option<T>; LAYER_COUNT],
    started: bool,
    ignored: usize,
}

impl<T> LayerSlots<T> {
    /// Range la couche, ou l'ignore si les quatre sont déjà parties.
    fn register(&mut self, index: u8, value: T) -> LayerHandle {
        assert!(
            usize::from(index) < LAYER_COUNT,
            "index de couche {index} : la bande-son compte {LAYER_COUNT} couches, de 0 à 3"
        );
        if self.started {
            self.ignored += 1;
        } else {
            self.slots[usize::from(index)] = Some(value);
        }
        LayerHandle(index)
    }

    fn all_registered(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }
}

// ------------------------------------------------------------------ backend nul

/// Un appel à [`AudioBackend::play`], tel qu'il a été reçu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlayedSound {
    pub clip: AudioClip,
    pub bus: Bus,
    pub volume: f32,
    pub pitch: f32,
}

/// Le backend des tests sans périphérique : il ne joue rien et **journalise tout**.
///
/// Le journal n'est pas une commodité : sans lui, quatre tests imposés de l'étape ne s'écrivent
/// pas. Il n'est donc ni optionnel ni désactivable. Aucun périphérique ouvert, aucun fil audio,
/// et l'`AssetServer` reçu n'est pas même appelé.
pub struct NullBackend {
    played: Vec<PlayedSound>,
    loaded: Vec<String>,
    layers: LayerSlots<String>,
    layer_loads: Vec<(String, u8)>,
    layer_gains: [f32; LAYER_COUNT],
    layer_gain_pushes: usize,
    bus_gains: [f32; Bus::ALL.len()],
    bus_gain_writes: Vec<(Bus, f32)>,
}

impl Default for NullBackend {
    fn default() -> Self {
        Self {
            played: Vec::new(),
            loaded: Vec::new(),
            layers: LayerSlots::default(),
            layer_loads: Vec::new(),
            layer_gains: [0.0; LAYER_COUNT],
            layer_gain_pushes: 0,
            bus_gains: [1.0; Bus::ALL.len()],
            bus_gain_writes: Vec::new(),
        }
    }
}

impl NullBackend {
    /// Un enregistrement par appel à `play`, dans l'ordre des appels.
    pub fn played(&self) -> &[PlayedSound] {
        &self.played
    }

    /// Les chemins passés à `load_clip`, dans l'ordre : l'index rendu est celui de cette liste.
    pub fn loaded(&self) -> &[String] {
        &self.loaded
    }

    /// Les couches enregistrées, `(chemin, index)`, dans l'ordre des index.
    pub fn layers(&self) -> Vec<(String, u8)> {
        (self.layers.slots.iter().enumerate())
            .filter_map(|(index, path)| Some((path.clone()?, index as u8)))
            .collect()
    }

    /// Un enregistrement par appel à `load_layer`, `(chemin, index)`, dans l'ordre des appels,
    /// retenu ou non. [`NullBackend::layers`] est un état plafonné à quatre cases : lui seul ne
    /// distinguerait pas quatre appels de huit.
    pub fn layer_loads(&self) -> &[(String, u8)] {
        &self.layer_loads
    }

    /// Vrai dès que les quatre couches sont enregistrées : elles partent ensemble.
    pub fn layers_started(&self) -> bool {
        self.layers.started
    }

    /// Appels à `load_layer` reçus après le départ, donc ignorés.
    pub fn ignored_layer_loads(&self) -> usize {
        self.layers.ignored
    }

    pub fn layer_gain(&self, layer: LayerHandle) -> f32 {
        self.layer_gains[usize::from(layer.0)]
    }

    /// Le nombre d'appels à `set_layer_gain`. **Un compteur, jamais un journal** : les gains de
    /// couche partent quatre fois par image, et ce backend est aussi celui d'une machine sans
    /// sortie audio, où un journal grossirait sans borne. Une poussée par couche et par image :
    /// lire [`NullBackend::layer_gain`] après chaque image, c'est lire chaque valeur poussée.
    pub fn layer_gain_pushes(&self) -> usize {
        self.layer_gain_pushes
    }

    pub fn bus_gain(&self, bus: Bus) -> f32 {
        self.bus_gains[bus.index()]
    }

    /// Un enregistrement par appel à `set_bus_gain`, dans l'ordre : les gains de bus ne bougent
    /// que sur action de l'utilisateur, leurs poussées se comptent. Les gains de couche, poussés
    /// à chaque image, ne sont pas journalisés : leur dernière valeur est tenue, et leurs
    /// poussées comptées.
    pub fn bus_gain_writes(&self) -> &[(Bus, f32)] {
        &self.bus_gain_writes
    }

    /// Vide les trois journaux, sons joués, poussées de bus et chargements de couche, et remet à
    /// zéro le compteur des poussées de gain, entre deux phases d'un même test. Le catalogue,
    /// les couches et les gains sont un état, pas un journal : ils restent.
    pub fn clear(&mut self) {
        self.played.clear();
        self.bus_gain_writes.clear();
        self.layer_loads.clear();
        self.layer_gain_pushes = 0;
    }
}

impl AudioBackend for NullBackend {
    fn load_clip(&mut self, _assets: &AssetServer, path: &str) -> AudioClip {
        let index = u16::try_from(self.loaded.len()).expect("catalogue de sons plein");
        self.loaded.push(path.to_string());
        AudioClip(index)
    }

    fn play(&mut self, clip: AudioClip, bus: Bus, volume: f32, pitch: f32) {
        self.played.push(PlayedSound {
            clip,
            bus,
            volume,
            pitch,
        });
    }

    fn load_layer(&mut self, _assets: &AssetServer, path: &str, index: u8) -> LayerHandle {
        self.layer_loads.push((path.to_string(), index));
        let handle = self.layers.register(index, path.to_string());
        self.layers.started |= self.layers.all_registered();
        handle
    }

    fn set_layer_gain(&mut self, layer: LayerHandle, gain: f32) {
        self.layer_gains[usize::from(layer.0)] = gain;
        self.layer_gain_pushes += 1;
    }

    fn set_bus_gain(&mut self, bus: Bus, gain: f32) {
        self.bus_gains[bus.index()] = gain;
        self.bus_gain_writes.push((bus, gain));
    }
}

// ----------------------------------------------------------------- backend réel

/// Le backend réel, branche A de l'addendum : il range ce qu'on lui demande, et
/// `apply_seedling_backend` l'applique au monde.
pub struct SeedlingBackend {
    catalog: Vec<Handle<AudioSample>>,
    layers: LayerSlots<Handle<AudioSample>>,
    layer_gains: [f32; LAYER_COUNT],
    bus_gains: [f32; Bus::ALL.len()],
    pending: Vec<PlayedSound>,
}

impl Default for SeedlingBackend {
    fn default() -> Self {
        Self {
            catalog: Vec::new(),
            layers: LayerSlots::default(),
            layer_gains: [0.0; LAYER_COUNT],
            bus_gains: [1.0; Bus::ALL.len()],
            pending: Vec::new(),
        }
    }
}

impl AudioBackend for SeedlingBackend {
    fn load_clip(&mut self, assets: &AssetServer, path: &str) -> AudioClip {
        let index = u16::try_from(self.catalog.len()).expect("catalogue de sons plein");
        self.catalog.push(assets.load(path.to_string()));
        AudioClip(index)
    }

    fn play(&mut self, clip: AudioClip, bus: Bus, volume: f32, pitch: f32) {
        self.pending.push(PlayedSound {
            clip,
            bus,
            volume,
            pitch,
        });
    }

    fn load_layer(&mut self, assets: &AssetServer, path: &str, index: u8) -> LayerHandle {
        self.layers.register(index, assets.load(path.to_string()))
    }

    fn set_layer_gain(&mut self, layer: LayerHandle, gain: f32) {
        self.layer_gains[usize::from(layer.0)] = gain;
    }

    fn set_bus_gain(&mut self, bus: Bus, gain: f32) {
        self.bus_gains[bus.index()] = gain;
    }
}

/// Amplitude linéaire de la façade vers le volume du moteur. Le zéro est le silence exact ;
/// l'unité s'écrit telle quelle, sans passer par un logarithme.
fn amplitude(gain: f32) -> Volume {
    if gain <= 0.0 {
        Volume::SILENT
    } else if gain == 1.0 {
        Volume::UNITY_GAIN
    } else {
        Volume::Decibels(20.0 * gain.log10())
    }
}

/// Le nœud de volume d'un routage.
#[derive(Component, Clone, Copy)]
struct BusNode(Bus);

/// Le lecteur d'une couche.
#[derive(Component, Clone, Copy)]
struct LayerVoice(u8);

#[derive(NodeLabel, PartialEq, Eq, Debug, Hash, Clone)]
struct MusicBus;
#[derive(NodeLabel, PartialEq, Eq, Debug, Hash, Clone)]
struct SfxBus;
#[derive(NodeLabel, PartialEq, Eq, Debug, Hash, Clone)]
struct SfxReverbBus;

#[derive(PoolLabel, PartialEq, Eq, Debug, Hash, Clone)]
struct LayerPool;
#[derive(PoolLabel, PartialEq, Eq, Debug, Hash, Clone)]
struct MasterPool;
#[derive(PoolLabel, PartialEq, Eq, Debug, Hash, Clone)]
struct MusicPool;
#[derive(PoolLabel, PartialEq, Eq, Debug, Hash, Clone)]
struct SfxPool;
#[derive(PoolLabel, PartialEq, Eq, Debug, Hash, Clone)]
struct SfxReverbPool;

/// Le graphe, posé à la main sur le gabarit vide : le gabarit par défaut du backend impose un
/// limiteur et des pools dont le jeu n'a pas l'usage.
fn build_seedling_graph(mut commands: Commands) {
    commands
        .spawn((MainBus, VolumeNode::default(), BusNode(Bus::Master)))
        .connect(AudioGraphOutput);
    commands
        .spawn((MusicBus, VolumeNode::default(), BusNode(Bus::Music)))
        .connect(MainBus);
    commands
        .spawn((SfxBus, VolumeNode::default(), BusNode(Bus::Sfx)))
        .connect(MainBus);

    // Le sec rejoint le bus SFX ; la réverbération, humide seulement, y revient en parallèle.
    let reverb = commands
        .spawn(FreeverbNode::default())
        .connect(SfxBus)
        .head();
    commands
        .spawn((SfxReverbBus, VolumeNode::default(), BusNode(Bus::SfxReverb)))
        .connect(SfxBus)
        .connect(reverb);

    commands
        .spawn((
            SamplerPool(LayerPool),
            sample_effects![VolumeNode::default()],
        ))
        .connect(MusicBus);
    commands.spawn(SamplerPool(MasterPool)).connect(MainBus);
    commands.spawn(SamplerPool(MusicPool)).connect(MusicBus);
    commands.spawn(SamplerPool(SfxPool)).connect(SfxBus);
    commands
        .spawn(SamplerPool(SfxReverbPool))
        .connect(SfxReverbBus);
}

type BusVolumes<'w, 's> = Query<'w, 's, (&'static mut VolumeNode, &'static BusNode)>;
type LayerVolumes<'w, 's> = Query<'w, 's, &'static mut VolumeNode, Without<BusNode>>;

/// La tuyauterie : ce que la façade a rangé part vers le monde. Voir la doc de tête.
fn apply_seedling_backend(
    mut handle: ResMut<AudioBackendHandle>,
    server: Res<AssetServer>,
    time: Res<Time<Audio>>,
    mut buses: BusVolumes,
    voices: Query<(&LayerVoice, &SampleEffects)>,
    mut layer_volumes: LayerVolumes,
    mut commands: Commands,
) {
    let Some(backend) = handle.seedling_mut() else {
        return;
    };

    for sound in backend.pending.drain(..) {
        let Some(sample) = backend.catalog.get(usize::from(sound.clip.0)) else {
            continue;
        };
        let player = SamplePlayer::new(sample.clone()).with_volume(amplitude(sound.volume));
        let settings = PlaybackSettings::default().with_speed(f64::from(sound.pitch));
        match sound.bus {
            Bus::Master => commands.spawn((player, settings, MasterPool)),
            Bus::Music => commands.spawn((player, settings, MusicPool)),
            Bus::Sfx => commands.spawn((player, settings, SfxPool)),
            Bus::SfxReverb => commands.spawn((player, settings, SfxReverbPool)),
        };
    }

    for (mut node, bus) in &mut buses {
        let volume = amplitude(backend.bus_gains[bus.0.index()]);
        if node.volume != volume {
            node.volume = volume;
        }
    }

    // Les quatre couches partent ensemble : créées en lecture, sous un même horodatage. Créées
    // en pause puis lancées plus tard, elles laisseraient fuir une image de son et reprendraient
    // décalées (mesuré à TASK-95).
    let loaded = |sample: &Option<Handle<AudioSample>>| {
        sample
            .as_ref()
            .is_some_and(|s| server.is_loaded_with_dependencies(s))
    };
    if !backend.layers.started && backend.layers.slots.iter().all(loaded) {
        backend.layers.started = true;
        let start = time.now();
        for (index, sample) in backend.layers.slots.iter().flatten().enumerate() {
            let mut events = AudioEvents::new(&time);
            let settings = PlaybackSettings::default();
            settings.play_at(None, start, &mut events);
            let gain = VolumeNode {
                volume: amplitude(backend.layer_gains[index]),
                ..Default::default()
            };
            commands.spawn((
                events,
                settings,
                SamplePlayer::new(sample.clone()).looping(),
                LayerPool,
                LayerVoice(index as u8),
                DiffTimestamp::new(&time),
                sample_effects![gain],
            ));
        }
    }

    for (voice, effects) in &voices {
        if let Ok(mut node) = layer_volumes.get_effect_mut(effects) {
            let volume = amplitude(backend.layer_gains[usize::from(voice.0)]);
            if node.volume != volume {
                node.volume = volume;
            }
        }
    }
}

// ------------------------------------------------------- ressource et montage

/// La ressource porteuse. `Resource` seule : en 0.19 elle est un sous-trait de `Component`, et
/// un type ne dérive pas les deux. Insérée par le plugin, et par personne d'autre ; tous les
/// systèmes de la crate l'empruntent en `ResMut`, un seul à la fois parle au backend.
#[derive(Resource)]
pub struct AudioBackendHandle(pub Box<dyn AudioBackend>);

impl AudioBackendHandle {
    /// Le journal, si le backend monté est le nul.
    pub fn null(&self) -> Option<&NullBackend> {
        (self.0.as_ref() as &dyn Any).downcast_ref()
    }

    /// Le journal en écriture, pour `clear()`.
    pub fn null_mut(&mut self) -> Option<&mut NullBackend> {
        (self.0.as_mut() as &mut dyn Any).downcast_mut()
    }

    fn seedling_mut(&mut self) -> Option<&mut SeedlingBackend> {
        (self.0.as_mut() as &mut dyn Any).downcast_mut()
    }
}

/// Quelle variante le plugin monte. Un discriminant `Copy` : le plugin ne peut pas porter
/// l'instance, `build` ne reçoit qu'un `&self`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BackendKind {
    /// Le backend réel sur la sortie audio de la machine.
    Real,
    /// Le backend réel **sans plateforme de sortie** : celui qui le monte active le contexte
    /// audio et le fait avancer lui-même. C'est la voie des tests sur le son réel.
    Offline,
    /// Le backend nul journalisant.
    Null,
}

/// Une machine sans sortie audio fait échouer le démarrage du flux, et cette erreur-là panique.
/// Le jeu tourne alors muet, sur le backend nul.
pub fn resolve_kind(requested: BackendKind, output_available: bool) -> BackendKind {
    match requested {
        BackendKind::Real if !output_available => BackendKind::Null,
        kind => kind,
    }
}

fn output_available() -> bool {
    cpal::default_host().default_output_device().is_some()
}

/// Monte la variante demandée. Appelée par `GameAudioPlugin::build`, et par elle seule.
/// Les variantes réelles exigent un `AssetPlugin` déjà monté.
pub(crate) fn install(app: &mut App, requested: BackendKind) {
    let kind = match requested {
        BackendKind::Real => resolve_kind(requested, output_available()),
        kind => kind,
    };
    if kind != requested {
        warn!("aucune sortie audio : le jeu tourne muet, sur le backend nul");
    }

    match kind {
        BackendKind::Null => {
            app.insert_resource(AudioBackendHandle(Box::new(NullBackend::default())));
            return;
        }
        BackendKind::Real => {
            app.add_plugins(SeedlingPlugins);
        }
        BackendKind::Offline => {
            app.add_plugins(bevy_seedling::SeedlingCorePlugin);
        }
    }
    app.register_node::<FreeverbNode>()
        .insert_resource(AudioGraphTemplate::Empty)
        .insert_resource(AudioBackendHandle(Box::new(SeedlingBackend::default())))
        .add_systems(Startup, build_seedling_graph)
        .add_systems(PostUpdate, apply_seedling_backend);
}
