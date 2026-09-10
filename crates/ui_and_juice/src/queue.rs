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
use core_engine::blind::BlindContext;
use core_engine::dice::Die;
use core_engine::scoring::{ScoreAction, ScoreStep, StepSource};
use game_state::{DieView, RelicSlotUI, ScoringStepQueue};

use crate::animation::{AnimatedNumber, CounterKind, PunchScale, ScreenShake};
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
    pub counters: Query<'w, 's, (Entity, &'static mut AnimatedNumber, &'static CounterKind)>,
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

/// **Commet le score. C'est le seul endroit du projet qui le fait.**
///
/// Deux gardes cumulatives, dans cet ordre : la file doit être vide, puis la
/// pause finale écoulée. Le drapeau `committed` retient le reste — sans lui,
/// chaque frame de la même phase recommettrait le total, et le score partirait
/// à l'infini en quelques dixièmes de seconde sans qu'aucun test de dépilement
/// ne le voie. La garde d'état ne suffirait pas : `NextState` ne s'applique
/// qu'au prochain `StateTransition`.
///
/// # La pause finale n'est jamais accélérée
///
/// `final_pause.tick(time.delta())`, sans `mul_f32`. C'est le respirateur qui
/// rend le total lisible ; l'accélération porte sur `step_timer`, et sur lui
/// seul. Le drapeau, lui, est remis à `false` au **remplissage** de la file, à
/// l'entrée dans la phase, jamais ici.
///
/// # Ce système ne transite pas, et ne le peut pas
///
/// **Dérogation au corpus, motivée.** Le § 3.6 du document source terminait par
/// un `next.set_if_neq(RunPhase::RoundEnd)`. La transition appartient à
/// `game_state` depuis TASK-38, qui la teste et l'a déclarée sienne par écrit ;
/// l'y reprendre laisserait `game_state` incapable de quitter une phase qu'il
/// sait pourtant entrer, et rendrait trois tests d'audit invérifiables.
/// `leave_scoring_when_queue_is_empty` attend donc le drapeau, et l'ordre est
/// garanti par lui plutôt que par l'ordonnancement de deux crates.
///
/// Le bénéfice n'est pas que d'économie : sans `NextState` dans la signature,
/// **ce système ne peut pas arbitrer**. Ni victoire, ni défaite, ni boutique.
/// La règle « l'Étape 4 n'arbitre rien » cesse d'être une consigne pour devenir
/// une impossibilité de compilation. `RunSession` et `AppState` en sont absents
/// pour la même raison.
///
/// Le total commis est le `u64` du pipeline, transporté tel quel : cette étape
/// **rejoue** un score, elle ne le calcule pas. Aucun `f64` n'entre ici,
/// `AnimatedNumber` étant un affichage et non une source. Et marquer
/// `used_hands` **est** la consommation de la case (ADR-001) : une main
/// abandonnée avant la fin du dépilement ne consomme rien, ce qui est voulu.
pub fn commit_score_when_drained(
    time: Res<Time>,
    mut queue: ResMut<ScoringStepQueue>,
    mut blind: ResMut<BlindContext>,
) {
    if queue.committed || !queue.steps.is_empty() {
        return;
    }
    queue.final_pause.tick(time.delta()); // JAMAIS multipliée par effective_speed()
    if !queue.final_pause.is_finished() {
        return;
    }

    // ---- COMMIT UNIQUE. Aucun autre système du projet ne fait ceci. ----
    blind.current_score = blind.current_score.saturating_add(queue.final_score);
    // `saturating_sub` : à zéro, un `-` nu rendrait 255 en release et
    // paniquerait en debug.
    blind.hands_remaining = blind.hands_remaining.saturating_sub(1);
    blind.used_hands.mark(queue.hand); // grille consommable
    queue.committed = true;
}

/// Amplitude d'une pulsation ordinaire, et de la pulsation renforcée d'une
/// multiplication. **Réglages d'Étape 7, pas des constantes normatives** : le
/// corpus dit « pulsation » et « pulsation renforcée » sans un chiffre.
///
/// Mesuré à TASK-49 sur le ressort de TASK-43 : le sursaut d'échelle vaut
/// environ **trois pour cent par unité d'impulsion**, linéairement. La fourchette
/// lisible de 10 à 30 % correspond donc à des amplitudes de 3,5 à 8.
const PULSE_NOMINALE: f32 = 4.0;
const PULSE_RENFORCEE: f32 = 8.0;
/// Trauma ajouté par une multiplication. L'amplitude étant quadratique
/// (TASK-45), 0,6 donne 0,36 × 12 px, soit une secousse franche et lisible.
const TRAUMA_MULTIPLICATION: f32 = 0.6;

/// Met en scène un palier : une impulsion pour sa source, une pour son action.
///
/// **Les deux axes sont indépendants.** Un palier traverse une branche de
/// `source` **et** une branche de `action` ; une source qui frappe un dé et une
/// action qui frappe un compteur produisent donc deux impulsions. Quand les deux
/// désignent la même entité, la seconde insertion relance le ressort — c'est le
/// contrat de ré-insertion de TASK-43, pas une somme.
///
/// Il pose des composants et écrit des cibles ; **il n'écrit aucun `Transform`**
/// et **ne joue aucun son**. Il ne commet rien non plus : le score, la main et la
/// grille appartiennent à TASK-50.
fn dispatch_step(step: &ScoreStep, ctx: &mut DispatchParams) {
    // **Inconditionnelle, et indépendante de la résolution d'entité.** Un palier
    // dont la cible est introuvable émet quand même : sinon l'Étape 8 perdrait
    // un son sans raison observable.
    ctx.events.write(ScoreStepPlayed::from(step));

    // ---- axe « source » ----
    let cible = match step.source {
        // Les deux sources frappent le même dé. Seule la teinte les
        // distinguait, et la teinte attend l'Étape 9 : il n'existe aujourd'hui
        // aucune entité porteuse d'une couleur dans le dépôt, et la palette
        // appartient à l'Étape 7. Ce bras se scindera quand la teinte aura de
        // quoi s'écrire ; deux bras au corps identique ne diraient rien de plus
        // et aucun test ne pourrait les distinguer.
        StepSource::Die { die_id, .. } | StepSource::Seal { die_id, .. } => {
            // **Résolution par identité, jamais par position.** `dice_count`
            // vaut 4, 5 ou 6 selon le gobelet et `DieView.order` est un rang
            // d'affichage recalculé à chaque manche : indexer produirait une
            // impulsion sur le mauvais dé au premier gobelet non standard.
            ctx.dice
                .iter()
                .find(|(_, _, die)| die.id == die_id)
                .map(|(entity, _, _)| entity)
        }
        // La base de la figure alimente les Chips : la faire pulser là où le
        // nombre bouge est ce qu'un joueur lit. **C'est une interprétation** de
        // la « boîte de score de départ » du corpus, faute d'entité ou de
        // marqueur qui la désigne ; l'Étape 9 peut la contredire.
        StepSource::HandBase { .. } => entite_du_compteur(ctx, CounterKind::Chips),
        // **Bras volontairement vide.** `RelicId` n'a aucune variante hors des
        // builds de test de `core_engine` : un `StepSource::Relic` n'est pas
        // constructible ici, donc aucun test ne peut couvrir ce bras. L'écrire
        // le ferait paraître livré alors que rien ne le vérifie. Il se câble à
        // l'étape qui livre le catalogue, seule capable de le tester.
        StepSource::Relic { .. } => None,
    };
    frapper(ctx, cible, PULSE_NOMINALE);

    // ---- axe « action » ----
    // Les trois cibles suivent le palier à chaque pas, pas seulement celle que
    // l'action touche : chaque palier porte l'état d'après des trois compteurs.
    for (_, mut number, kind) in &mut ctx.counters {
        number.target = kind.target_for(step);
    }

    let (compteur, force) = match step.action {
        ScoreAction::AddChips(_) => (CounterKind::Chips, PULSE_NOMINALE),
        ScoreAction::AddMult(_) => (CounterKind::Mult, PULSE_NOMINALE),
        ScoreAction::MultiplyMult(_) => {
            ctx.shake.add_trauma(TRAUMA_MULTIPLICATION);
            (CounterKind::Mult, PULSE_RENFORCEE)
        }
    };
    let cible = entite_du_compteur(ctx, compteur);
    frapper(ctx, cible, force);
}

/// L'entité portant ce compteur, si la scène en porte un.
fn entite_du_compteur(ctx: &DispatchParams, kind: CounterKind) -> Option<Entity> {
    ctx.counters
        .iter()
        .find(|(_, _, porte)| **porte == kind)
        .map(|(entity, _, _)| entity)
}

/// Pose l'impulsion **par commande**, et ne fait rien si la cible manque.
///
/// Aucune panique, aucun `unwrap` : un dé despawné ou un compteur que l'écran
/// ne porte pas encore est un cas normal, pas une erreur.
fn frapper(ctx: &mut DispatchParams, cible: Option<Entity>, amplitude: f32) {
    if let Some(entity) = cible {
        ctx.commands
            .entity(entity)
            .insert(PunchScale::impulse(amplitude));
    }
}

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
        // Le commit réclame la manche : `ui_and_juice` n'est pas montable sans
        // `game_state`, et une manche absente pendant le comptage est un
        // montage cassé, pas un cas nominal.
        app.insert_resource(manche(0, 3, 9_999));
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

    // ---- Accessibilité : flashs (TASK-51) ----

    use crate::animation::FlashOverlay;

    fn alpha(app: &mut App) -> f32 {
        let mut etat = app
            .world_mut()
            .query_filtered::<&BackgroundColor, With<FlashOverlay>>();
        etat.iter(app.world())
            .map(|c| c.0.alpha())
            .next()
            .expect("le nœud de flash")
    }

    /// Une scène de `n` multiplications, dépilées à `vitesse`, avec relevé de
    /// l'alpha **à chaque frame**.
    fn multiplications(n: usize, vitesse: u32, intensite_flash: f32) -> (Scene, Vec<f32>) {
        let paliers = (0..n)
            .map(|_| {
                palier_de(
                    StepSource::HandBase {
                        hand: YahtzeeHand::FullHouse,
                    },
                    ScoreAction::MultiplyMult(150),
                )
            })
            .collect();
        let mut s = scene(paliers);
        s.app
            .world_mut()
            .resource_mut::<JuiceSettings>()
            .flash_intensity = intensite_flash;
        s.app
            .world_mut()
            .resource_mut::<ScoringStepQueue>()
            .set_speed_multiplier(vitesse)
            .expect("vitesse admise");

        // 64 frames de 15 625 µs : une seconde simulée exactement.
        let releve = (0..64)
            .map(|_| {
                frame(&mut s.app);
                alpha(&mut s.app)
            })
            .collect();
        (s, releve)
    }

    /// Nombre de **fronts montants** : un flash est un passage de zéro à une
    /// valeur non nulle. C'est ce que l'œil compte, et ce que le plafond borne.
    fn flashs(releve: &[f32]) -> usize {
        releve
            .iter()
            .zip(std::iter::once(&0.0).chain(releve.iter()))
            .filter(|(maintenant, avant)| **maintenant > 0.0 && **avant == 0.0)
            .count()
    }

    /// Demande un flash **sans passer par la file** : un message de palier joué
    /// écrit à la main, pour maîtriser l'instant de chaque demande.
    fn demander_un_flash(app: &mut App) {
        app.world_mut().write_message(ScoreStepPlayed {
            source: StepSource::HandBase {
                hand: YahtzeeHand::FullHouse,
            },
            action: ScoreAction::MultiplyMult(150),
        });
    }

    fn app_flash(intensite: f32) -> App {
        let mut s = scene(Vec::new());
        s.app
            .world_mut()
            .resource_mut::<JuiceSettings>()
            .flash_intensity = intensite;
        s.app
    }

    #[test]
    fn test_refused_flash_does_not_spend_the_budget() {
        // **Un refus ne coûte rien.** Sinon, un joueur qui réactive les flashs
        // après les avoir coupés resterait jusqu'à une seconde sans en voir un
        // seul : le budget aurait été consommé par des flashs invisibles.
        let mut app = app_flash(0.0);
        for _ in 0..3 {
            demander_un_flash(&mut app);
            frame_de(&mut app, 15_625);
        }
        app.world_mut()
            .resource_mut::<JuiceSettings>()
            .flash_intensity = 1.0;

        demander_un_flash(&mut app);
        frame_de(&mut app, 15_625);
        assert!(
            alpha(&mut app) > 0.0,
            "le budget a été dépensé par des flashs que personne n'a vus"
        );
    }

    #[test]
    fn test_flashes_resume_one_second_after_the_first() {
        // La fenêtre glisse : elle expire une seconde après le **premier** flash
        // émis, pas après le dernier refus. Sans purge, ou avec une fenêtre trop
        // longue, la reprise n'arriverait jamais.
        let mut app = app_flash(1.0);
        for _ in 0..6 {
            demander_un_flash(&mut app);
            frame_de(&mut app, 125_000);
        }
        // 0,75 s écoulées, trois flashs émis à 0,125 / 0,25 / 0,375 s et trois
        // refusés. La fenêtre expire une seconde après le **premier**, soit à
        // 1,125 s : c'est cette date-là qu'il faut dépasser, pas la seconde
        // ronde.
        frame_de(&mut app, 400_000);
        demander_un_flash(&mut app);
        frame_de(&mut app, 15_625);
        assert!(
            alpha(&mut app) > 0.0,
            "les flashs n'ont pas repris après l'expiration de la fenêtre"
        );
    }

    #[test]
    fn test_only_multiplications_flash() {
        let mut app = app_flash(1.0);
        for action in [ScoreAction::AddChips(10), ScoreAction::AddMult(400)] {
            app.world_mut().write_message(ScoreStepPlayed {
                source: StepSource::HandBase {
                    hand: YahtzeeHand::FullHouse,
                },
                action,
            });
            frame_de(&mut app, 15_625);
            assert_eq!(alpha(&mut app), 0.0, "{action:?} a déclenché un flash");
        }
    }

    #[test]
    fn test_peak_alpha_follows_the_setting() {
        // L'intensité **module** le flash, elle ne fait pas que l'autoriser.
        let pic = |intensite: f32| {
            let mut app = app_flash(intensite);
            demander_un_flash(&mut app);
            frame_de(&mut app, 15_625);
            alpha(&mut app)
        };
        let plein = pic(1.0);
        let moitie = pic(0.5);
        assert!(plein > 0.0);
        // Égalité **exacte** : multiplier par 0,5 puis par 2 ne perd rien en
        // binaire, et le corpus proscrit les comparaisons approchées.
        assert_eq!(
            moitie * 2.0,
            plein,
            "un flash à moitié d'intensité ne vaut pas la moitié"
        );
    }

    #[test]
    fn test_reduce_flashes_disables_full_screen_flash() {
        let (mut s, releve) = multiplications(6, 2, 0.0);

        assert_eq!(flashs(&releve), 0, "un flash a été émis à intensité nulle");
        assert!(
            releve.iter().all(|a| *a == 0.0),
            "l'alpha n'est pas resté nul"
        );

        // **Le retour local reste entier.** C'est la règle 1 : le mode
        // accessibilité rend le jeu moins agressif, jamais moins lisible.
        assert!(
            s.app.world().resource::<ScreenShake>().trauma > 0.0,
            "le trauma a disparu avec le flash"
        );
        let mult = s.compteurs[1];
        assert!(
            s.app
                .world()
                .get::<AnimatedNumber>(mult)
                .expect("compteur")
                .target
                > 0.0,
            "la cible du compteur Mult n'a pas été écrite"
        );
        assert!(
            porteurs_de_punch(&mut s.app).contains(&mult),
            "la pulsation du compteur Mult a disparu"
        );
        // **Et elle garde son amplitude.** Vérifier la seule présence du
        // composant laisserait passer une impulsion nulle, qui porte un
        // `PunchScale` sans rien montrer : le banc l'a montré.
        let ressort = s.app.world().get::<PunchScale>(mult).expect("ressort");
        assert!(
            ressort.velocity >= PULSE_RENFORCEE,
            "la pulsation a été affaiblie avec le flash : {}",
            ressort.velocity
        );
    }

    #[test]
    fn test_flash_cap_is_three_per_second() {
        // Six multiplications en une seconde, à x2 : huit paliers par seconde.
        let (_, releve) = multiplications(6, 2, 1.0);
        assert_eq!(
            flashs(&releve),
            3,
            "le plafond de trois changements de luminance par seconde n'est pas tenu"
        );
    }

    #[test]
    fn test_flash_returns_to_black_between_two_flashes() {
        // **Sans ce test, l'Étape 7 peut annuler le plafond en croyant ne
        // toucher qu'à l'esthétique** : trois flashs par seconde dont la
        // décroissance dure plus que leur écartement donnent un écran
        // continûment blanc, et le plafond ne protège plus personne.
        let (_, releve) = multiplications(6, 2, 1.0);
        let pics: Vec<usize> = releve
            .iter()
            .enumerate()
            .zip(std::iter::once(&0.0).chain(releve.iter()))
            .filter(|((_, a), avant)| **a > 0.0 && **avant == 0.0)
            .map(|((i, _), _)| i)
            .collect();
        assert_eq!(pics.len(), 3);
        for paire in pics.windows(2) {
            assert!(
                releve[paire[0]..paire[1]].contains(&0.0),
                "l'écran n'est pas repassé par le noir entre deux flashs"
            );
        }
    }

    #[test]
    fn test_flash_and_shake_settings_are_independent() {
        // Deux réglages, deux effets : aucune branche ne lit l'un pour décider
        // de l'autre. Le relevé porte sur les **fronts montants**, le flash
        // pouvant s'être éteint avant la dernière frame.
        let une_multiplication = || {
            vec![palier_de(
                StepSource::HandBase {
                    hand: YahtzeeHand::FullHouse,
                },
                ScoreAction::MultiplyMult(150),
            )]
        };

        // Secousse coupée : le flash reste.
        let mut s = scene(une_multiplication());
        s.app
            .world_mut()
            .resource_mut::<JuiceSettings>()
            .shake_intensity = 0.0;
        let releve: Vec<f32> = (0..24)
            .map(|_| {
                frame(&mut s.app);
                alpha(&mut s.app)
            })
            .collect();
        assert_eq!(flashs(&releve), 1, "la secousse coupée a emporté le flash");

        // Flash coupé : le trauma reste. C'est l'amplitude que
        // `shake_intensity` annule, pas le trauma lui-même.
        let mut s = scene(une_multiplication());
        s.app
            .world_mut()
            .resource_mut::<JuiceSettings>()
            .flash_intensity = 0.0;
        let releve: Vec<f32> = (0..24)
            .map(|_| {
                frame(&mut s.app);
                alpha(&mut s.app)
            })
            .collect();
        assert_eq!(flashs(&releve), 0, "un flash a survécu à son extinction");
        assert!(
            s.app.world().resource::<ScreenShake>().trauma > 0.0,
            "le flash coupé a emporté le trauma"
        );
    }

    // ---- Commit unique du score (TASK-50) ----

    use core_engine::blind::{BlindContext, BlindDefinition};
    use core_engine::hands::HandGrid;

    /// Une manche prête à recevoir un commit.
    fn manche(score: u64, mains: u8, cible: u64) -> BlindContext {
        BlindContext {
            blind: BlindDefinition::default(),
            target_score: cible,
            current_score: score,
            hands_remaining: mains,
            used_hands: HandGrid::default(),
        }
    }

    /// App du commit, **sans garde d'état** : le système tourne à chaque frame,
    /// de sorte que les tests éprouvent le drapeau `committed` et non la garde.
    /// Sous garde, `test_score_committed_exactly_once` ne prouverait que la
    /// sortie de phase.
    fn app_commit(file: ScoringStepQueue, manche: BlindContext) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins.build().disable::<TimePlugin>());
        app.init_resource::<Time>();
        app.insert_resource(file);
        app.insert_resource(manche);
        app.add_systems(Update, commit_score_when_drained);
        app
    }

    fn file_drainee(total: u64, figure: YahtzeeHand) -> ScoringStepQueue {
        ScoringStepQueue::new(VecDeque::new(), figure, total)
    }

    fn avancer_de(app: &mut App, microsecondes: u64) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_micros(microsecondes));
        app.update();
    }

    fn score(app: &App) -> u64 {
        app.world().resource::<BlindContext>().current_score
    }

    #[test]
    fn test_score_committed_exactly_once() {
        let mut app = app_commit(
            file_drainee(1_000, YahtzeeHand::FullHouse),
            manche(500, 3, 9_999),
        );
        avancer_de(&mut app, 500_000);
        assert_eq!(score(&app), 1_500, "le total n'a pas été commis");

        // Cent frames de plus, système toujours actif : le drapeau seul retient.
        for _ in 0..100 {
            avancer_de(&mut app, 16_667);
        }
        assert_eq!(score(&app), 1_500, "le score a été commis plus d'une fois");
        assert_eq!(app.world().resource::<BlindContext>().hands_remaining, 2);
    }

    #[test]
    fn test_final_pause_is_not_accelerated() {
        let mut file = file_drainee(1_000, YahtzeeHand::FullHouse);
        file.set_speed_multiplier(2).expect("vitesse admise");
        file.fast_forward = true;
        assert_eq!(file.effective_speed(), 8.0, "le montage ne teste rien");

        let mut app = app_commit(file, manche(0, 3, 9_999));
        // 0,49 s : rien. La pause vaut 0,5 s même à x8.
        avancer_de(&mut app, 490_000);
        assert_eq!(score(&app), 0, "la pause finale a été accélérée");
        avancer_de(&mut app, 10_000);
        assert_eq!(score(&app), 1_000);
    }

    #[test]
    fn test_nothing_is_committed_while_steps_remain() {
        let mut file = file_drainee(1_000, YahtzeeHand::FullHouse);
        file.steps.push_back(palier_de(
            StepSource::HandBase {
                hand: YahtzeeHand::FullHouse,
            },
            ScoreAction::AddChips(10),
        ));
        let mut app = app_commit(file, manche(0, 3, 9_999));
        for _ in 0..60 {
            avancer_de(&mut app, 16_667);
        }
        assert_eq!(score(&app), 0, "commis avec des paliers en attente");
    }

    #[test]
    fn test_hands_remaining_saturates() {
        let mut app = app_commit(
            file_drainee(10, YahtzeeHand::FullHouse),
            manche(0, 0, 9_999),
        );
        avancer_de(&mut app, 500_000);
        assert_eq!(
            app.world().resource::<BlindContext>().hands_remaining,
            0,
            "une soustraction nue aurait rendu 255"
        );
    }

    #[test]
    fn test_used_hands_marks_played_hand() {
        let mut app = app_commit(
            file_drainee(10, YahtzeeHand::FullHouse),
            manche(0, 3, 9_999),
        );
        avancer_de(&mut app, 500_000);
        let grille = app.world().resource::<BlindContext>().used_hands;
        for figure in YahtzeeHand::ALL {
            assert_eq!(
                grille.contains(figure),
                figure == YahtzeeHand::FullHouse,
                "grille fausse pour {figure:?}"
            );
        }
    }

    #[test]
    fn test_a_closed_queue_commits_nothing() {
        // Entrée stérile : la file au repos naît `committed`, elle n'a rien à
        // rejouer ni à commettre. C'est la raison d'être du `Default` de
        // TASK-47, et ce test en fait une décision.
        let mut app = app_commit(ScoringStepQueue::default(), manche(500, 3, 9_999));
        avancer_de(&mut app, 500_000);
        assert_eq!(score(&app), 500);
        assert_eq!(app.world().resource::<BlindContext>().hands_remaining, 3);
    }

    #[test]
    fn test_commit_precedes_the_phase_exit() {
        // La sortie de phase appartient à `game_state`, qui attend le drapeau.
        // Ici on vérifie l'ordre : quand `committed` passe à vrai, les trois
        // écritures sont **déjà** faites, donc `RoundEnd` arbitrera sur un
        // contexte à jour.
        let mut app = app_commit(
            file_drainee(1_000, YahtzeeHand::FullHouse),
            manche(0, 3, 9_999),
        );
        avancer_de(&mut app, 500_000);
        let file = app.world().resource::<ScoringStepQueue>();
        assert!(file.committed);
        assert_eq!(score(&app), 1_000);
        assert_eq!(app.world().resource::<BlindContext>().hands_remaining, 2);
    }

    // ---- Mise en scène d'un palier (TASK-49) ----

    use crate::animation::{AnimatedNumber, CounterKind, PunchScale};
    use core_engine::dice::{Die, DieId, DieSeal};
    use game_state::{DieView, RelicSlotUI};

    struct Scene {
        app: App,
        des: Vec<Entity>,
        compteurs: Vec<Entity>,
    }

    /// Une variante de source et les entités qu'elle doit faire pulser.
    type Cas = (StepSource, fn(&Scene) -> Vec<Entity>);

    /// Trois dés, deux slots de relique, trois compteurs — instanciés à la main,
    /// puisque **rien dans le dépôt n'instancie encore d'interface**.
    fn scene(paliers: Vec<ScoreStep>) -> Scene {
        let mut app = app_file(0);
        {
            let mut file = app.world_mut().resource_mut::<ScoringStepQueue>();
            file.steps = paliers.into_iter().collect();
        }

        let des = (1..=3)
            .map(|n| {
                app.world_mut()
                    .spawn((Die::new(DieId(n), 6), DieView { order: n as u8 }))
                    .id()
            })
            .collect();
        // Deux slots de relique, jamais relus : ils sont là pour que
        // l'assertion « aucune autre entité n'est frappée » ait de quoi
        // échouer.
        for n in 0..2u8 {
            app.world_mut().spawn(RelicSlotUI(n));
        }
        let compteurs = [CounterKind::Chips, CounterKind::Mult, CounterKind::Total]
            .into_iter()
            .map(|kind| {
                app.world_mut()
                    .spawn((
                        AnimatedNumber {
                            displayed: 0.0,
                            target: 0.0,
                            rate: 12.0,
                            decimals: u8::from(kind == CounterKind::Mult),
                        },
                        kind,
                    ))
                    .id()
            })
            .collect();

        brancher_le_journal(&mut app);

        Scene {
            app,
            des,
            compteurs,
        }
    }

    fn palier_de(source: StepSource, action: ScoreAction) -> ScoreStep {
        ScoreStep {
            source,
            action,
            chips_after: 120,
            mult_after: 430,
            score_after: 516,
        }
    }

    /// Une frame d'exactement une cadence : un palier, pas deux.
    fn une_cadence(scene: &mut Scene) {
        frame_de(&mut scene.app, 250_000);
    }

    fn porteurs_de_punch(app: &mut App) -> Vec<Entity> {
        let mut etat = app.world_mut().query_filtered::<Entity, With<PunchScale>>();
        let mut v: Vec<Entity> = etat.iter(app.world()).collect();
        v.sort();
        v
    }

    /// Les messages **enregistrés frame par frame**, jamais relus en fin de
    /// scénario.
    ///
    /// `Messages<T>` est un tampon tournant : au troisième `update()` les
    /// premiers messages ont disparu, et un test qui relit le tampon à la fin
    /// compte deux paliers là où quatre ont été joués. C'est le piège qui avait
    /// rendu la CI rouge à TASK-41, transmis par TASK-46 § 6 — et dans lequel ce
    /// ticket est retombé au premier essai.
    #[derive(Resource, Default)]
    struct Journal(Vec<ScoreStepPlayed>);

    fn brancher_le_journal(app: &mut App) {
        app.init_resource::<Journal>();
        app.add_systems(
            Last,
            |mut lecteur: MessageReader<ScoreStepPlayed>, mut journal: ResMut<Journal>| {
                journal.0.extend(lecteur.read().copied());
            },
        );
    }

    fn messages(app: &App) -> Vec<ScoreStepPlayed> {
        app.world().resource::<Journal>().0.clone()
    }

    #[test]
    fn test_dispatch_punches_expected_entity_only() {
        // **Deux axes indépendants** : la source frappe une entité, l'action en
        // frappe une autre. Le test porte donc sur l'ensemble exact des
        // porteurs, pas sur une seule entité — et sur le fait qu'aucune autre
        // n'est touchée.
        let cas: Vec<Cas> = vec![
            (
                StepSource::Die {
                    die_id: DieId(2),
                    value: 5,
                },
                |s: &Scene| vec![s.des[1], s.compteurs[0]],
            ),
            (
                StepSource::Seal {
                    die_id: DieId(3),
                    seal: DieSeal::Gold,
                },
                |s: &Scene| vec![s.des[2], s.compteurs[0]],
            ),
            (
                // Source et action désignent le même compteur : une seule
                // entité frappée, la ré-insertion relançant le ressort.
                StepSource::HandBase {
                    hand: YahtzeeHand::FullHouse,
                },
                |s: &Scene| vec![s.compteurs[0]],
            ),
        ];

        for (source, attendu) in cas {
            let mut s = scene(vec![palier_de(source, ScoreAction::AddChips(10))]);
            une_cadence(&mut s);
            let mut cibles = attendu(&s);
            cibles.sort();
            assert_eq!(
                porteurs_de_punch(&mut s.app),
                cibles,
                "mauvaises entités frappées pour {source:?}"
            );
        }
    }

    #[test]
    fn test_hand_base_punches_chips_even_when_the_action_targets_mult() {
        // **Les deux axes se masquent sur `HandBase` + `AddChips`** : la source
        // et l'action désignent alors le même compteur, et supprimer la branche
        // source passe inaperçu — le banc l'a montré. Un `AddMult` les sépare :
        // la source frappe les Chips, l'action frappe le Mult.
        let mut s = scene(vec![palier_de(
            StepSource::HandBase {
                hand: YahtzeeHand::FullHouse,
            },
            ScoreAction::AddMult(400),
        )]);
        let mut attendu = vec![s.compteurs[0], s.compteurs[1]];
        attendu.sort();
        une_cadence(&mut s);
        assert_eq!(porteurs_de_punch(&mut s.app), attendu);
    }

    #[test]
    fn test_add_mult_punches_the_mult_counter() {
        let mut s = scene(vec![palier_de(
            StepSource::HandBase {
                hand: YahtzeeHand::FullHouse,
            },
            ScoreAction::AddMult(400),
        )]);
        let mult = s.compteurs[1];
        une_cadence(&mut s);
        assert!(porteurs_de_punch(&mut s.app).contains(&mult));
    }

    #[test]
    fn test_counter_targets_come_from_the_step() {
        let mut s = scene(vec![palier_de(
            StepSource::Die {
                die_id: DieId(1),
                value: 4,
            },
            ScoreAction::AddChips(10),
        )]);
        let (chips, mult, total) = (s.compteurs[0], s.compteurs[1], s.compteurs[2]);
        une_cadence(&mut s);

        let cible = |app: &App, e: Entity| {
            app.world()
                .get::<AnimatedNumber>(e)
                .expect("compteur")
                .target
        };
        assert_eq!(cible(&s.app, chips), 120.0, "Chips");
        assert_eq!(cible(&s.app, mult), 4.3, "Mult : centièmes vers unités");
        assert_eq!(cible(&s.app, total), 516.0, "Total");
    }

    #[test]
    fn test_multiply_mult_adds_trauma_add_chips_does_not() {
        for (action, attendu) in [
            (ScoreAction::AddChips(10), false),
            (ScoreAction::MultiplyMult(150), true),
        ] {
            let mut s = scene(vec![palier_de(
                StepSource::HandBase {
                    hand: YahtzeeHand::FullHouse,
                },
                action,
            )]);
            une_cadence(&mut s);
            let trauma = s.app.world().resource::<ScreenShake>().trauma;
            assert_eq!(trauma > 0.0, attendu, "trauma pour {action:?}");
        }
    }

    #[test]
    fn test_reinforced_pulse_is_stronger_than_nominal() {
        // La propriété est le **rapport**, pas les valeurs : celles-ci sont des
        // réglages d'Étape 7 et bougeront.
        let mesure = |action: ScoreAction| {
            let mut s = scene(vec![palier_de(
                StepSource::HandBase {
                    hand: YahtzeeHand::FullHouse,
                },
                action,
            )]);
            let mult = s.compteurs[1];
            une_cadence(&mut s);
            s.app
                .world()
                .get::<PunchScale>(mult)
                .map(|p| p.velocity)
                .unwrap_or(0.0)
        };
        let nominale = mesure(ScoreAction::AddMult(400));
        let renforcee = mesure(ScoreAction::MultiplyMult(150));
        assert!(
            renforcee > nominale * 1.5,
            "la pulsation renforcée ne l'est pas : {nominale} puis {renforcee}"
        );
    }

    #[test]
    fn test_unknown_die_id_does_not_panic() {
        let mut s = scene(vec![palier_de(
            StepSource::Die {
                die_id: DieId(999),
                value: 4,
            },
            ScoreAction::AddChips(10),
        )]);
        une_cadence(&mut s);
        // L'axe « action » frappe quand même son compteur : c'est la **source**
        // qui n'a pas de cible, pas le palier.
        let chips = s.compteurs[0];
        assert_eq!(
            porteurs_de_punch(&mut s.app),
            vec![chips],
            "un dé fantôme a été frappé"
        );
        assert_eq!(
            messages(&s.app).len(),
            1,
            "l'événement doit partir quand même"
        );
    }

    #[test]
    fn test_one_event_per_step() {
        let paliers: Vec<ScoreStep> = (1..=4)
            .map(|n| {
                palier_de(
                    StepSource::Die {
                        die_id: DieId(n),
                        value: 3,
                    },
                    ScoreAction::AddChips(u64::from(n)),
                )
            })
            .collect();
        let mut s = scene(paliers.clone());
        for _ in 0..4 {
            une_cadence(&mut s);
        }
        let recus = messages(&s.app);
        assert_eq!(recus.len(), 4, "un événement par palier joué");
        assert_eq!(
            recus,
            paliers
                .iter()
                .map(ScoreStepPlayed::from)
                .collect::<Vec<_>>(),
            "ordre ou contenu altéré"
        );
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
