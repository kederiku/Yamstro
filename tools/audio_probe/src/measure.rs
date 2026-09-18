//! Les rendus hors ligne et l'analyse du signal : ce fichier produit les chiffres de l'addendum.
//!
//! Une estimation n'est pas une mesure : chaque valeur sort du signal rendu, comparé au signal
//! attendu, lui-même relu dans les WAV sans passer par le décodeur mesuré.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use bevy::{log::LogPlugin, prelude::*, time::TimeUpdateStrategy};
use bevy_seedling::{SeedlingCorePlugin, prelude::*};

use crate::{
    graph::{GraphPlan, build_graph},
    offline::{BLOCKS_PER_UPDATE, OfflineDriver, OfflinePlatformPlugin, RATE, UPDATE_STEP},
    score::{
        Decoded, FADE_DELAY_SECONDS, FADE_SECONDS, Fade, Launch, Score, Start,
        drive_per_frame_fade, load_assets, start_when_loaded,
    },
    sounds::STEMS,
    wav::read_mono_pcm16,
};

const LOOP_FRAMES: usize = 192_000;
const IMPULSE_SPACING: usize = 480;
const HIT_FRAMES: usize = 5_760;
const LOOP_MINUTES: f64 = 10.0;
/// Seuil du critère 1 : 1 ms à 48 kHz.
const DRIFT_LIMIT_FRAMES: i64 = 48;
/// Un front montant de plus de 0,1 entre deux échantillons ne peut être qu'une impulsion :
/// la pente maximale des quatre notes réunies reste sous 0,02.
const IMPULSE_EDGE: f32 = 0.1;

struct Render {
    left: Vec<f32>,
    start: usize,
    stereo_mismatches: u64,
    decoded: Vec<Decoded>,
}

impl Render {
    /// Le signal à partir de l'échantillon de départ visé.
    fn after_start(&self) -> &[f32] {
        &self.left[self.start..]
    }
}

static LOG_INSTALLED: AtomicBool = AtomicBool::new(false);

fn render(graph: GraphPlan, score: Score, seconds_after_start: f64) -> Render {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: "assets".to_string(),
            ..Default::default()
        },
        SeedlingCorePlugin,
        OfflinePlatformPlugin,
    ));
    if !LOG_INSTALLED.swap(true, Ordering::SeqCst) {
        app.add_plugins(LogPlugin::default());
    }
    app.insert_resource(TimeUpdateStrategy::ManualDuration(UPDATE_STEP))
        .insert_resource(AudioGraphTemplate::Empty)
        .insert_resource(graph)
        .insert_resource(score)
        .init_resource::<Launch>()
        .add_systems(Startup, (build_graph, load_assets))
        .add_systems(Update, (start_when_loaded, drive_per_frame_fade).chain());
    app.finish();
    app.cleanup();

    let needed = (seconds_after_start * f64::from(RATE)).ceil() as usize;
    let mut left = Vec::new();
    loop {
        app.update();
        app.world_mut()
            .resource_mut::<OfflineDriver>()
            .pump(BLOCKS_PER_UPDATE, &mut left);
        match app.world().resource::<Launch>().start_sample {
            Some(start) if left.len() >= start as usize + needed => break,
            None if left.len() > 120 * RATE as usize => panic!("les assets ne se chargent pas"),
            _ => {}
        }
    }
    let launch = app.world().resource::<Launch>();
    Render {
        start: launch.start_sample.expect("départ") as usize,
        decoded: launch.decoded.clone(),
        stereo_mismatches: app.world().resource::<OfflineDriver>().stereo_mismatches,
        left,
    }
}

fn dbfs(value: f64) -> f64 {
    if value <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * value.log10()
    }
}

fn peak(signal: &[f32]) -> f64 {
    signal
        .iter()
        .fold(0.0_f64, |m, &v| m.max(f64::from(v.abs())))
}

fn rms(signal: &[f32]) -> f64 {
    (signal
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum::<f64>()
        / signal.len() as f64)
        .sqrt()
}

fn max_step(signal: &[f32]) -> f64 {
    signal
        .windows(2)
        .fold(0.0_f64, |m, w| m.max(f64::from((w[1] - w[0]).abs())))
}

/// Gain de la chaîne, par moindres carrés entre le rendu et l'attendu.
fn least_squares_gain(rendered: &[f32], expected: &[f32]) -> f64 {
    let (mut xe, mut ee) = (0.0_f64, 0.0_f64);
    for (&x, &e) in rendered.iter().zip(expected) {
        xe += f64::from(x) * f64::from(e);
        ee += f64::from(e) * f64::from(e);
    }
    xe / ee
}

fn report(key: &str, value: impl std::fmt::Display) {
    println!("{key} = {value}");
}

fn load_stems() -> Vec<Vec<f32>> {
    STEMS
        .iter()
        .map(|name| {
            let stem = read_mono_pcm16(&Path::new("assets").join(name), RATE);
            assert_eq!(stem.len(), LOOP_FRAMES, "{name} : durée inattendue");
            stem
        })
        .collect()
}

/// Ce que l'analyse lit dans un rendu de stems en boucle.
struct LoopFindings {
    first_impulse: usize,
    gain: f64,
    complete_loops: usize,
    impulses: usize,
    /// Écart maximal entre stems au sein d'un même bouclage, en trames.
    drift: i64,
    /// Écart maximal d'une impulsion à sa place sur la grille du premier départ, en trames.
    wander: i64,
    worst_residual: f64,
    worst_residual_at: usize,
    seam_residual: f64,
}

/// Dérive et couture, lues sur le signal. La dérive vient des impulsions (une par stem et par
/// bouclage, à la trame k * 480) ; la couture vient du résidu entre le rendu et les stems tuilés.
fn analyse_loop(signal: &[f32], stems: &[Vec<f32>]) -> LoopFindings {
    let sum: Vec<f32> = (0..LOOP_FRAMES)
        .map(|m| stems.iter().map(|s| s[m]).sum())
        .collect();
    let edges: Vec<usize> = (1..signal.len())
        .filter(|&n| signal[n] - signal[n - 1] > IMPULSE_EDGE)
        .collect();
    let origin = *edges.first().expect("aucune impulsion dans le rendu");
    let gain = least_squares_gain(
        &signal[origin + LOOP_FRAMES..origin + 2 * LOOP_FRAMES],
        &sum,
    );

    let loops = (signal.len() - origin) / LOOP_FRAMES + 1;
    let mut deviations = vec![[None::<i64>; 4]; loops + 1];
    for &n in &edges {
        let rel = (n - origin) as i64;
        let (mut turn, mut at) = (rel / LOOP_FRAMES as i64, rel % LOOP_FRAMES as i64);
        if at > (LOOP_FRAMES / 2) as i64 {
            turn += 1;
            at -= LOOP_FRAMES as i64;
        }
        let stem = ((at as f64 / IMPULSE_SPACING as f64).round() as i64).clamp(0, 3);
        if let Some(row) = deviations.get_mut(turn as usize) {
            row[stem as usize] = Some(at - stem * IMPULSE_SPACING as i64);
        }
    }
    let complete: Vec<[i64; 4]> = deviations
        .iter()
        .filter_map(|row| Some([row[0]?, row[1]?, row[2]?, row[3]?]))
        .collect();
    let spread = |d: &[i64; 4]| d.iter().max().expect("quatre") - d.iter().min().expect("quatre");
    let drift = complete.iter().map(spread).max().unwrap_or(i64::MAX);
    let wander = complete
        .iter()
        .flatten()
        .map(|d| d.abs())
        .max()
        .unwrap_or(i64::MAX);

    let (mut worst_residual, mut worst_residual_at, mut seam_residual) = (0.0_f64, 0, 0.0_f64);
    for (i, &x) in signal[origin..].iter().enumerate() {
        let at = i % LOOP_FRAMES;
        let residual = (f64::from(x) - gain * f64::from(sum[at])).abs();
        if residual > worst_residual {
            (worst_residual, worst_residual_at) = (residual, i);
        }
        if i >= LOOP_FRAMES / 2 && !(256..LOOP_FRAMES - 256).contains(&at) {
            seam_residual = seam_residual.max(residual);
        }
    }
    LoopFindings {
        first_impulse: origin,
        gain,
        complete_loops: complete.len(),
        impulses: edges.len(),
        drift,
        wander,
        worst_residual,
        worst_residual_at,
        seam_residual,
    }
}

/// Critère 1 : quatre stems en boucle pendant dix minutes, dérive et couture lues sur le signal.
fn measure_loop(stems: &[Vec<f32>]) {
    let seconds = LOOP_MINUTES * 60.0 + 0.1;
    let score = Score {
        stems: vec![0, 1, 2, 3],
        ..Default::default()
    };
    let render = render(GraphPlan::UNITY, score, seconds);
    let found = analyse_loop(&render.left, stems);

    report("boucle.depart_vise_echantillon", render.start);
    report("boucle.premiere_impulsion_echantillon", found.first_impulse);
    report(
        "boucle.ecart_depart_trames",
        found.first_impulse as i64 - render.start as i64,
    );
    report("boucle.gain_de_chaine", format!("{:.9}", found.gain));
    report("boucle.minutes_rendues", LOOP_MINUTES);
    report("boucle.bouclages_complets_observes", found.complete_loops);
    report("boucle.impulsions_detectees", found.impulses);
    report("boucle.derive_max_entre_stems_trames", found.drift);
    report(
        "boucle.derive_max_entre_stems_ms",
        format!("{:.4}", found.drift as f64 * 1000.0 / f64::from(RATE)),
    );
    report("boucle.ecart_max_a_la_grille_trames", found.wander);
    report("boucle.seuil_trames", DRIFT_LIMIT_FRAMES);
    report(
        "boucle.residu_max_dbfs",
        format!("{:.1}", dbfs(found.worst_residual)),
    );
    report("boucle.residu_max_position_trames", found.worst_residual_at);
    report(
        "boucle.residu_max_aux_coutures_dbfs",
        format!("{:.1}", dbfs(found.seam_residual)),
    );
    report("boucle.trames_stereo_differentes", render.stereo_mismatches);
    let held = found.drift <= DRIFT_LIMIT_FRAMES && dbfs(found.seam_residual) < -60.0;
    report("boucle.critere_1", if held { "PASSE" } else { "ECHEC" });
}

/// L'analyse a des dents : un stem en retard de 50 trames et un clic injecté doivent se voir.
/// Un zéro mesuré ne vaut que si l'instrument sait lire autre chose que zéro.
fn self_check(stems: &[Vec<f32>]) {
    const LATE: usize = 50;
    let mut signal = vec![0.0_f32; 768 + 6 * LOOP_FRAMES];
    for (k, stem) in stems.iter().enumerate() {
        for i in 0..6 * LOOP_FRAMES {
            let late = if k == 2 && i >= 3 * LOOP_FRAMES {
                LATE
            } else {
                0
            };
            if i >= late {
                signal[768 + i] += stem[(i - late) % LOOP_FRAMES];
            }
        }
    }
    let found = analyse_loop(&signal, stems);
    report("autotest.derive_injectee_trames", LATE);
    report("autotest.derive_mesuree_trames", found.drift);
    report(
        "autotest.residu_avec_derive_dbfs",
        format!("{:.1}", dbfs(found.worst_residual)),
    );
    assert_eq!(
        found.drift, LATE as i64,
        "l'analyse ne voit pas une dérive de {LATE} trames"
    );
    assert!(
        dbfs(found.worst_residual) > -20.0,
        "le résidu ne voit pas le stem en retard"
    );

    // Un fondu linéaire propre, puis le même avec une marche de gain de 0,5 à mi-course.
    let tone = &stems[3];
    let faded = |click: bool| -> Vec<f32> {
        (0..2 * RATE as usize)
            .map(|i| {
                let ramp = (i as f32 / RATE as f32).min(1.0);
                let gain = if click && i >= RATE as usize / 2 {
                    (ramp + 0.5).min(1.0)
                } else {
                    ramp
                };
                gain * tone[2_000 + i]
            })
            .collect()
    };
    let (clean, clicked) = (faded(false), faded(true));
    let steady = RATE as usize * 3 / 2..2 * RATE as usize;
    let ratio = |s: &[f32]| max_step(&s[..RATE as usize]) / max_step(&s[steady.clone()]);
    report(
        "autotest.rapport_des_pas_fondu_propre",
        format!("{:.4}", ratio(&clean)),
    );
    report(
        "autotest.rapport_des_pas_clic_injecte",
        format!("{:.4}", ratio(&clicked)),
    );
    assert!(
        ratio(&clean) <= 1.01,
        "un fondu propre est pris pour un clic"
    );
    assert!(
        ratio(&clicked) > 1.01,
        "un clic de 0,5 de gain passe inaperçu"
    );
}

/// Critère 2, première moitié : une couche monte du silence à l'unité en 1,5 s, sans clic.
/// Deux façons de monter sont mesurées ; la seconde est celle que le jeu emploiera.
fn measure_fades(stems: &[Vec<f32>]) -> bool {
    let scheduled = measure_fade(stems, "fondu_programme", Fade::Scheduled);
    let per_frame = measure_fade(stems, "fondu_par_image", Fade::PerFrame);
    scheduled && per_frame
}

fn measure_fade(stems: &[Vec<f32>], name: &str, fade: Fade) -> bool {
    let layer = 3;
    let score = Score {
        stems: vec![layer],
        fade_layer: Some((layer, fade)),
        ..Default::default()
    };
    let render = render(GraphPlan::UNITY, score, 3.6);
    let report = |key: &str, value: String| report(&format!("{name}.{key}"), value);
    let signal = render.after_start();
    let rate = RATE as usize;

    let fade_start = (FADE_DELAY_SECONDS * f64::from(RATE)) as usize;
    let fade_end = fade_start + (FADE_SECONDS * f64::from(RATE)) as usize;
    let steady = fade_end + rate / 2..fade_end + rate * 14 / 10;
    let window = fade_start..fade_end + rate * 3 / 10;

    let unity: Vec<f32> = (0..signal.len())
        .map(|i| stems[layer][i % LOOP_FRAMES])
        .collect();
    let chain = least_squares_gain(&signal[steady.clone()], &unity[steady.clone()]);

    let step_fade = max_step(&signal[window.clone()]);
    let step_steady = max_step(&signal[steady.clone()]);
    report("pas_max_pendant_le_fondu", format!("{step_fade:.7}"));
    report("pas_max_a_l_unite", format!("{step_steady:.7}"));
    report("rapport_des_pas", format!("{:.4}", step_fade / step_steady));

    // Gain instantané, là où la note est assez loin de zéro pour que le quotient ait un sens.
    let gains: Vec<(usize, f64)> = window
        .clone()
        .filter(|&i| unity[i].abs() > 0.04)
        .map(|i| (i, f64::from(signal[i]) / (chain * f64::from(unity[i]))))
        .collect();
    let (gain_step, gain_step_at) = gains
        .windows(2)
        .filter(|w| w[1].0 == w[0].0 + 1)
        .map(|w| ((w[1].1 - w[0].1).abs(), w[1].0))
        .fold((0.0_f64, 0), |m, c| if c.0 > m.0 { c } else { m });
    let curve: Vec<String> = [0.05, 0.1, 0.25, 0.5, 0.75, 1.0, 1.25, 1.45]
        .iter()
        .map(|t| {
            let at = fade_start + (t * f64::from(RATE)) as usize;
            let g = gains
                .iter()
                .find(|(i, _)| *i >= at)
                .map_or(f64::NAN, |(_, g)| *g);
            format!("{t} s: {g:.5}")
        })
        .collect();
    report("courbe_de_gain", curve.join(" ; "));
    report(
        "saut_de_gain_max_instant_s",
        format!(
            "{:.4}",
            (gain_step_at - fade_start) as f64 / f64::from(RATE)
        ),
    );
    report(
        "saut_de_gain_max_en_sortie",
        format!("{:.7}", gain_step * f64::from(unity[gain_step_at].abs())),
    );
    let reached = gains
        .iter()
        .find(|(_, g)| *g >= 0.99)
        .map(|(i, _)| (*i - fade_start) as f64 / f64::from(RATE));
    report("saut_de_gain_max_par_trame", format!("{gain_step:.7}"));
    report(
        "niveau_avant_le_fondu_dbfs",
        format!("{:.1}", dbfs(peak(&signal[..fade_start]))),
    );
    report(
        "gain_a_mi_course",
        format!(
            "{:.5}",
            gains
                .iter()
                .find(|(i, _)| *i >= fade_start + rate * 3 / 4)
                .map_or(f64::NAN, |(_, g)| *g)
        ),
    );
    report("gain_en_regime_etabli", format!("{chain:.6}"));
    report(
        "secondes_pour_atteindre_99_pourcent",
        reached.map_or("jamais".to_string(), |s| format!("{s:.4}")),
    );
    let jumps = gains
        .windows(2)
        .filter(|w| w[1].0 == w[0].0 + 1 && (w[1].1 - w[0].1).abs() > 0.002)
        .count();
    report("marches_de_gain_de_plus_de_0_002", jumps.to_string());
    report(
        "saut_max_en_sortie_relatif_a_la_note_db",
        format!(
            "{:.1}",
            dbfs(gain_step * f64::from(unity[gain_step_at].abs()))
                - dbfs(peak(&unity[steady.clone()]))
        ),
    );
    let clean = step_fade <= step_steady * 1.01;
    report(
        "sans_clic",
        (if clean { "PASSE" } else { "ECHEC" }).to_string(),
    );
    clean
}

/// Critère 2, seconde moitié : les trois bus s'atténuent indépendamment, par superposition.
/// Critère 3 : la réverbération du bus SFX laisse une queue après le coup sec.
fn measure_buses_and_reverb() {
    let seconds = 2.6;
    let window = 0..(2.5 * f64::from(RATE)) as usize;
    let music = Score {
        stems: vec![0, 1, 2, 3],
        ..Default::default()
    };
    let hit = Score {
        hit: true,
        ..Default::default()
    };
    let both = Score {
        stems: vec![0, 1, 2, 3],
        hit: true,
        ..Default::default()
    };

    let music_only = render(GraphPlan::UNITY, music, seconds);
    let hit_only = render(GraphPlan::UNITY, hit.clone(), seconds);
    let hit_dry = render(
        GraphPlan {
            reverb: false,
            ..GraphPlan::UNITY
        },
        hit,
        seconds,
    );
    let (m, s) = (
        &music_only.after_start()[window.clone()],
        &hit_only.after_start()[window.clone()],
    );

    let mut independent = true;
    for (name, plan, music_gain, sfx_gain) in [
        ("unite", GraphPlan::UNITY, 1.0_f64, 1.0_f64),
        (
            "music_moins_20_db",
            GraphPlan {
                music_db: -20.0,
                ..GraphPlan::UNITY
            },
            0.1,
            1.0,
        ),
        (
            "sfx_moins_20_db",
            GraphPlan {
                sfx_db: -20.0,
                ..GraphPlan::UNITY
            },
            1.0,
            0.1,
        ),
        (
            "master_moins_20_db",
            GraphPlan {
                master_db: -20.0,
                ..GraphPlan::UNITY
            },
            0.1,
            0.1,
        ),
    ] {
        let mixed = render(plan, both.clone(), seconds);
        let residual = mixed.after_start()[window.clone()]
            .iter()
            .zip(m.iter().zip(s.iter()))
            .fold(0.0_f64, |worst, (&x, (&a, &b))| {
                worst
                    .max((f64::from(x) - music_gain * f64::from(a) - sfx_gain * f64::from(b)).abs())
            });
        report(
            &format!("bus.{name}.residu_de_superposition_dbfs"),
            format!("{:.1}", dbfs(residual)),
        );
        independent &= dbfs(residual) < -60.0;
    }
    report(
        "bus.independance",
        if independent { "PASSE" } else { "ECHEC" },
    );

    let tail = HIT_FRAMES..HIT_FRAMES + (0.3 * f64::from(RATE)) as usize;
    let wet_tail = dbfs(rms(&hit_only.after_start()[tail.clone()]));
    let dry_tail = dbfs(rms(&hit_dry.after_start()[tail]));
    report(
        "reverb.coup_sec_crete_dbfs",
        format!("{:.1}", dbfs(peak(&hit_dry.after_start()[..HIT_FRAMES]))),
    );
    report(
        "reverb.coup_avec_effet_crete_dbfs",
        format!("{:.1}", dbfs(peak(&hit_only.after_start()[..HIT_FRAMES]))),
    );
    report(
        "reverb.queue_300_ms_avec_effet_dbfs",
        format!("{wet_tail:.1}"),
    );
    report(
        "reverb.queue_300_ms_sans_effet_dbfs",
        format!("{dry_tail:.1}"),
    );
    report(
        "reverb.critere_3",
        if wet_tail > -40.0 && dry_tail < -80.0 {
            "PASSE"
        } else {
            "ECHEC"
        },
    );
}

/// Le départ programmé dans le futur, relevé à part : exactitude, et fuite à la création.
fn measure_scheduled_start() {
    let score = Score {
        start: Start::Scheduled,
        stems: vec![0, 1, 2, 3],
        ..Default::default()
    };
    let render = render(GraphPlan::UNITY, score, 1.0);
    let edges: Vec<usize> = (1..render.left.len())
        .filter(|&n| render.left[n] - render.left[n - 1] > IMPULSE_EDGE)
        .collect();
    let leaked = render.left[..render.start]
        .iter()
        .filter(|v| **v != 0.0)
        .count();
    let on_target: Vec<i64> = edges
        .iter()
        .filter(|&&n| n >= render.start)
        .take(4)
        .enumerate()
        .map(|(k, &n)| n as i64 - (render.start + k * IMPULSE_SPACING) as i64)
        .collect();
    report("depart_programme.echantillon_vise", render.start);
    report(
        "depart_programme.ecart_des_quatre_stems_trames",
        format!("{on_target:?}"),
    );
    report(
        "depart_programme.trames_non_nulles_avant_l_instant_vise",
        leaked,
    );
}

/// Le bus Music pris seul : le rapport des gains de chaîne à -20 dB et à l'unité doit valoir 0,1.
fn measure_music_bus_alone(stems: &[Vec<f32>]) {
    let sum: Vec<f32> = (0..LOOP_FRAMES)
        .map(|m| stems.iter().map(|s| s[m]).sum())
        .collect();
    let music = Score {
        stems: vec![0, 1, 2, 3],
        ..Default::default()
    };
    let window = 24_000..96_000;
    let mut gains = Vec::new();
    for plan in [
        GraphPlan::UNITY,
        GraphPlan {
            music_db: -20.0,
            ..GraphPlan::UNITY
        },
    ] {
        let render = render(plan, music.clone(), 2.1);
        gains.push(least_squares_gain(
            &render.after_start()[window.clone()],
            &sum[window.clone()],
        ));
    }
    report("bus.music_seul.gain_a_l_unite", format!("{:.6}", gains[0]));
    report(
        "bus.music_seul.gain_a_moins_20_db",
        format!("{:.6}", gains[1]),
    );
    report(
        "bus.music_seul.rapport",
        format!("{:.6}", gains[1] / gains[0]),
    );
}

/// Le décodeur : ce que `bevy_seedling` relève d'un WAV et d'un OGG Vorbis, sans `bevy_audio`.
fn measure_decoder() {
    let score = Score {
        decode_only: vec!["stem_0.wav", "stem_0.ogg"],
        ..Default::default()
    };
    for decoded in render(GraphPlan::UNITY, score, 0.1).decoded {
        report(
            &format!("decodeur.{}", decoded.path),
            format!(
                "{} trames, {} voie(s), {:?} Hz, {} Hz à l'origine",
                decoded.frames, decoded.channels, decoded.sample_rate, decoded.original_sample_rate
            ),
        );
    }
}

pub fn run() {
    let stems = load_stems();
    report("flux.frequence_hz", RATE);
    self_check(&stems);
    measure_decoder();
    measure_scheduled_start();
    measure_fades(&stems);
    measure_music_bus_alone(&stems);
    measure_buses_and_reverb();
    if std::env::args().any(|a| a == "--short") {
        return;
    }
    measure_loop(&stems);
}
