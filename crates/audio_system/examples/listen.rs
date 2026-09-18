//! Banc d'écoute de l'Étape 8 (TASK-109) : le **vrai** plugin audio, sur les fichiers de
//! production, sans fenêtre, piloté au clavier.
//!
//! **De l'outillage, pas du code de jeu.** Trois critères de fin d'étape ne s'automatisent pas :
//! la boucle sans couture, le loquet reconnaissable sans regarder l'écran, les curseurs sans
//! craquement. Ils s'écoutent une fois, par une oreille humaine, et se consignent. Aucun build
//! jouable n'existe avant l'Étape 11 : ce banc est ce qui rend ces écoutes possibles **sur le
//! chemin de code du jeu**, `GameAudioPlugin::new()` et ses systèmes, et non sur un banc à part.
//! Il ne joue aucun son lui-même pour ce que le jeu sait déclencher : il pose l'état, et les
//! systèmes de la crate sonnent. Les effets isolés passent par la façade, comme le jeu.
//!
//! ```bash
//! cargo run -p audio_system --example listen
//! ```
//!
//! Une commande puis Entrée. `aide` les redonne.

use std::{
    collections::VecDeque,
    io::BufRead,
    sync::{Mutex, mpsc},
    time::Duration,
};

use audio_system::{
    AudioBackendHandle, AudioBusVolumes, Bus, GameAudioPlugin, SoundEffectBank, bus::local_gain,
};
use bevy::{
    app::ScheduleRunnerPlugin, ecs::system::SystemParam, prelude::*, state::app::StatesPlugin,
};
use core_engine::{
    blinds::{BlindContext, BlindDefinition, BlindType},
    dice::{DieId, DieSeal},
    hands::{HandGrid, YahtzeeHand},
    relics::RelicId,
    scoring::{ScoreAction, StepSource},
};
use game_state::states::{AppState, RunPhase};
use ui_and_juice::events::ScoreStepPlayed;

const HELP: &str = "\
états     : menu | run | boss | climax | boutique
déclenche : case (une figure consommée) | victoire (fanfare et ducking) | decompte (neuf paliers)
effets    : lancer tic relique base coup sceau verrou loquet survol clic piece fanfare
curseurs  : master | music | sfx  (balayage 1 → 0 → 1 en quatre secondes, puis retour à 1)
            m0 m5 m1 | u0 u5 u1 | s0 s5 s1  (saut à 0, 0,5 ou 1)
divers    : aide | q";

/// Les lignes lues sur l'entrée standard, par un fil à part : la boucle du jeu ne bloque pas.
#[derive(Resource)]
struct Lines(Mutex<mpsc::Receiver<String>>);

/// L'état que l'auditeur a demandé. Un système le rejoint, une transition à la fois : la
/// sous-phase de run n'existe qu'une image après l'entrée en run.
#[derive(Resource)]
struct Wanted {
    app: AppState,
    phase: Option<RunPhase>,
}

/// Les paliers du décompte à publier, un toutes les 250 ms, comme la mise en scène.
#[derive(Resource, Default)]
struct Countdown {
    steps: VecDeque<ScoreStepPlayed>,
    timer: Option<Timer>,
}

/// Un balayage de curseur en cours : le bus, et l'avancement de 0 à 1.
#[derive(Resource, Default)]
struct Sweep(Option<(Bus, f32)>);

fn blind(kind: BlindType, current_score: u64) -> BlindContext {
    let target_score = 1_000;
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

fn countdown_steps() -> VecDeque<ScoreStepPlayed> {
    let die = |die_id, value| ScoreStepPlayed {
        source: StepSource::Die {
            die_id: DieId(die_id),
            value,
        },
        action: ScoreAction::AddChips(u64::from(value) * 10),
    };
    VecDeque::from([
        ScoreStepPlayed {
            source: StepSource::HandBase {
                hand: YahtzeeHand::FullHouse,
            },
            action: ScoreAction::AddChips(30),
        },
        die(0, 3),
        die(1, 3),
        die(2, 3),
        die(3, 5),
        die(4, 5),
        ScoreStepPlayed {
            source: StepSource::Seal {
                die_id: DieId(4),
                seal: DieSeal::Gold,
            },
            action: ScoreAction::AddChips(20),
        },
        ScoreStepPlayed {
            source: StepSource::Relic {
                uid: 7,
                def: RelicId::CrackedDie,
            },
            action: ScoreAction::AddMult(400),
        },
        ScoreStepPlayed {
            source: StepSource::Relic {
                uid: 8,
                def: RelicId::CrackedDie,
            },
            action: ScoreAction::MultiplyMult(300),
        },
    ])
}

/// Le pupitre du banc : ce que les commandes écrivent, pris ensemble.
#[derive(SystemParam)]
struct Desk<'w> {
    wanted: ResMut<'w, Wanted>,
    countdown: ResMut<'w, Countdown>,
    sweep: ResMut<'w, Sweep>,
    volumes: ResMut<'w, AudioBusVolumes>,
}

fn obey(
    lines: Res<Lines>,
    mut commands: Commands,
    desk: Desk,
    mut bank: ResMut<SoundEffectBank>,
    mut backend: ResMut<AudioBackendHandle>,
    blind_now: Option<ResMut<BlindContext>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(line) = lines.0.lock().expect("verrou").try_recv() else {
        return;
    };
    let Desk {
        mut wanted,
        mut countdown,
        mut sweep,
        mut volumes,
    } = desk;
    let mut in_run = |phase: RunPhase, context: BlindContext| {
        commands.insert_resource(context);
        *wanted = Wanted {
            app: AppState::InRun,
            phase: Some(phase),
        };
    };
    let mut effect = |clip, bus| backend.0.play(clip, bus, local_gain(1.0), 1.0);
    match line.trim() {
        "menu" => {
            *wanted = Wanted {
                app: AppState::MainMenu,
                phase: None,
            }
        }
        "run" => in_run(RunPhase::Roll, blind(BlindType::Small, 0)),
        "boss" => in_run(RunPhase::Roll, blind(BlindType::Boss, 0)),
        "climax" => in_run(RunPhase::Roll, blind(BlindType::Boss, 800)),
        "boutique" => in_run(RunPhase::Shop, blind(BlindType::Small, 0)),
        "victoire" => in_run(RunPhase::RoundEnd, blind(BlindType::Small, 1_000)),
        "decompte" => {
            in_run(RunPhase::Scoring, blind(BlindType::Small, 0));
            countdown.steps = countdown_steps();
            countdown.timer = Some(Timer::new(Duration::from_millis(250), TimerMode::Repeating));
        }
        "case" => match blind_now {
            Some(mut context) => {
                let free = YahtzeeHand::ALL
                    .into_iter()
                    .find(|hand| !context.used_hands.contains(*hand));
                match free {
                    Some(hand) => context.used_hands.mark(hand),
                    None => println!("les treize cases sont consommées : relance `run`"),
                }
            }
            None => println!("aucune manche en cours : `run` d'abord"),
        },
        "lancer" => {
            let (clip, pitch) = bank.dice_roll();
            backend.0.play(clip, Bus::Sfx, local_gain(1.0), pitch);
        }
        "tic" => effect(bank.chip_tick, Bus::Sfx),
        "relique" => effect(bank.relic_chord, Bus::Sfx),
        "base" => effect(bank.hand_base_chord, Bus::Sfx),
        "coup" => effect(bank.mult_hit, Bus::SfxReverb),
        "sceau" => effect(bank.seal_tick, Bus::Sfx),
        "verrou" => effect(bank.die_lock, Bus::Sfx),
        "loquet" => effect(bank.hand_consumed, Bus::Sfx),
        "survol" => effect(bank.ui_hover, Bus::Sfx),
        "clic" => effect(bank.ui_click, Bus::Sfx),
        "piece" => effect(bank.coin, Bus::Sfx),
        "fanfare" => effect(bank.victory_fanfare, Bus::Sfx),
        "master" => sweep.0 = Some((Bus::Master, 0.0)),
        "music" => sweep.0 = Some((Bus::Music, 0.0)),
        "sfx" => sweep.0 = Some((Bus::Sfx, 0.0)),
        "m0" => volumes.master = 0.0,
        "m5" => volumes.master = 0.5,
        "m1" => volumes.master = 1.0,
        "u0" => volumes.music = 0.0,
        "u5" => volumes.music = 0.5,
        "u1" => volumes.music = 1.0,
        "s0" => volumes.sfx = 0.0,
        "s5" => volumes.sfx = 0.5,
        "s1" => volumes.sfx = 1.0,
        "aide" | "" => println!("{HELP}"),
        "q" => {
            exit.write(AppExit::Success);
        }
        other => println!("commande inconnue : {other}\n{HELP}"),
    }
}

/// Rejoint l'état demandé, une transition à la fois. `victoire` rejoue l'entrée en fin de
/// manche même si on y est déjà : c'est une transition réflexive, celle du jeu.
fn reach(
    mut wanted: ResMut<Wanted>,
    app: Res<State<AppState>>,
    phase: Option<Res<State<RunPhase>>>,
    mut next_app: ResMut<NextState<AppState>>,
    next_phase: Option<ResMut<NextState<RunPhase>>>,
) {
    if *app.get() != wanted.app {
        next_app.set(wanted.app);
        return;
    }
    // La sous-phase n'existe qu'une image après l'entrée en run : on garde la demande jusque-là.
    let (Some(_), Some(mut next)) = (phase, next_phase) else {
        return;
    };
    if let Some(target) = wanted.phase.take() {
        next.set(target);
    }
}

fn publish(
    time: Res<Time>,
    mut countdown: ResMut<Countdown>,
    mut steps: MessageWriter<ScoreStepPlayed>,
) {
    let Some(timer) = countdown.timer.as_mut() else {
        return;
    };
    if !timer.tick(time.delta()).just_finished() {
        return;
    }
    match countdown.steps.pop_front() {
        Some(step) => {
            steps.write(step);
        }
        None => countdown.timer = None,
    }
}

/// Le curseur descend de 1 à 0 puis remonte, **en continu** : c'est pendant le mouvement qu'un
/// craquement s'entendrait, pas à l'arrivée.
fn slide(time: Res<Time>, mut sweep: ResMut<Sweep>, mut volumes: ResMut<AudioBusVolumes>) {
    let Some((bus, progress)) = sweep.0 else {
        return;
    };
    let progress = progress + time.delta_secs() / 4.0;
    let value = if progress >= 1.0 {
        1.0
    } else {
        (2.0 * progress - 1.0).abs()
    };
    match bus {
        Bus::Master => volumes.master = value,
        Bus::Music => volumes.music = value,
        Bus::Sfx | Bus::SfxReverb => volumes.sfx = value,
    }
    sweep.0 = (progress < 1.0).then_some((bus, progress));
}

/// Les avertissements de la crate, sur la sortie d'erreur : un fichier manquant ou illisible se
/// dit ici (« remplacé par un bip », « stem introuvable »), au lieu de passer pour un choix.
struct Stderr;

impl log::Log for Stderr {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

fn main() {
    log::set_logger(&Stderr).expect("un seul journaliseur par processus");
    log::set_max_level(log::LevelFilter::Warn);

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });

    println!("Banc d'écoute de l'Étape 8. Au menu, la bande-son monte en fondu.\n{HELP}");
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
        StatesPlugin,
        AssetPlugin {
            // La racine de production, en chemin absolu : le banc se lance aussi hors de cargo.
            file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").to_string(),
            ..Default::default()
        },
    ));
    app.init_state::<AppState>().add_sub_state::<RunPhase>();
    app.add_message::<ScoreStepPlayed>();
    app.add_plugins(GameAudioPlugin::new());
    app.insert_resource(Lines(Mutex::new(receiver)))
        .insert_resource(Wanted {
            app: AppState::MainMenu,
            phase: None,
        })
        .init_resource::<Countdown>()
        .init_resource::<Sweep>()
        .add_systems(Update, (obey, reach, publish, slide).chain());
    app.run();
}
