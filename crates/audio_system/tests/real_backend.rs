//! Le backend **réel**, sur le son qu'il rend : sans périphérique, donc sur tout runner.
//!
//! Le backend nul vérifie des décisions ; il est aveugle au pire défaut de l'étape, qui est
//! **vert et muet** : des voies vides, des gains au carré, une réverbération qui mange le son
//! sec, une voie détruite par un changement d'état. Ces tests écoutent le signal.
//!
//! Les quatre couches sont chargées **par le jeu lui-même**, au démarrage (TASK-100) : aucun test
//! n'appelle `load_layer`. Les fichiers de `tests/assets/audio` portent les noms de production.
//!
//! Et leurs gains sont poussés **par le système de musique**, à chaque image (TASK-101) : un gain
//! posé par la façade serait écrasé à l'image suivante. Un test qui veut tenir ses gains passe
//! par la ressource publique : une vitesse de fondu nulle gèle l'interpolation, et les gains
//! courants portent ce qu'il veut entendre. Le test du mix, lui, laisse le fondu faire.

#[path = "support/offline.rs"]
mod offline;

use std::sync::{Mutex, Once};

use audio_system::{
    AdaptiveMusicManager, AudioBackendHandle, AudioBusVolumes, Bus, PitchScaleTracker,
    SoundEffectBank,
    bus::{local_gain, music_gain, sfx_volume},
    music::STEM_PATHS,
};
use bevy::prelude::*;
use bevy_seedling::prelude::SamplePlayer;
use core_engine::{
    blinds::{BlindContext, BlindDefinition, BlindType},
    dice::DieId,
    hands::HandGrid,
    scoring::{ScoreAction, StepSource},
};
use game_state::states::{AppState, RunPhase};
use offline::{RATE, offline_app, offline_app_at, render, step, wait_loaded};
use ui_and_juice::events::ScoreStepPlayed;

const LAYER_FRAMES: usize = 48_000;
const IMPULSE_SPACING: usize = 480;
const HIT_FRAMES: usize = 5_760;
/// Les trois curseurs à l'unité.
const UNITY: AudioBusVolumes = AudioBusVolumes {
    master: 1.0,
    music: 1.0,
    sfx: 1.0,
};

fn with_backend<R>(
    app: &mut App,
    act: impl FnOnce(&mut AudioBackendHandle, &AssetServer) -> R,
) -> R {
    let server = app.world().resource::<AssetServer>().clone();
    let mut handle = app.world_mut().resource_mut::<AudioBackendHandle>();
    act(&mut handle, &server)
}

fn manager(app: &mut App) -> Mut<'_, AdaptiveMusicManager> {
    app.world_mut().resource_mut::<AdaptiveMusicManager>()
}

/// Une image, celle du démarrage : le jeu y charge ses quatre couches. Le test **gèle le fondu**
/// et pose les gains courants, que le système de musique pousse dès l'image suivante, donc
/// **avant le départ**, qui attend la fin des chargements : le premier échantillon sorti est
/// celui de la trame 0.
fn layers_with_gains(app: &mut App, gains: [f32; 4], left: &mut Vec<f32>) {
    layers_with(app, gains, 1.0, left);
}

/// La même, avec un ducking : posé lui aussi avant le départ, sans quoi la première impulsion
/// sortirait sans lui, le temps que le gain se lisse.
fn layers_with(app: &mut App, gains: [f32; 4], duck: f32, left: &mut Vec<f32>) {
    step(app, left);
    let mut manager = manager(app);
    manager.fade_per_second = 0.0;
    manager.current_gains = gains;
    manager.duck = duck;
    wait_loaded(app, &STEM_PATHS, left);
}

/// Le même gel, sans attendre des chargements : pour une racine où les stems manquent.
fn manager_pinned(app: &mut App, gains: [f32; 4], left: &mut Vec<f32>) {
    step(app, left);
    let mut manager = manager(app);
    manager.fade_per_second = 0.0;
    manager.current_gains = gains;
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
    // Fin de run : les quatre cibles y sont nulles, la musique ne déborde pas sur la mesure.
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::GameOver);
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
    layers_with_gains(&mut app, [1.0; 4], &mut left);
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
    manager(&mut app).current_gains[3] = 0.0;
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
    let first_impulse = |volumes: AudioBusVolumes, weight: f32, duck: f32| {
        let mut app = offline_app();
        let mut left = Vec::new();
        app.world_mut().insert_resource(volumes);
        layers_with(&mut app, [weight, 0.0, 0.0, 0.0], duck, &mut left);
        render(&mut app, 0.5, &mut left);
        left[left
            .iter()
            .position(|v| *v != 0.0)
            .expect("la couche 0 ne sort pas")]
    };
    let layer = per_mille(
        first_impulse(volumes, 0.5, 0.5),
        first_impulse(UNITY, 1.0, 1.0),
    );
    assert!(
        (61..=64).contains(&layer),
        "couche entendue à {layer} pour mille, 62,5 attendus"
    );
}

/// TASK-100 : **`layers[i]` fait sonner le fichier d'index `i`**, et lui seul. Une permutation
/// ne casse aucune compilation et la somme des quatre couches y est aveugle : il faut l'oreille.
/// Chaque couche est donc écoutée seule ; son impulsion est à `i` × 480 trames de son départ.
#[test]
fn test_each_layer_handle_drives_its_own_stem() {
    for index in 0..4 {
        let mut gains = [0.0; 4];
        gains[index] = 1.0;
        let mut app = offline_app();
        let mut left = Vec::new();
        layers_with_gains(&mut app, gains, &mut left);
        render(&mut app, 0.5, &mut left);

        let start = onset(&left);
        let impulse = left
            .iter()
            .position(|v| *v > 0.3)
            .expect("la couche ne porte pas son impulsion");
        let heard = (impulse - start + IMPULSE_SPACING / 2) / IMPULSE_SPACING;
        assert_eq!(
            heard, index,
            "`layers[{index}]` fait sonner le stem d'index {heard}"
        );
    }
}

fn blind(kind: BlindType, target_score: u64, current_score: u64) -> BlindContext {
    BlindContext {
        blind: BlindDefinition {
            kind,
            target_score,
            ..Default::default()
        },
        target_score,
        current_score,
        hands_remaining: 3,
        used_hands: HandGrid::default(),
    }
}

/// TASK-101 : **l'état du jeu s'entend**, par le fondu réel, sans rien geler. En run, Base et
/// Mélodie sont pleines et les deux autres couches muettes ; un Boss lève la Tension ; la boutique
/// adoucit le mix de moitié et fait taire la Tension, **alors que le contexte du Boss battu est
/// encore dans le monde** : c'est le piège de TASK-99, écouté sur le signal.
#[test]
fn test_state_changes_are_heard() {
    let mut app = offline_app();
    let mut left = Vec::new();
    // Ce test écoute la musique : l'entrée dans la phase de lancer fait sonner un lancer de dés
    // (TASK-103), qui est ici un bip, ses fichiers manquant à cette racine. Le curseur SFX le tait.
    app.world_mut()
        .insert_resource(AudioBusVolumes { sfx: 0.0, ..UNITY });
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::InRun);
    // Le mix de run est posé avant le départ, pour que le premier échantillon soit celui de la
    // trame 0 ; la vitesse de fondu reste celle du jeu.
    step(&mut app, &mut left);
    manager(&mut app).current_gains = [1.0, 1.0, 0.0, 0.0];
    wait_loaded(&mut app, &STEM_PATHS, &mut left);
    render(&mut app, 1.2, &mut left);

    let origin = onset(&left);
    let impulse = |left: &[f32], turn: usize, layer: usize| {
        left[origin + turn * LAYER_FRAMES + layer * IMPULSE_SPACING]
    };
    let last_turn = |left: &[f32]| (left.len() - origin - 4 * IMPULSE_SPACING) / LAYER_FRAMES;

    let run = impulse(&left, 0, 0);
    // Le codec rabote l'impulsion, chaque fichier à sa façon : une couche pleine se lit au-dessus
    // de 0,3, une couche muette sous 0,1 (il y reste les notes des autres), et un rapport ne se
    // prend qu'entre deux lectures de la même couche.
    assert!(run > 0.3, "la Base ne sonne pas en run : {run}");
    assert!(impulse(&left, 0, 1) > 0.3, "la Mélodie ne sonne pas en run");
    assert!(impulse(&left, 0, 2) < 0.1, "la Tension sonne sans raison");
    assert!(impulse(&left, 0, 3) < 0.1, "le Climax sonne sans raison");

    // Un Boss annoncé : la Tension monte, par le fondu.
    app.world_mut()
        .insert_resource(blind(BlindType::Boss, 1_000, 0));
    render(&mut app, 3.5, &mut left);
    let turn = last_turn(&left);
    let tension = impulse(&left, turn, 2);
    assert!(tension > 0.3, "le Boss ne lève pas la Tension : {tension}");
    assert!(
        impulse(&left, turn, 3) < 0.1,
        "le Climax sonne sous le seuil"
    );

    // Le Boss est battu, la manche se clôt, la boutique s'ouvre. Son contexte reste dans le
    // monde : sans le filtre, Tension et Climax sonneraient à chaque visite.
    app.world_mut()
        .insert_resource(blind(BlindType::Boss, 1_000, 1_000));
    go_phase(&mut app, RunPhase::Roll, false, &mut left);
    go_phase(&mut app, RunPhase::Scoring, false, &mut left);
    go_phase(&mut app, RunPhase::RoundEnd, false, &mut left);
    go_phase(&mut app, RunPhase::Shop, false, &mut left);
    assert!(app.world().contains_resource::<BlindContext>());
    render(&mut app, 4.0, &mut left);
    let turn = last_turn(&left);
    let base = per_mille(impulse(&left, turn, 0), run);
    assert!(
        (490..=510).contains(&base),
        "Base en boutique à {base} pour mille, 500 attendus"
    );
    assert!(
        impulse(&left, turn, 2) < 0.1,
        "la Tension sonne en boutique"
    );
    assert!(impulse(&left, turn, 3) < 0.1, "le Climax sonne en boutique");
}

/// Les identifiants des lecteurs du monde, triés : les quatre voies, et elles seules.
fn players(app: &mut App) -> Vec<Entity> {
    let mut found: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<SamplePlayer>>()
        .iter(app.world())
        .collect();
    found.sort();
    found
}

fn go_app(app: &mut App, state: AppState, left: &mut Vec<f32>) {
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(state);
    render(app, 0.2, left);
    assert_eq!(*app.world().resource::<State<AppState>>().get(), state);
}

/// `reflexive` pose un `set` nu, qui autorise la transition d'un état vers lui-même ; sinon
/// c'est la forme du jeu, `set_if_neq`, en appel qualifié.
fn go_phase(app: &mut App, phase: RunPhase, reflexive: bool, left: &mut Vec<f32>) {
    {
        let mut next = app.world_mut().resource_mut::<NextState<RunPhase>>();
        if reflexive {
            next.set(phase);
        } else {
            NextState::set_if_neq(&mut next, phase);
        }
    }
    render(app, 0.2, left);
    assert_eq!(*app.world().resource::<State<RunPhase>>().get(), phase);
}

/// TASK-100 : **la musique survit à tous les changements d'état**, y compris la sortie de la
/// run et une transition d'un état vers lui-même. Le backend nul ne crée aucune entité : seul le
/// backend réel a des voies qu'un marqueur de despawn lié à un état pourrait détruire, et une
/// voie détruite ne renaît pas, les couches ne partent qu'une fois. Le test compare les
/// **identifiants** des quatre lecteurs, pas leur nombre, puis écoute : chaque impulsion de
/// chaque couche est à sa trame, du premier bouclage au dernier.
#[test]
fn test_music_survives_the_state_cycle() {
    let mut app = offline_app();
    let mut left = Vec::new();
    // Le lancer de dés sonne à chaque entrée dans sa phase (TASK-103) : le curseur SFX le tait,
    // ce test écoute les impulsions de la musique.
    app.world_mut()
        .insert_resource(AudioBusVolumes { sfx: 0.0, ..UNITY });
    layers_with_gains(&mut app, [1.0; 4], &mut left);
    render(&mut app, 0.5, &mut left);
    let voices = players(&mut app);
    assert_eq!(voices.len(), 4, "les quatre voies ne sont pas parties");

    // Un témoin rattaché à la phase de lancer : s'il tombe, les transitions ont bien eu lieu et
    // le piège est réel.
    go_app(&mut app, AppState::CupSelect, &mut left);
    go_app(&mut app, AppState::InRun, &mut left);
    go_phase(&mut app, RunPhase::Roll, false, &mut left);
    let witness = app.world_mut().spawn(DespawnOnExit(RunPhase::Roll)).id();
    go_phase(&mut app, RunPhase::Roll, true, &mut left);
    assert!(
        app.world().get_entity(witness).is_err(),
        "la transition réflexive n'a pas eu lieu : le test ne prouve rien"
    );
    assert_eq!(players(&mut app), voices, "transition réflexive");

    // Les transitions déclarées de la run : une main perdue et la suivante, une blind battue et
    // sa boutique, puis la défaite, qui sort de la run, et le retour au menu.
    go_phase(&mut app, RunPhase::Scoring, false, &mut left);
    go_phase(&mut app, RunPhase::RoundEnd, false, &mut left);
    go_phase(&mut app, RunPhase::Roll, false, &mut left);
    assert_eq!(players(&mut app), voices, "retour au lancer");
    go_phase(&mut app, RunPhase::Scoring, false, &mut left);
    go_phase(&mut app, RunPhase::RoundEnd, false, &mut left);
    go_phase(&mut app, RunPhase::Shop, false, &mut left);
    go_phase(&mut app, RunPhase::BlindSelect, false, &mut left);
    go_phase(&mut app, RunPhase::Roll, false, &mut left);
    go_phase(&mut app, RunPhase::Scoring, false, &mut left);
    go_phase(&mut app, RunPhase::RoundEnd, false, &mut left);
    go_app(&mut app, AppState::GameOver, &mut left);
    go_app(&mut app, AppState::MainMenu, &mut left);
    render(&mut app, 1.0, &mut left);
    assert_eq!(players(&mut app), voices, "sortie de la run");

    // Le son : quatre impulsions par bouclage, chacune à sa trame, sans un trou.
    let origin = left
        .iter()
        .position(|v| *v != 0.0)
        .expect("aucune couche ne sort");
    let turns = (left.len() - origin - 4 * IMPULSE_SPACING) / LAYER_FRAMES;
    assert!(turns >= 4, "rendu trop court : {turns} bouclages");
    for turn in 0..turns {
        for layer in 0..4 {
            let at = origin + turn * LAYER_FRAMES + layer * IMPULSE_SPACING;
            assert!(
                left[at] > 0.3,
                "couche {layer}, bouclage {turn} : la voie s'est tue ({})",
                left[at]
            );
        }
    }
}

// ------------------------------------------------------------------ TASK-102

/// Les avertissements de tout le binaire de test, captés par un journaliseur posé une fois. Les
/// tests tournent en parallèle dans un même processus : chacun ne compte que les messages qui
/// portent **un chemin qu'il est seul à faire échouer**.
static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            WARNINGS
                .lock()
                .expect("verrou")
                .push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

fn capture_warnings() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_logger(&Capture).expect("un seul journaliseur par processus");
        log::set_max_level(log::LevelFilter::Warn);
    });
}

fn warnings_about(path: &str) -> Vec<String> {
    let all = WARNINGS.lock().expect("verrou");
    all.iter().filter(|m| m.contains(path)).cloned().collect()
}

/// Joue `clip` et rend une seconde de son, à partir de l'appel.
fn render_clip(app: &mut App, clip: audio_system::AudioClip, left: &mut Vec<f32>) -> Vec<f32> {
    with_backend(app, |handle, _| handle.0.play(clip, Bus::Sfx, 1.0, 1.0));
    let before = left.len();
    render(app, 1.0, left);
    left.split_off(before)
}

/// Durée sonore d'un rendu, en trames : du premier au dernier échantillon non nul.
fn sounding_frames(signal: &[f32]) -> usize {
    let last = signal.iter().rposition(|v| *v != 0.0).expect("silence");
    last + 1 - onset(signal)
}

/// TASK-102 : **un fichier manquant s'entend, et se dit une fois.** La racine d'assets des tests
/// ne porte aucun des dix-sept fichiers de la banque : chacun y est remplacé par le bip. Le
/// backend nul, qui ne lit aucun fichier, est aveugle à tout cela.
#[test]
fn test_a_missing_clip_is_heard_as_a_beep() {
    capture_warnings();
    let mut app = offline_app();
    let mut left = Vec::new();
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::GameOver);
    let absent = "audio/absent_du_test_du_bip.ogg";
    let (missing, hit) = with_backend(&mut app, |handle, server| {
        let missing = handle.0.load_clip(server, absent);
        // Demandé **avant** que le clip soit résolu : le son attend dans la file, il n'est ni
        // perdu, ni confié à un lecteur qui attendrait pour toujours.
        handle.0.play(missing, Bus::Sfx, 1.0, 1.0);
        (missing, handle.0.load_clip(server, "hit.ogg"))
    });
    wait_loaded(&mut app, &["hit.ogg"], &mut left);
    render(&mut app, 1.0, &mut left);

    // Le bip : 60 ms à 48 kHz, soit 2 880 trames, à 0,25 d'amplitude.
    let early = left.clone();
    let frames = sounding_frames(&early);
    assert!(
        (2_870..=2_880).contains(&frames),
        "le son demandé trop tôt dure {frames} trames"
    );
    let level = per_mille(peak(&early), 1.0);
    assert!((245..=255).contains(&level), "bip à {level} pour mille");
    // Il s'ouvre et se ferme sans claquer : ses deux bouts sont à fleur de zéro.
    let first = early[onset(&early)];
    let last = early[early.iter().rposition(|v| *v != 0.0).expect("silence")];
    assert!((-0.005..=0.005).contains(&first), "le bip claque : {first}");
    assert!((-0.005..=0.005).contains(&last), "le bip claque : {last}");

    // Un clip de la banque, absent lui aussi, sonne pareil ; un fichier présent sonne comme lui.
    let third_roll = app.world().resource::<SoundEffectBank>().dice_rolls[2];
    let roll = render_clip(&mut app, third_roll, &mut left);
    assert!((2_870..=2_880).contains(&sounding_frames(&roll)));
    let again = render_clip(&mut app, missing, &mut left);
    assert!((2_870..=2_880).contains(&sounding_frames(&again)));
    let real = render_clip(&mut app, hit, &mut left);
    assert!(peak(&real) > 0.5, "le fichier présent a été remplacé");
    assert!(sounding_frames(&real) > 5_000);

    // Dit une fois, avec le chemin, et pas une fois par image ni par `play`.
    let said = warnings_about(absent);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("bip"), "{said:?}");

    // Aucun lecteur n'attend pour toujours un asset qui ne viendra pas : il ne reste que les
    // quatre voies de la musique.
    assert_eq!(players(&mut app).len(), 4);
}

/// TASK-102 : un **stem** manquant se dit une fois, et n'est pas remplacé : les quatre couches
/// partent ensemble ou pas du tout. `tests/support` n'a pas de dossier `audio`.
#[test]
fn test_a_missing_stem_is_said_once() {
    capture_warnings();
    let mut app = offline_app_at("tests/support");
    let mut left = Vec::new();
    manager_pinned(&mut app, [1.0; 4], &mut left);
    render(&mut app, 1.5, &mut left);

    for path in STEM_PATHS {
        let said = warnings_about(path);
        assert_eq!(said.len(), 1, "{path} : {said:?}");
        assert!(said[0].contains("stem"), "{said:?}");
    }
    assert_eq!(
        peak(&left),
        0.0,
        "la bande-son est partie sans ses quatre couches"
    );
    assert!(players(&mut app).is_empty());
}

// ------------------------------------------------------------------ TASK-104

/// TASK-104 : **la hauteur au plafond se joue vraiment.** Vingt-quatre demi-tons, c'est quatre
/// fois la vitesse de lecture, et TASK-97 n'avait écouté que l'octave : un backend qui bornerait
/// la vitesse rendrait le haut de la gamme faux, sans une erreur. Le coup dure quatre fois moins.
#[test]
fn test_the_capped_pitch_is_played() {
    let mut tracker = PitchScaleTracker::default();
    for _ in 0..40 {
        tracker.advance();
    }
    let length = |signal: &[f32]| {
        signal
            .iter()
            .rposition(|v| v.abs() > 1e-4)
            .expect("silence")
            + 1
    };
    let reference = render_hit(Bus::Sfx, 1.0, 1.0, UNITY);
    let top = render_hit(Bus::Sfx, 1.0, tracker.pitch(), UNITY);
    let quartered = per_mille(length(&top) as f32, length(&reference) as f32);
    assert!(
        (225..=275).contains(&quartered),
        "au plafond, le coup dure {quartered} pour mille de sa durée nominale"
    );
}

// ------------------------------------------------------------------ TASK-105

/// Publie un palier de score, comme le fait la mise en scène, et rend une seconde de son à
/// partir de là. La musique se tait : fin de run. Les clips de la banque manquent à cette
/// racine : chaque palier y sonne comme le bip, ce qui suffit à écouter un routage.
fn render_step(action: ScoreAction) -> Vec<f32> {
    let mut app = offline_app();
    let mut left = Vec::new();
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::GameOver);
    render(&mut app, 0.5, &mut left);
    assert_eq!(peak(&left), 0.0, "du son sort avant le premier palier");

    app.world_mut().write_message(ScoreStepPlayed {
        source: StepSource::Die {
            die_id: DieId(0),
            value: 6,
        },
        action,
    });
    let before = left.len();
    render(&mut app, 1.0, &mut left);
    left.split_off(before)
}

/// TASK-105 : **un palier multiplicatif résonne, les autres non.** Le lecteur des paliers est
/// écouté de bout en bout : le palier publié, le routage choisi, la queue de réverbération qui
/// suit le coup. Sur le backend nul, ce n'est qu'un nom de bus dans un journal.
#[test]
fn test_a_multiplying_step_resonates() {
    let dry = render_step(ScoreAction::AddChips(100));
    let wet = render_step(ScoreAction::MultiplyMult(300));
    let (dry_start, wet_start) = (onset(&dry), onset(&wet));
    // Les deux paliers sonnent, au même instant et au même niveau : deux actions au plein volume.
    assert_eq!(wet_start, dry_start);
    let level = per_mille(
        peak(&wet[wet_start..][..2_880]),
        peak(&dry[dry_start..][..2_880]),
    );
    assert!(
        (950..=1_050).contains(&level),
        "coup à {level} pour mille du tic"
    );

    // Après le coup, 60 ms, le routage sec se tait ; le routage réverbéré laisse une queue.
    let tail = |signal: &[f32], start: usize| {
        let from = start + 2_880 + RATE / 50;
        dbfs_rms(&signal[from..from + RATE * 3 / 10])
    };
    let (dry_tail, wet_tail) = (tail(&dry, dry_start), tail(&wet, wet_start));
    assert!(
        dry_tail < -80.0,
        "le palier additif laisse une queue : {dry_tail:.1} dBFS"
    );
    assert!(
        wet_tail > -60.0,
        "le palier multiplicatif ne résonne pas : {wet_tail:.1} dBFS"
    );
}
