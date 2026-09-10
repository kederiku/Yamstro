//! Dépilement de la file de score : cadence, accélération, et rien d'autre.
//!
//! Ce module ne déclare **aucun type de données** : `ScoringStepQueue` vit dans
//! `game_state`, qui la remplit. La déplacer ici créerait une dépendance
//! circulaire (TASK-47 § 1.1).
//!
//! # Dépendances de montage
//!
//! Les deux systèmes prennent leurs ressources **directement**, sans `Option` :
//! `ui_and_juice` n'est pas montable sans `game_state`, dont il importe la
//! file, les composants et la phase. Une file absente au moment du comptage
//! n'est donc pas un cas nominal mais un montage cassé, et il vaut mieux qu'il
//! panique que de laisser le joueur devant un écran de comptage qui ne se vide
//! jamais. `read_fast_forward_input` ajoute `InputPlugin` à ces dépendances.
//!
//! # Ni chaînage ni garde d'état écrits ici
//!
//! TASK-42 a configuré `JuiceSet::ReadInput → TickQueue → Commit`, chaînés et
//! gardés par `in_state(RunPhase::Scoring)`. Les deux systèmes entrent dans
//! leurs ensembles : un `.chain()` ou un `run_if` supplémentaire ne ferait que
//! doubler ce qui existe.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use core_engine::dice::Die;
use core_engine::scoring::ScoreStep;
use game_state::{DieView, RelicSlotUI, ScoringStepQueue};

use crate::animation::{AnimatedNumber, ScreenShake};
use crate::events::ScoreStepPlayed;
use crate::settings::JuiceSettings;

/// Les accès dont dispose la mise en scène d'un palier — **et aucun autre**.
///
/// Le ticket fixait ces accès en huit paramètres nus ; clippy les refuse au
/// delà de sept, et `-D warnings` est dans la définition de terminé. Le
/// `SystemParam` contraint davantage qu'une signature plate : il nomme les
/// accès en un seul endroit, refuse tout aussi bien une requête mutable sur le
/// `Transform`, et rend structurelle la règle « le dépileur n'emprunte pas la
/// file » — elle n'est pas dans le bundle, donc elle n'y entrera pas par
/// distraction.
///
/// **Aucun accès mutable au `Transform` ici**, sous aucune forme. Un seul
/// système écrit celui d'une entité de jeu, `animate_punch_scale` (TASK-43), et
/// la caméra est traitée à part par `apply_screen_shake`. La garde du volet 1
/// interdit la forme même de cet emprunt partout ailleurs que dans le module
/// d'animation : elle est volontairement large, et cette phrase-ci est écrite
/// pour ne pas la déclencher.
#[derive(SystemParam)]
pub struct DispatchParams<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub dice: Query<'w, 's, (Entity, &'static DieView, &'static Die)>,
    pub slots: Query<'w, 's, (Entity, &'static RelicSlotUI)>,
    pub counters: Query<'w, 's, &'static mut AnimatedNumber>,
    pub shake: ResMut<'w, ScreenShake>,
    pub settings: Res<'w, JuiceSettings>,
    pub events: MessageWriter<'w, ScoreStepPlayed>,
}

/// `fast_forward` suit l'appui, **frame par frame**.
///
/// Une seule affectation, sans branche : c'est un maintien, pas une bascule. Un
/// `if pressed { … = true }` sans `else` resterait bloqué à `true` au premier
/// appui et le joueur ne retrouverait jamais sa vitesse de réglage.
pub fn read_fast_forward_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut queue: ResMut<ScoringStepQueue>,
) {
    queue.fast_forward = keys.pressed(KeyCode::Space) || mouse.pressed(MouseButton::Left);
}

/// Dépile les paliers échus, **tous ceux de la frame**.
///
/// # Deux pièges de `Timer`, et un seul remède
///
/// `Timer` n'a pas de champ « vitesse » : ni `set_speed`, ni `set_scale`.
/// L'accélération se fait en multipliant le pas de temps **avant** le tic,
/// jamais en réglant l'horloge globale, qui accélérerait aussi les animations
/// et la pause finale.
///
/// Et tester `finished()` ne dépilerait **qu'un palier par frame**. Un minuteur
/// `Repeating` réenroule son `elapsed` à chaque période franchie
/// (`checked_rem`) : tout ce qui dépasse une période dans le même tic est jeté
/// par un booléen. Dès que le pas multiplié dépasse la cadence — frame longue,
/// machine lente, WASM, réglage ×2 combiné au maintien ×4 — des paliers
/// disparaîtraient définitivement de la séquence.
/// `times_finished_this_tick()` rend le compte exact ; la boucle le consomme.
///
/// **`final_pause` n'est pas touchée ici** : elle appartient à TASK-50, et le
/// facteur ne lui est appliqué nulle part.
pub fn tick_scoring_queue(
    time: Res<Time>,
    mut queue: ResMut<ScoringStepQueue>,
    mut ctx: DispatchParams,
) {
    let factor = queue.effective_speed();
    queue.step_timer.tick(time.delta().mul_f32(factor));

    for _ in 0..queue.step_timer.times_finished_this_tick() {
        let Some(step) = queue.steps.pop_front() else {
            break;
        };
        dispatch_step(&step, &mut ctx);
    }
}

/// Met en scène un palier. **Corps écrit par TASK-49**, signature fixée ici.
///
/// Il pose des composants et écrit des cibles ; il n'écrit aucun `Transform` et
/// ne joue aucun son. Les cibles des compteurs se lisent dans le palier
/// lui-même — `chips_after`, `mult_after`, `score_after` — et rien n'y est
/// recalculé : cette étape rejoue un score, elle ne le calcule pas.
fn dispatch_step(_step: &ScoreStep, _ctx: &mut DispatchParams) {}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimePlugin;
    use core_engine::hands::YahtzeeHand;
    use core_engine::scoring::{ScoreAction, ScoreStep, StepSource};
    use game_state::{AppState, RunPhase, ScoringStepQueue};
    use std::collections::VecDeque;
    use std::time::Duration;

    /// Une frame de 15 625 µs, soit **2⁻⁶ s exactement**.
    ///
    /// Le comptage se fait par franchissement de seuil : un pouième de trop et
    /// le nombre de paliers change. `Duration::mul_f32` passant par un `f32`,
    /// une frame de 16 667 µs rendrait le résultat dépendant de l'arrondi —
    /// c'est ce qui avait fait échouer un test à TASK-44. Ici tout est exact :
    /// ×4 donne 2⁻⁴ s, et seize frames font exactement la cadence de 0,25 s.
    const FRAME: u64 = 15_625;

    fn palier(n: u64) -> ScoreStep {
        ScoreStep {
            source: StepSource::HandBase {
                hand: YahtzeeHand::ALL[(n % 13) as usize],
            },
            action: ScoreAction::AddChips(n),
            chips_after: n,
            mult_after: n as i64,
            score_after: n,
        }
    }

    fn app_file(nombre: u64) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins.build().disable::<TimePlugin>(),
            StatesPlugin,
            InputPlugin,
            crate::JuicePlugin,
        ));
        app.init_resource::<Time>();
        app.init_state::<AppState>();
        app.add_sub_state::<RunPhase>();
        app.insert_resource(ScoringStepQueue::new(
            (1..=nombre).map(palier).collect::<VecDeque<_>>(),
            YahtzeeHand::FullHouse,
            42,
        ));

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InRun);
        app.update();
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::Scoring);
        app.update();
        app
    }

    fn en_attente(app: &App) -> Vec<ScoreStep> {
        app.world()
            .resource::<ScoringStepQueue>()
            .steps
            .iter()
            .copied()
            .collect()
    }

    /// Avance d'une frame et rend **les paliers partis pendant celle-ci**, dans
    /// l'ordre où ils ont quitté la file.
    ///
    /// Reconstruit la séquence dépilée sans passer par un `dispatch_step`
    /// factice : ce qui a disparu du **début** de la file est exactement ce qui
    /// a été joué. Un `pop_back` se verrait aussitôt, la queue restant intacte
    /// et le début non.
    fn frame_de(app: &mut App, microsecondes: u64) -> Vec<ScoreStep> {
        let avant = en_attente(app);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_micros(microsecondes));
        app.update();
        let apres = en_attente(app);
        assert!(
            avant.ends_with(&apres),
            "la file n'a pas été dépilée par le début"
        );
        avant[..avant.len() - apres.len()].to_vec()
    }

    fn frame(app: &mut App) -> Vec<ScoreStep> {
        frame_de(app, FRAME)
    }

    fn maintenir(app: &mut App, appuye: bool) {
        let mut touches = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        if appuye {
            touches.press(KeyCode::Space);
        } else {
            touches.release(KeyCode::Space);
        }
    }

    #[test]
    fn test_queue_drains_completely() {
        for nombre in [1, 7, 40] {
            let mut app = app_file(nombre);
            let mut joues = Vec::new();
            for _ in 0..(16 * nombre + 4) {
                joues.extend(frame(&mut app));
            }
            assert!(en_attente(&app).is_empty(), "file non vidée à {nombre}");
            assert_eq!(joues.len() as u64, nombre, "paliers perdus à {nombre}");
        }
    }

    #[test]
    fn test_drain_respects_step_order() {
        let mut app = app_file(5);
        let mut joues = Vec::new();
        for _ in 0..84 {
            joues.extend(frame(&mut app));
        }
        assert_eq!(joues, (1..=5).map(palier).collect::<Vec<_>>());
    }

    #[test]
    fn test_nominal_cadence_is_250ms() {
        // x1, sans maintien : seize frames par palier, donc quatre paliers en
        // une seconde simulée.
        let mut app = app_file(40);
        let mut joues = 0;
        for _ in 0..64 {
            joues += frame(&mut app).len();
        }
        assert_eq!(joues, 4);
    }

    #[test]
    fn test_no_step_before_the_first_beat() {
        // Le minuteur naît à zéro : le premier palier arrive **après** la
        // cadence, pas à l'entrée. Ce temps mort est voulu — une respiration
        // avant que le compteur ne démarre — et ce test le dit, pour que
        // personne ne le « corrige » sans le savoir.
        let mut app = app_file(3);
        for _ in 0..15 {
            assert!(frame(&mut app).is_empty(), "un palier a joué trop tôt");
        }
        assert_eq!(
            frame(&mut app).len(),
            1,
            "le premier palier a manqué sa cadence"
        );
    }

    #[test]
    fn test_fast_forward_drains_multiple_steps_per_frame() {
        let mut app = app_file(40);
        maintenir(&mut app, true);

        // Frames régulières : à x4 la cadence effective vaut quatre frames.
        // L'assertion porte sur l'absence de **retard cumulé**, pas sur le
        // dépilement multiple, qui ne peut pas se produire ici.
        let mut joues = 0;
        for _ in 0..64 {
            joues += frame(&mut app).len();
        }
        assert_eq!(joues, 16, "retard cumulé à x4");

        // Hoquet de 140 625 µs : ×4 vaut 0,5625 s, soit deux paliers et un
        // reste franc. C'est cette frame-là qui **exhibe** le dépilement
        // multiple ; un test booléen sur `finished()` n'en rendrait qu'un.
        assert_eq!(
            frame_de(&mut app, 140_625).len(),
            2,
            "un palier a été jeté par la frame longue"
        );
    }

    #[test]
    fn test_left_click_also_fast_forwards() {
        // « Espace **ou** clic gauche » : le banc a montré que retirer la
        // moitié souris passait toute la suite, aucun test ne la touchant.
        let mut app = app_file(40);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);

        let mut joues = 0;
        for _ in 0..64 {
            joues += frame(&mut app).len();
        }
        assert_eq!(joues, 16, "le clic gauche n'accélère pas");
    }

    #[test]
    fn test_release_returns_to_setting_speed() {
        let mut app = app_file(40);
        maintenir(&mut app, true);
        for _ in 0..4 {
            frame(&mut app);
        }
        maintenir(&mut app, false);

        // Dès la frame suivante, la cadence est de nouveau celle du réglage :
        // seize frames par palier, donc rien pendant les quinze premières.
        let mut joues = 0;
        for _ in 0..15 {
            joues += frame(&mut app).len();
        }
        assert_eq!(joues, 0, "l'accélération est restée bloquée au relâchement");
        assert!(
            !app.world().resource::<ScoringStepQueue>().fast_forward,
            "fast_forward n'est pas redescendu"
        );
    }

    #[test]
    fn test_empty_queue_does_not_panic() {
        for vitesse in [1, 4] {
            let mut app = app_file(0);
            app.world_mut()
                .resource_mut::<ScoringStepQueue>()
                .set_speed_multiplier(vitesse)
                .expect("vitesse admise");
            maintenir(&mut app, vitesse == 4);
            for _ in 0..100 {
                assert!(frame(&mut app).is_empty());
            }
        }
    }
}
