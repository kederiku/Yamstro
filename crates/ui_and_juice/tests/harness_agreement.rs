//! Le fixture d'accord : le harnais et la boucle du jeu jouent la même manche.
//!
//! **Deux implémentations d'une même boucle divergent avec le temps.** Le
//! harnais réimplémente l'orchestration qui vit dans la crate d'états, parce
//! qu'il ne peut pas appeler une crate qui tire le moteur graphique. Ce fichier
//! est la parade : une graine de référence, un script figé, et la comparaison
//! main par main de ce que les deux boucles voient et commettent.
//!
//! **Il vit ici, et pas dans la crate d'états.** Le score commis n'est écrit
//! qu'à un endroit du dépôt, `queue.rs`, dans cette crate : un test d'accord
//! posé dans la crate d'états devrait la prendre en dépendance de
//! développement, et la garde de CI « dépendance inverse vers le juice » —
//! portée sur le chemin entier, manifeste compris — passerait au rouge. Cette
//! crate, elle, dépend déjà des deux autres : le test s'écrit d'ici **sans
//! ajouter une seule arête** au graphe du jeu.
//!
//! **Sous `tests/`, jamais sous `src/`** : un test d'intégration se lie à la
//! crate compilée sans la compilation de test, donc il ne peut nommer aucune
//! fixture interne — celles de la crate d'états lui sont inaccessibles, et ce
//! fichier remonte son propre montage.

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimePlugin;
use core_engine::blinds::{BlindContext, BlindType};
use core_engine::config::RunConfig;
use core_engine::cups::CupId;
use core_engine::cups::definitions::cup;
use core_engine::dice::{Die, DieId};
use core_engine::hands::{HandLevels, YahtzeeHand};
use core_engine::relics::RelicInventory;
use core_engine::rng::RunRng;
use game_state::{AppState, GameStatePlugin, HandContext, RunPhase, RunSession, ScoringStepQueue};
use rand::RngExt;
use rand_chacha::ChaCha8Rng;
use sim_harness::config::{PolicyKind, ShopPolicyKind, SimConfig};
use sim_harness::policy::Policy;
use sim_harness::policy::shop::BudgetShopPolicy;
use sim_harness::view::{HandDecision, HandView, LockMask};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use ui_and_juice::JuicePlugin;

/// La graine de référence, **déclarée une fois**.
///
/// Elle ne change pas sans que le script du § suivant soit revalidé dans le
/// même commit : chaque figure soumise doit rester formée par les dés que
/// cette graine produit, et `test_script_is_playable` en fait un échec de test
/// et non une fausse divergence.
const GRAINE_DE_REFERENCE: u64 = 12_345;

/// Le nombre de mains d'une Petite Mise au Gobelet Classique, à la Mise 1.
/// Lu de la configuration par le test, jamais écrit ici en littéral.
const MAINS_ATTENDUES: usize = 4;

/// Le script, **écrit à la main et figé**.
///
/// **Il n'est pas dérivé d'une passe du harnais**, et c'est le point. Un script
/// dérivé se plierait à l'instrument : si le harnais avait tort, le script
/// épouserait son erreur et les deux boucles s'accorderaient sur une faute
/// commune. Écrit à la main, il est une troisième voix.
///
/// **La comparaison porte alors sur l'orchestration, et sur rien d'autre.**
/// Avec une sonde de décision des deux côtés, un écart pourrait venir d'une
/// évaluation, d'un ordre de tri à ex æquo ou d'une lecture d'aperçu — et le
/// test dirait « les deux boucles divergent » en désignant la mauvaise cause.
fn script() -> Vec<HandDecision> {
    vec![
        // Main 1 : on garde les deux premiers dés et on relance les trois
        // autres. C'est le seul geste qui éprouve le chemin de relance, l'une
        // des six réimplémentations.
        HandDecision::Reroll(LockMask::new(vec![DieId(0), DieId(1)])),
        HandDecision::Submit(YahtzeeHand::SmallStraight),
        // Mains 2 à 4 : une figure par main, aucune deux fois. Une figure
        // resoumise est refusée par les deux boucles — correctement — et se
        // lirait comme une divergence.
        HandDecision::Submit(YahtzeeHand::Sixes),
        HandDecision::Submit(YahtzeeHand::Threes),
        HandDecision::Submit(YahtzeeHand::Chance),
    ]
}

/// Une figure soumise, et les figures que les dés formaient à ce moment-là.
/// C'est ce que `test_script_is_playable` relit pour dire si le script tient.
type Soumission = (YahtzeeHand, Vec<YahtzeeHand>);

/// Ce qu'une main laisse voir : les dés au moment de la décision, et le score
/// commis à la fin de la main — `None` quand aucun commit n'a été observé.
type Main = (Vec<u8>, Option<u64>);

/// La comparaison, **sous forme de fonction**.
///
/// Elle l'est pour que la contre-épreuve puisse l'appeler sur une suite
/// volontairement fausse : un fixture d'accord qui ne peut pas échouer ne
/// couvre aucun risque, et une égalité écrite en ligne ne s'éprouve pas.
///
/// **La suite porte les dés en plus des scores**, et diverge donc une main plus
/// tôt quand la cause est un flux : les dés partent en premier, le score suit.
fn accord(bevy: &[Main], harnais: &[Main]) -> Result<(), String> {
    if bevy.is_empty() || harnais.is_empty() {
        return Err(format!(
            "une suite est vide : bevy {} mains, harnais {} mains — \
             un accord entre deux suites vides n'est pas un accord",
            bevy.len(),
            harnais.len()
        ));
    }
    if bevy.len() != harnais.len() {
        return Err(format!(
            "longueurs différentes : bevy {} mains, harnais {} mains",
            bevy.len(),
            harnais.len()
        ));
    }
    for (rang, (gauche, droite)) in bevy.iter().zip(harnais).enumerate() {
        if gauche.0 != droite.0 {
            return Err(format!(
                "main {} : dés {:?} côté jeu, {:?} côté harnais",
                rang + 1,
                gauche.0,
                droite.0
            ));
        }
        if gauche.1 != droite.1 {
            return Err(format!(
                "main {} : score commis {:?} côté jeu, {:?} côté harnais",
                rang + 1,
                gauche.1,
                droite.1
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------- côté harnais

fn session_de_reference(seed: u64) -> RunSession {
    let deck = cup(CupId::Standard);
    RunSession {
        config: RunConfig::from_cup(&deck),
        ante: 1,
        blind_kind: BlindType::Small,
        gold: deck.starting_gold,
        cup_id: CupId::Standard,
        stake_level: 1,
        hand_levels: HandLevels::default(),
        rng: RunRng::from_seed(seed),
    }
}

/// La sonde scriptée : elle rend la décision suivante de la liste et **ne
/// consomme aucun flux**, ni celui de la run ni celui de l'instrument.
struct Scriptee {
    gestes: Vec<HandDecision>,
    rang: usize,
    vues: Rc<RefCell<Vec<Vec<u8>>>>,
    figures: Rc<RefCell<Vec<Soumission>>>,
}

impl Policy for Scriptee {
    fn name(&self) -> &'static str {
        "scriptee"
    }

    fn decide(&mut self, view: &HandView<'_>, _rng: &mut ChaCha8Rng) -> HandDecision {
        let des: Vec<u8> = view.dice.iter().map(|die| die.current_value).collect();
        let geste = self
            .gestes
            .get(self.rang)
            .cloned()
            .unwrap_or(HandDecision::Submit(YahtzeeHand::Chance));
        self.rang += 1;

        if let HandDecision::Submit(figure) = &geste {
            self.figures.borrow_mut().push((
                *figure,
                view.matches.iter().map(|trouve| trouve.hand).collect(),
            ));
            self.vues.borrow_mut().push(des);
        }
        // Une relance n'ouvre pas de main : elle en change les dés.

        geste
    }
}

/// **Le score commis se lit dans le journal du harnais, pas dans la vue.**
///
/// La vue porte le score courant *avant* la main : la dernière main d'une
/// manche n'a pas de main suivante, et son commit serait invisible. Le journal,
/// lui, publie chaque commit — c'est ce pour quoi il existe, et son format est
/// épinglé par les tests qui le livrent.
#[derive(Default)]
struct Greffier {
    cumuls: Vec<u64>,
}

impl sim_harness::trace::Observateur for Greffier {
    fn note<F: FnOnce() -> String>(&mut self, ligne: F) {
        let ligne = ligne();
        let Some(reste) = ligne.trim_start().strip_prefix("commit ") else {
            return;
        };
        if let Some(cumul) = reste
            .split_whitespace()
            .find_map(|mot| mot.strip_prefix("cumul="))
            && let Ok(valeur) = cumul.parse::<u64>()
        {
            self.cumuls.push(valeur);
        }
    }
}

/// Joue la manche côté harnais et rend la suite des mains.
fn manche_du_harnais(seed: u64) -> (Vec<Main>, Vec<Soumission>) {
    let vues = Rc::new(RefCell::new(Vec::new()));
    let figures = Rc::new(RefCell::new(Vec::new()));
    let mut politique = Scriptee {
        gestes: script(),
        rang: 0,
        vues: Rc::clone(&vues),
        figures: Rc::clone(&figures),
    };
    let mut achats = BudgetShopPolicy;
    let mut greffier = Greffier::default();
    let config = SimConfig {
        runs: 1,
        seed_base: seed,
        cups: vec![CupId::Standard],
        stakes: vec![1],
        policy: PolicyKind::GridAware,
        shop_policy: ShopPolicyKind::Budget,
        threads: 1,
    };
    let _ = sim_harness::run::simulate_with_obs(
        &config,
        seed,
        &mut politique,
        &mut achats,
        &mut greffier,
    );

    // Aucune troncature : la manche n'est pas franchie, le run s'arrête avec
    // elle, et les assertions de longueur des tests gardent la propriété.
    // Mesuré au banc : une troncature posée ici ne changeait rien, donc elle ne
    // gardait rien.
    let suite: Vec<Main> = vues
        .borrow()
        .iter()
        .cloned()
        .zip(greffier.cumuls.iter().copied().map(Some))
        .collect();
    let figures = figures.borrow().clone();
    (suite, figures)
}

// ---------------------------------------------------------------- côté jeu

/// Frappe une touche : une pression **et** son relâchement, en messages.
///
/// Le système d'entrées vide `just_pressed` en début de frame, donc une
/// pression posée à la main sur la ressource serait effacée avant l'`Update` ;
/// et sans relâchement la touche resterait enfoncée, rendant la **seconde**
/// frappe muette.
fn frapper(app: &mut App, code: KeyCode) {
    for etat in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput {
            key_code: code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: etat,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    }
}

const TOUCHES_DE_RANG: [KeyCode; 5] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
];

fn des_tries(app: &mut App) -> Vec<(DieId, u8)> {
    let mut requete = app.world_mut().query::<&Die>();
    let mut valeurs: Vec<(DieId, u8)> = requete
        .iter(app.world())
        .map(|die| (die.id, die.current_value))
        .collect();
    valeurs.sort_unstable_by_key(|(id, _)| *id);
    valeurs
}

/// Monte la boucle du jeu, sans écran et sans horloge réelle.
///
/// **`InputPlugin` est obligatoire** : la boucle se pilote au clavier, et le
/// plugin de la crate d'états le déclare en tête de son propre fichier — son
/// absence fait paniquer la construction des paramètres d'un système, sur un
/// message qui ne nomme ni le système ni la ressource.
///
/// **`TimePlugin` est désactivé, et `Time` posée à la main.** C'est la
/// correction que personne ne devine, et son absence est **verte** : l'horloge
/// réelle réécrit `Time` à chaque frame, l'avance posée par le test est
/// effacée, la file de score ne se vide jamais, le commit n'a pas lieu — et
/// une suite sans aucun score commis s'accorderait avec une autre suite sans
/// aucun score commis. Mesuré : avec l'horloge réelle, la file reste à cinq
/// paliers et le score commis vaut zéro, sans la moindre erreur.
fn app_de_reference(seed: u64, horloge_reelle: bool) -> App {
    let partie = session_de_reference(seed);
    let stock = RelicInventory::new(partie.config.relic_capacity);

    let mut app = App::new();
    if horloge_reelle {
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            InputPlugin,
            GameStatePlugin,
            JuicePlugin,
        ));
    } else {
        app.add_plugins((
            MinimalPlugins.build().disable::<TimePlugin>(),
            StatesPlugin,
            InputPlugin,
            GameStatePlugin,
            JuicePlugin,
        ));
        app.init_resource::<Time>();
    }

    app.insert_resource(partie);
    app.insert_resource(stock);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::InRun);
    app.update();
    app.world_mut()
        .resource_mut::<NextState<RunPhase>>()
        .set(RunPhase::Roll);
    app.update();
    app.update();
    app
}

/// Joue la manche côté jeu, en suivant le même script.
///
/// Rend la suite des mains **et** l'état de fin de manche. Les deux sortent
/// d'un seul pilotage : une seconde fonction qui rejouerait la manche pour en
/// lire la fin serait une recopie, et une recopie diverge au premier geste
/// ajouté au script.
fn manche_du_jeu(seed: u64, horloge_reelle: bool) -> (Vec<Main>, u8, u64) {
    let mut app = app_de_reference(seed, horloge_reelle);
    let mut suite: Vec<Main> = Vec::new();

    for geste in script() {
        let des: Vec<u8> = des_tries(&mut app).into_iter().map(|(_, v)| v).collect();
        match geste {
            HandDecision::Reroll(masque) => {
                let identifiants: Vec<DieId> =
                    des_tries(&mut app).into_iter().map(|(id, _)| id).collect();
                // Le verrouillage s'exprime par **touche de rang** : la touche
                // n vise le dé dont l'ordre d'affichage vaut n − 1, et cet
                // ordre suit le tri par identifiant posé à l'entrée en phase
                // de lancer. Le masque, lui, liste les dés **conservés**.
                for (position, id) in identifiants.iter().enumerate() {
                    if masque.contains(*id) {
                        frapper(&mut app, TOUCHES_DE_RANG[position]);
                    }
                }
                app.update();
                frapper(&mut app, KeyCode::Space);
                app.update();
                app.update();
            }
            HandDecision::Submit(figure) => {
                let manche = app.world().resource::<BlindContext>().clone();
                {
                    let mut main = app.world_mut().resource_mut::<HandContext>();
                    game_state::systems::input::select_hand(&mut main, &manche, figure);
                }
                app.update();
                frapper(&mut app, KeyCode::Enter);
                app.update();
                app.update();

                // La pause finale **n'est jamais accélérée** : le test
                // l'attend, il ne la contourne pas. La borne existe pour que
                // l'horloge réelle du montage fautif ne fasse pas boucler le
                // test à l'infini.
                let mut commis = None;
                for _ in 0..600 {
                    app.world_mut()
                        .resource_mut::<Time>()
                        .advance_by(Duration::from_millis(16));
                    app.update();
                    if app.world().resource::<ScoringStepQueue>().committed {
                        commis = Some(app.world().resource::<BlindContext>().current_score);
                        break;
                    }
                }
                suite.push((des, commis));

                // **Aucune transition n'est forcée ici.** Mesuré au banc :
                // retirer un retour explicite en phase de lancer ne change
                // rien, parce que le jeu y revient de lui-même après le commit.
                // Le forcer masquerait une régression de cette transition —
                // c'est-à-dire l'une des six réimplémentations que ce fixture
                // existe pour surveiller.
                app.update();
                app.update();
            }
        }
    }
    let manche = app.world().resource::<BlindContext>();
    (suite, manche.hands_remaining, manche.current_score)
}

// ------------------------------------------------------------------- tests

#[test]
fn test_harness_matches_bevy_loop() {
    let (harnais, _) = manche_du_harnais(GRAINE_DE_REFERENCE);
    let (jeu, _, _) = manche_du_jeu(GRAINE_DE_REFERENCE, false);

    // **Non vacuous avant toute comparaison.** Une suite vide s'accorde avec
    // une autre suite vide, et c'est le seul échec de ce fichier qui se
    // présenterait comme un succès.
    assert_eq!(
        jeu.len(),
        MAINS_ATTENDUES,
        "la boucle du jeu n'a pas joué les mains attendues : {jeu:?}"
    );
    assert_eq!(
        harnais.len(),
        MAINS_ATTENDUES,
        "le harnais n'a pas joué les mains attendues : {harnais:?}"
    );
    assert!(
        jeu.iter().all(|main| main.1.is_some()),
        "une main du jeu n'a rien commis : {jeu:?}"
    );

    if let Err(motif) = accord(&jeu, &harnais) {
        panic!("les deux boucles divergent — {motif}");
    }
}

#[test]
fn test_agreement_covers_a_full_blind() {
    let (harnais, _) = manche_du_harnais(GRAINE_DE_REFERENCE);
    let (jeu, restantes, score) = manche_du_jeu(GRAINE_DE_REFERENCE, false);

    // L'accord porte sur **toutes** les mains, la dernière comprise.
    assert_eq!(jeu.len(), MAINS_ATTENDUES);
    assert!(accord(&jeu, &harnais).is_ok());

    // Et l'état de fin de manche est le même : la manche est consommée jusqu'au
    // bout, pas seulement jusqu'à l'avant-dernière main.
    assert_eq!(restantes, 0, "la manche n'est pas allée à son terme");
    assert_eq!(
        Some(score),
        jeu.last().and_then(|main| main.1),
        "le score de fin de manche ne suit pas la dernière main commise"
    );
    assert_eq!(
        Some(score),
        harnais.last().and_then(|main| main.1),
        "le harnais ne finit pas la manche sur le même score"
    );
}

#[test]
fn test_agreement_detects_an_injected_divergence() {
    let (harnais, _) = manche_du_harnais(GRAINE_DE_REFERENCE);
    let (jeu, _, _) = manche_du_jeu(GRAINE_DE_REFERENCE, false);
    assert!(
        accord(&jeu, &harnais).is_ok(),
        "l'accord de départ doit tenir"
    );

    // **Une valeur perturbée suffit.** La comparaison est une fonction
    // précisément pour qu'on puisse l'appeler sur une suite fausse : un fixture
    // d'accord qui ne peut pas échouer ne couvre aucun risque.
    let mut fausse = harnais.clone();
    if let Some(main) = fausse.last_mut() {
        main.1 = Some(main.1.unwrap_or_default().saturating_add(1));
    }
    assert!(
        accord(&jeu, &fausse).is_err(),
        "un score faussé d'une unité n'est pas détecté"
    );

    let mut des_fausses = harnais.clone();
    if let Some(main) = des_fausses.first_mut() {
        main.0[0] = main.0[0] % 6 + 1;
    }
    assert!(
        accord(&jeu, &des_fausses).is_err(),
        "un dé faussé n'est pas détecté"
    );

    // **Les deux suites vides ne s'accordent pas.** C'est la propriété centrale
    // du comparateur, et elle n'était éprouvée par aucun test : mesuré au banc,
    // retirer ce refus laissait passer un accord entre deux néants.
    assert!(
        accord(&[], &[]).is_err(),
        "deux suites vides s'accordent : le fixture ne couvre plus rien"
    );
    assert!(
        accord(&jeu, &[]).is_err(),
        "une suite vide s'accorde avec une suite pleine"
    );

    // Et deux longueurs différentes se refusent avant toute comparaison
    // valeur par valeur : une manche écourtée d'un côté est une divergence.
    assert!(
        accord(&jeu[..jeu.len() - 1], &harnais).is_err(),
        "une manche écourtée s'accorde avec une manche complète"
    );

    // Et sur une autre graine : les deux suites ne sont pas égales par
    // construction.
    let (autre, _) = manche_du_harnais(GRAINE_DE_REFERENCE + 1);
    assert!(
        accord(&jeu, &autre).is_err(),
        "une graine différente rend le même résultat : la graine ne porte rien"
    );
}

#[test]
fn test_time_plugin_must_be_disabled() {
    // **Le seul piège du montage qui soit silencieux et vert.** Avec l'horloge
    // réelle, l'avance posée par le test est réécrite à chaque frame : la file
    // ne se vide jamais, aucun commit n'a lieu, et deux suites sans score
    // s'accorderaient parfaitement.
    //
    // L'omission de l'initialisation des états, elle, **n'est pas silencieuse**
    // en 0.19 : elle panique en nommant le plugin manquant. C'est ici que le
    // danger est, pas là.
    let (fautif, _, _) = manche_du_jeu(GRAINE_DE_REFERENCE, true);
    assert!(
        fautif.iter().any(|main| main.1.is_none()),
        "l'horloge réelle a laissé commettre : le piège a disparu, revois ce test"
    );

    let (harnais, _) = manche_du_harnais(GRAINE_DE_REFERENCE);
    assert!(
        accord(&fautif, &harnais).is_err(),
        "une suite sans commit s'accorde : le montage fautif passerait inaperçu"
    );
}

#[test]
fn test_scripted_policy_consumes_no_stream() {
    // Les quatre flux de la run **plus** celui de l'instrument. La comparaison
    // se fait sur des **tirages**, jamais sur des structures : le générateur de
    // run ne dérive pas l'égalité, délibérément.
    let mut run = RunRng::from_seed(GRAINE_DE_REFERENCE);
    let mut instrument = sim_harness::rng::SimRng::from_seed(GRAINE_DE_REFERENCE);

    let avant: Vec<u32> = [
        &mut run.dice,
        &mut run.shop,
        &mut run.boss,
        &mut run.relic_effects,
        &mut instrument.policy,
    ]
    .into_iter()
    .map(|flux| flux.clone().random_range(0..1_000_000))
    .collect();

    let mut politique = Scriptee {
        gestes: script(),
        rang: 0,
        vues: Rc::new(RefCell::new(Vec::new())),
        figures: Rc::new(RefCell::new(Vec::new())),
    };
    let session = session_de_reference(GRAINE_DE_REFERENCE);
    // La manche s'assemble par le harnais : `BlindContext` n'a pas de valeur
    // par défaut, et sa fabrique interne à la crate du moteur ne franchit pas
    // la frontière de crate.
    let manche = sim_harness::blind::blind_context(
        sim_harness::blind::blind_definition(1, BlindType::Small, CupId::Standard, 1, None),
        &session.config,
    );
    let vue = HandView {
        dice: &[],
        matches: &[],
        blind: &manche,
        rerolls_left: 2,
        hand_levels: &session.hand_levels,
        relics: &RelicInventory::new(session.config.relic_capacity),
    };
    let _ = politique.decide(&vue, &mut instrument.policy.clone());

    let apres: Vec<u32> = [
        &mut run.dice,
        &mut run.shop,
        &mut run.boss,
        &mut run.relic_effects,
        &mut instrument.policy,
    ]
    .into_iter()
    .map(|flux| flux.clone().random_range(0..1_000_000))
    .collect();

    assert_eq!(avant, apres, "la sonde scriptée a consommé un flux");
}

#[test]
fn test_script_is_playable() {
    // **Un script injouable échoue ici, avec son nom.** Une figure resoumise
    // est refusée par les deux boucles — correctement — et se lirait comme une
    // divergence d'orchestration si ce test ne la nommait pas.
    let (_, figures) = manche_du_harnais(GRAINE_DE_REFERENCE);
    assert_eq!(figures.len(), MAINS_ATTENDUES, "{figures:?}");

    let mut vues: Vec<YahtzeeHand> = Vec::new();
    for (figure, formees) in &figures {
        assert!(
            !vues.contains(figure),
            "{figure:?} est soumise deux fois : la seconde sera refusée"
        );
        assert!(
            formees.contains(figure),
            "{figure:?} n'est pas formée par les dés de la graine : {formees:?}"
        );
        vues.push(*figure);
    }
}

#[test]
fn test_reference_seed_is_a_named_constant() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/harness_agreement.rs"
    ))
    .expect("le fichier de test se relit");

    // **Le motif s'assemble, il ne s'épelle pas.** Épelé, il apparaîtrait dans
    // cette assertion même, et le compte vaudrait deux : le test se
    // déclencherait sur lui-même.
    let declaration = format!("{}: u64", "GRAINE_DE_REFERENCE");
    assert_eq!(
        source.matches(&declaration).count(),
        1,
        "la graine est déclarée plus d'une fois"
    );
    // **Aucune graine littérale ne double la constante.** Le contrôle porte sur
    // la valeur elle-même, tirée de la constante et cherchée dans un texte
    // débarrassé de ses séparateurs de milliers : elle ne doit apparaître
    // qu'une fois, à sa déclaration. Écrire les deux orthographes en clair
    // ferait échouer ce test sur lui-même — troisième fois dans ce fichier.
    let sans_separateurs: String = source.chars().filter(|lettre| *lettre != '_').collect();
    assert_eq!(
        sans_separateurs
            .matches(&GRAINE_DE_REFERENCE.to_string())
            .count(),
        1,
        "la valeur de la graine est écrite ailleurs qu'à sa déclaration"
    );
}

#[test]
fn test_commit_path_is_the_only_writer() {
    // **Ce ticket n'écrit aucun des trois motifs du commit** : il observe le
    // commit du jeu, il ne le simule pas. Il n'a donc aucune exclusion à
    // demander, et n'en ajoute aucune.
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/harness_agreement.rs"
    ))
    .expect("le fichier de test se relit");
    // Mêmes précautions : les trois motifs s'assemblent. Épelés, ils
    // apparaîtraient dans cette boucle et le test échouerait sur lui-même —
    // et, pire, il échouerait **pour la bonne raison apparente**.
    for (champ, acte) in [
        ("current_score", "saturating_add"),
        ("hands_remaining", "saturating_sub"),
        ("used_hands", "mark"),
    ] {
        let motif = format!("{champ}.{acte}");
        assert!(
            !source.contains(&motif),
            "le test crée un second site de commit : {motif}"
        );
    }

    // Et la garde du dépôt est là, **inchangée**. Elle est portée sur les
    // répertoires de production des deux crates de jeu : un test d'intégration
    // n'est pas dans sa portée, et aucune exclusion nominative n'existe ni
    // n'est nécessaire.
    let ci = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../.github/workflows/ci.yml"
    ))
    .expect("le fichier de CI se relit");
    assert!(
        ci.contains("commit dupliqué hors du juice"),
        "la garde du commit unique a disparu"
    );
    assert!(
        !ci.contains("!crates/ui_and_juice/src/queue.rs"),
        "une exclusion nominative est apparue sur la garde du commit"
    );
}

#[test]
fn test_dev_dependency_creates_no_cycle() {
    let manifeste = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sim_harness/Cargo.toml"
    ))
    .expect("le manifeste du harnais se relit");
    // **Les noms s'assemblent, ils ne s'épellent pas.** La garde du dépôt qui
    // interdit la dépendance inverse vers la boutique est ancrée sur le **nom
    // nu**, et porte sur le répertoire entier de cette crate, tests compris :
    // épelé ici, le nom ferait tomber la garde — mesuré, un échec du volet 1
    // déclenché par ce fichier. C'est le défaut le plus fréquent de ce corpus,
    // pris par l'autre bout ; la garde elle-même est signalée à l'audit.
    for (jeu, suffixe) in [("game", "state"), ("shop", "system"), ("ui_and", "juice")] {
        let crate_de_jeu = format!("{jeu}_{suffixe}");
        assert!(
            !manifeste.contains(&crate_de_jeu),
            "le harnais dépend de {crate_de_jeu} : le cycle est créé"
        );
    }

    let arbre = std::process::Command::new(env!("CARGO"))
        .args(["tree", "-p", "sim_harness", "--edges", "normal"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("cargo tree");
    let arbre = String::from_utf8_lossy(&arbre.stdout);
    // Non vacuous : un arbre vide passerait le filtre sans rien garder.
    assert!(
        arbre.contains("core_engine"),
        "l'arbre n'est pas lu : {arbre}"
    );
    assert!(
        !arbre.contains("bevy"),
        "une dépendance de développement est entrée dans l'arbre normal du harnais"
    );
}
