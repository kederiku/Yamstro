//! Le backend **réel**, sur le son qu'il rend : sans périphérique, donc sur tout runner.
//!
//! Le backend nul vérifie des décisions ; il est aveugle au pire défaut de l'étape, qui est
//! **vert et muet** : des voies vides, des gains au carré, une réverbération qui mange le son
//! sec. Ces trois tests écoutent le signal.

#[path = "support/offline.rs"]
mod offline;

use audio_system::{
    AudioBackendHandle, AudioBusVolumes, Bus,
    bus::{layer_gain, local_gain, music_gain, sfx_volume},
};
use bevy::prelude::*;
use offline::{RATE, offline_app, render, wait_loaded};

const LAYER_FRAMES: usize = 48_000;
const IMPULSE_SPACING: usize = 480;
const HIT_FRAMES: usize = 5_760;
/// Les trois curseurs à l'unité.
const UNITY: AudioBusVolumes = AudioBusVolumes {
    master: 1.0,
    music: 1.0,
    sfx: 1.0,
};
const LAYERS: [&str; 4] = ["layer_0.ogg", "layer_1.ogg", "layer_2.ogg", "layer_3.ogg"];

fn with_backend<R>(
    app: &mut App,
    act: impl FnOnce(&mut AudioBackendHandle, &AssetServer) -> R,
) -> R {
    let server = app.world().resource::<AssetServer>().clone();
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    act(&mut handle, &server)
}

fn peak(signal: &[f32]) -> f32 {
    signal.iter().fold(0.0, |m, v| m.max(v.abs()))
}

fn dbfs_rms(signal: &[f32]) -> f64 {
    let mean = signal
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum::<f64>()
        / signal.len() as f64;
    if mean == 0.0 {
        f64::NEG_INFINITY
    } else {
        10.0 * mean.log10()
    }
}

/// Joue `hit.ogg` une fois après avoir posé les volumes utilisateur, **par la ressource** (elle est
/// la seule source de vérité : un gain posé directement sur la façade serait écrasé à la première
/// poussée), et rend le signal **à partir de
/// l'appel à `play`**, sans le rogner : la latence jusqu'au premier échantillon est la même d'un
/// rendu à l'autre, ce qui permet de comparer deux routages trame à trame.
fn render_hit_from_play(bus: Bus, volume: f32, pitch: f32, volumes: AudioBusVolumes) -> Vec<f32> {
    let mut app = offline_app();
    let mut left = Vec::new();
    app.world_mut().insert_resource(volumes);
    let clip = with_backend(&mut app, |handle, server| {
        handle.0.load_clip(server, "hit.ogg")
    });
    wait_loaded(&mut app, &["hit.ogg"], &mut left);
    // Un gain de bus se lisse sur plusieurs dizaines de millisecondes : on le laisse s'établir.
    render(&mut app, 0.5, &mut left);
    assert_eq!(peak(&left), 0.0, "du son sort avant le premier `play`");

    with_backend(&mut app, |handle, _| {
        handle.0.play(clip, bus, volume, pitch)
    });
    let before = left.len();
    render(&mut app, 1.0, &mut left);
    left.split_off(before)
}

/// Première trame non nulle.
fn onset(signal: &[f32]) -> usize {
    signal
        .iter()
        .position(|v| *v != 0.0)
        .expect("le coup ne sort pas")
}

/// Le même rendu, à partir du premier échantillon non nul du coup.
fn render_hit(bus: Bus, volume: f32, pitch: f32, volumes: AudioBusVolumes) -> Vec<f32> {
    let mut signal = render_hit_from_play(bus, volume, pitch, volumes);
    let start = onset(&signal);
    signal.split_off(start)
}

#[test]
fn test_four_layers_start_in_phase_and_stay() {
    let mut app = offline_app();
    let mut left = Vec::new();
    let layers = with_backend(&mut app, |handle, server| {
        let load = |(index, path): (usize, &&str)| {
            let layer = handle.0.load_layer(server, path, index as u8);
            handle.0.set_layer_gain(layer, 1.0);
            layer
        };
        LAYERS.iter().enumerate().map(load).collect::<Vec<_>>()
    });
    wait_loaded(&mut app, &LAYERS, &mut left);
    render(&mut app, 3.5, &mut left);

    // Rien ne fuit avant le départ : le premier échantillon non nul est l'impulsion de la
    // couche 0, à sa trame 0. Des lecteurs créés en pause laisseraient fuir une image de son.
    let origin = left
        .iter()
        .position(|v| *v != 0.0)
        .expect("aucune couche ne sort");
    assert!(
        left[origin] > 0.3,
        "le départ n'est pas l'impulsion de la couche 0 : {}",
        left[origin]
    );

    // Trois bouclages, quatre couches : chaque impulsion est à sa trame, exactement.
    for turn in 0..3 {
        for layer in 0..4 {
            let expected = origin + turn * LAYER_FRAMES + layer * IMPULSE_SPACING;
            let window = expected - 100..expected + 100;
            let found = window
                .clone()
                .max_by(|&a, &b| left[a].total_cmp(&left[b]))
                .expect("fenêtre");
            assert!(
                left[found] > 0.3,
                "couche {layer}, bouclage {turn} : impulsion absente, la voie est vide"
            );
            assert_eq!(
                found, expected,
                "couche {layer}, bouclage {turn} : hors phase"
            );
        }
    }

    // Un gain poussé après le départ agit, et sur sa couche seule : c'est l'usage de chaque
    // image du jeu. La couche 3 se tait, la couche 0 reste, et la phase ne bouge pas.
    with_backend(&mut app, |handle, _| {
        handle.0.set_layer_gain(layers[3], 0.0)
    });
    let turn = (left.len() - origin).div_ceil(LAYER_FRAMES) + 1;
    render(&mut app, 3.0, &mut left);
    let silenced = origin + turn * LAYER_FRAMES + 3 * IMPULSE_SPACING;
    let kept = origin + turn * LAYER_FRAMES;
    assert!(
        (-0.1..0.1).contains(&left[silenced]),
        "la couche 3 sonne encore : {}",
        left[silenced]
    );
    assert!(
        left[kept] > 0.3,
        "la couche 0 s'est tue avec la couche 3 : {}",
        left[kept]
    );
}

/// Un rapport d'amplitudes, en pour-mille entiers : les assertions comparent des entiers à une
/// plage, jamais deux flottants à un epsilon près.
fn per_mille(value: f32, reference: f32) -> i64 {
    (f64::from(value) / f64::from(reference) * 1000.0).round() as i64
}

#[test]
fn test_play_applies_linear_volume_pitch_and_bus_gains() {
    let reference = render_hit(Bus::Sfx, 1.0, 1.0, UNITY);
    let full = peak(&reference[..HIT_FRAMES]);
    assert!(full > 0.5, "le coup de référence est trop faible : {full}");
    let ratio = |signal: &[f32]| per_mille(peak(&signal[..HIT_FRAMES]), full);

    // Le volume est une amplitude linéaire : 0,5 sort à la moitié, pas au quart.
    let half = ratio(&render_hit(Bus::Sfx, 0.5, 1.0, UNITY));
    assert!(
        (495..=505).contains(&half),
        "volume 0,5 : {half} pour mille"
    );

    // Les gains de bus aussi, et ils se composent : SFX puis Master.
    let sfx = ratio(&render_hit(
        Bus::Sfx,
        1.0,
        1.0,
        AudioBusVolumes { sfx: 0.1, ..UNITY },
    ));
    assert!(
        (99..=101).contains(&sfx),
        "bus SFX à 0,1 : {sfx} pour mille"
    );
    let both = ratio(&render_hit(
        Bus::Sfx,
        1.0,
        1.0,
        AudioBusVolumes {
            sfx: 0.5,
            master: 0.5,
            ..UNITY
        },
    ));
    assert!(
        (247..=253).contains(&both),
        "SFX 0,5 et Master 0,5 : {both} pour mille"
    );

    // Le bus Music ne touche pas un son du bus SFX.
    let untouched = ratio(&render_hit(
        Bus::Sfx,
        1.0,
        1.0,
        AudioBusVolumes {
            music: 0.1,
            ..UNITY
        },
    ));
    assert!(
        (990..=1010).contains(&untouched),
        "bus Music à 0,1 : {untouched} pour mille"
    );

    // Hauteur 2 : une octave plus haut, le coup dure moitié moins.
    let octave = render_hit(Bus::Sfx, 1.0, 2.0, UNITY);
    let length = |signal: &[f32]| {
        signal
            .iter()
            .rposition(|v| v.abs() > 1e-4)
            .expect("silence")
            + 1
    };
    let halved = per_mille(length(&octave) as f32, length(&reference) as f32);
    assert!(
        (450..=550).contains(&halved),
        "hauteur 2 : durée à {halved} pour mille"
    );
}

#[test]
fn test_only_the_reverb_routing_leaves_a_tail() {
    let tail = HIT_FRAMES + RATE / 50..HIT_FRAMES + RATE / 50 + RATE * 3 / 10;
    let dry_from_play = render_hit_from_play(Bus::Sfx, 1.0, 1.0, UNITY);
    let wet_from_play = render_hit_from_play(Bus::SfxReverb, 1.0, 1.0, UNITY);

    // **Le son sec est là, et il arrive au même instant.** La réverbération ne rend que
    // l'humide et sa première réflexion met des dizaines de millisecondes à sortir : montée en
    // série, elle retarderait l'attaque et la remplacerait. Une crête pendant le coup ne prouve
    // rien, la queue seule la dépasse ; c'est l'attaque qui se compare, trame à trame.
    let start = onset(&dry_from_play);
    assert_eq!(
        onset(&wet_from_play),
        start,
        "le routage réverbéré a perdu son attaque sèche"
    );
    let (dry, wet) = (&dry_from_play[start..], &wet_from_play[start..]);
    let attack = per_mille(peak(&wet[..IMPULSE_SPACING]), peak(&dry[..IMPULSE_SPACING]));
    assert!(
        (990..=1010).contains(&attack),
        "l'attaque du routage réverbéré n'est pas celle du son sec : {attack} pour mille"
    );

    let (dry_tail, wet_tail) = (dbfs_rms(&dry[tail.clone()]), dbfs_rms(&wet[tail]));
    assert!(
        dry_tail < -80.0,
        "le bus SFX laisse une queue : {dry_tail:.1} dBFS"
    );
    assert!(
        wet_tail > -40.0,
        "le routage réverbéré ne résonne pas : {wet_tail:.1} dBFS"
    );

    // Le volume SFX s'applique au routage réverbéré : il rejoint le bus SFX.
    let quiet = render_hit(
        Bus::SfxReverb,
        1.0,
        1.0,
        AudioBusVolumes { sfx: 0.1, ..UNITY },
    );
    let ratio = per_mille(
        peak(&quiet[..IMPULSE_SPACING]),
        peak(&wet[..IMPULSE_SPACING]),
    );
    assert!(
        (95..=105).contains(&ratio),
        "bus SFX à 0,1 sur le routage réverbéré : {ratio} pour mille"
    );
}

/// TASK-98 : **ce que l'auditeur entend est la formule, une fois**. Les curseurs s'appliquent sur
/// les bus ; la façade ne reçoit que la part locale. Remettre à la façade la formule entière
/// appliquerait les curseurs deux fois : 31 pour mille au lieu de 125, 4 au lieu de 62.
#[test]
fn test_effective_volume_matches_the_formulas() {
    // Un son : sfx 0,5, master 0,5, volume local 0,5.
    let volumes = AudioBusVolumes {
        master: 0.5,
        sfx: 0.5,
        ..UNITY
    };
    assert_eq!(sfx_volume(&volumes, 0.5), 0.125);
    let reference = render_hit(Bus::Sfx, local_gain(1.0), 1.0, UNITY);
    let heard = render_hit(Bus::Sfx, local_gain(0.5), 1.0, volumes);
    let sound = per_mille(peak(&heard[..HIT_FRAMES]), peak(&reference[..HIT_FRAMES]));
    assert!(
        (123..=127).contains(&sound),
        "son entendu à {sound} pour mille, 125 attendus"
    );

    // Une couche : music 0,5, master 0,5, poids 0,5, ducking 0,5. Les trois autres se taisent.
    let volumes = AudioBusVolumes {
        master: 0.5,
        music: 0.5,
        ..UNITY
    };
    assert_eq!(music_gain(&volumes, 0.5, 0.5), 0.0625);
    let first_impulse = |volumes: AudioBusVolumes, gain: f32| {
        let mut app = offline_app();
        let mut left = Vec::new();
        app.world_mut().insert_resource(volumes);
        with_backend(&mut app, |handle, server| {
            for (index, path) in LAYERS.iter().enumerate() {
                let layer = handle.0.load_layer(server, path, index as u8);
                handle
                    .0
                    .set_layer_gain(layer, if index == 0 { gain } else { 0.0 });
            }
        });
        wait_loaded(&mut app, &LAYERS, &mut left);
        render(&mut app, 0.5, &mut left);
        left[left
            .iter()
            .position(|v| *v != 0.0)
            .expect("la couche 0 ne sort pas")]
    };
    let layer = per_mille(
        first_impulse(volumes, layer_gain(0.5, 0.5)),
        first_impulse(UNITY, layer_gain(1.0, 1.0)),
    );
    assert!(
        (61..=64).contains(&layer),
        "couche entendue à {layer} pour mille, 62,5 attendus"
    );
}
