//! Le graphe du banc : trois bus, deux pools, aucun limiteur.
//!
//! Le gabarit par défaut de `bevy_seedling` (`AudioGraphTemplate::Game`) place un `LimiterNode`
//! après `MainBus` : il masquerait un clic ou une saturation. Le banc part du gabarit `Empty`.

use bevy::prelude::*;
use bevy_seedling::prelude::*;

/// Bus des quatre couches musicales.
#[derive(NodeLabel, PartialEq, Eq, Debug, Hash, Clone)]
pub struct MusicBus;

/// Bus des effets sonores ; il porte la réverbération quand le plan la demande.
#[derive(NodeLabel, PartialEq, Eq, Debug, Hash, Clone)]
pub struct SfxBus;

/// Pool des couches musicales : un `VolumeNode` par lecteur, c'est lui qui porte le fondu.
#[derive(PoolLabel, PartialEq, Eq, Debug, Hash, Clone)]
pub struct MusicLayers;

/// Pool des effets sonores.
#[derive(PoolLabel, PartialEq, Eq, Debug, Hash, Clone)]
pub struct SfxPool;

/// Ce que le graphe porte pour un rendu : la réverbération et le gain de chacun des trois bus.
#[derive(Resource, Clone, Copy, Debug)]
pub struct GraphPlan {
    pub reverb: bool,
    pub master_db: f32,
    pub music_db: f32,
    pub sfx_db: f32,
}

impl GraphPlan {
    pub const UNITY: Self = Self {
        reverb: true,
        master_db: 0.0,
        music_db: 0.0,
        sfx_db: 0.0,
    };
}

/// Gain d'un bus. Zéro décibel s'écrit en gain unité exact, sans passer par `powf`.
pub fn bus_volume(db: f32) -> Volume {
    if db == 0.0 {
        Volume::UNITY_GAIN
    } else {
        Volume::Decibels(db)
    }
}

fn bus_node(db: f32) -> VolumeNode {
    VolumeNode {
        volume: bus_volume(db),
        ..Default::default()
    }
}

/// Master = `MainBus` vers la sortie ; Music et SFX rejoignent `MainBus`.
///
/// La réverbération est le `FreeverbNode` de Firewheel : aucune ligne de DSP n'est écrite ici.
/// Il ne rend que le signal humide (`dry` vaut zéro dans `freeverb.rs` et rien ne le règle) :
/// chaîné en série, il ferait disparaître le son sec. Le bus SFX sort donc deux fois, vers
/// `MainBus` pour le sec et vers la réverbération pour la queue.
pub fn build_graph(mut commands: Commands, plan: Res<GraphPlan>) {
    commands
        .spawn((MainBus, bus_node(plan.master_db)))
        .connect(AudioGraphOutput);
    commands
        .spawn((MusicBus, bus_node(plan.music_db)))
        .connect(MainBus);

    let sfx = commands
        .spawn((SfxBus, bus_node(plan.sfx_db)))
        .connect(MainBus)
        .head();
    if plan.reverb {
        let reverb = commands
            .spawn(FreeverbNode::default())
            .connect(MainBus)
            .head();
        commands.entity(sfx).connect(reverb);
    }

    commands
        .spawn((
            SamplerPool(MusicLayers),
            sample_effects![VolumeNode::default()],
        ))
        .connect(MusicBus);
    commands.spawn(SamplerPool(SfxPool)).connect(SfxBus);
}
