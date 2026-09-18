//! Audit de clôture de l'Étape 8 : TASK-109.
//!
//! **Ce fichier ne refait pas le travail des gardes.** Les neuf critères de fin d'étape et les
//! douze corrections v1 → v2 du document source sont déjà tenus, ticket par ticket, par des
//! gardes de CI ancrées sur un acte et par des tests nommés. Ce que rien ne tenait, c'est **leur
//! existence** : une garde retirée du workflow, un test renommé, et le critère qu'ils portaient
//! cesse d'être gardé sans qu'aucune commande échoue. La concordance ci-dessous dit, critère par
//! critère, qui le tient, et vérifie que chacun est encore là, une fois et une seule.
//!
//! Le ticket d'audit proposait des recherches textuelles sur des noms nus. Plusieurs mordaient
//! la prose qui explique l'interdit, ou du code légitime : elles ne sont pas recopiées, elles
//! sont mises en concordance avec la garde ancrée qui tient le même fait.
//!
//! Ni `App`, ni backend, ni son, ni outil externe : des fichiers lus depuis
//! `CARGO_MANIFEST_DIR`.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const CI: &str = ".github/workflows/ci.yml";
const CONTRACT: &str = "crates/audio_system/tests/audio_contract.rs";
const GLOSSARY_BLOCK: &str = "reports/glossaire-etape-8.md";

/// Ce qui tient un critère : des libellés de gardes du workflow, et des tests nommés
/// (`fichier`, `nom`).
struct Holding {
    what: &'static str,
    guards: &'static [&'static str],
    tests: &'static [(&'static str, &'static str)],
}

const CONCORDANCE: &[Holding] = &[
    Holding {
        what: "critère 1 : tâche 0 tranchée et consignée",
        guards: &[
            "TASK-95  l'addendum nomme la branche et la version",
            "TASK-95  trois critères, trois verdicts",
            "TASK-95  trois rectifications de l'ADR-006",
            "TASK-95  valeur estimée dans l'addendum",
            "TASK-97  la façade fuit hors de backend.rs",
        ],
        tests: &[],
    },
    Holding {
        what: "critère 2 : musique continue",
        guards: &[
            "TASK-100 marqueur de despawn lié à un état",
            "TASK-100 transition écrite par l'audio",
            "TASK-100 la survie s'écoute sur le son rendu",
        ],
        tests: &[
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_music_survives_round_end_to_roll",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_layers_created_once",
            ),
            (
                "crates/audio_system/tests/real_backend.rs",
                "test_music_survives_the_state_cycle",
            ),
        ],
    },
    Holding {
        what: "critère 2 bis : les quatre voies sont chargées",
        guards: &[
            "TASK-97  une voie nommée sans être chargée",
            "TASK-100 un seul site charge les couches",
            "TASK-100 quatre couches, par la constante",
            "TASK-100 quatre stems, dans l'ordre des index",
            "TASK-107 les couches chargées, lues sur le journal",
        ],
        tests: &[
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_four_stems_are_loaded_and_started",
            ),
            (
                "crates/audio_system/tests/assets_contract.rs",
                "test_the_game_loads_exactly_the_shipped_audio",
            ),
            (
                "crates/audio_system/tests/production_assets.rs",
                "test_production_assets_load_without_a_beep",
            ),
        ],
    },
    Holding {
        what: "critère 3 : couches justes et seuil unique",
        guards: &[
            "TASK-99  le seuil, sous son nom, et il vaut ce qu'il vaut",
            "TASK-99  le seuil n'est écrit qu'une fois",
            "TASK-99  une seule définition du seuil",
            "TASK-99  comparaison entière en u128",
            "TASK-99  flottant double dans la musique",
            "TASK-99  tension sur Boss ou dernière main",
        ],
        tests: &[
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_climax_threshold_is_75_percent",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_target_gains_are_pure",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_tension_on_boss_and_last_hand",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_shop_softens_the_mix",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_climax_holds_at_u64_extremes",
            ),
        ],
    },
    Holding {
        what: "critère 4 : modèle de hauteur unique",
        guards: &[
            "TASK-104 un seul modèle de hauteur",
            "TASK-104 le demi-ton tempéré",
            "TASK-104 une seule puissance dans pitch.rs",
            "TASK-104 formule de hauteur recopiée ailleurs",
            "TASK-104 modèle de hauteur linéaire",
            "TASK-106 puissance calculée dans la musique",
        ],
        tests: &[
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_pitch_semitone_formula",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_pitch_resets_on_scoring_entry",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_pitch_is_capped",
            ),
            (
                "crates/audio_system/tests/real_backend.rs",
                "test_the_capped_pitch_is_played",
            ),
        ],
    },
    Holding {
        what: "critère 5 : déterminisme",
        guards: &[
            "TASK-102 générateur de la run nommé par l'audio",
            "TASK-102 cinquième flux dans le générateur de la run",
            "TASK-102 quatre flux, toujours",
            "TASK-102 le test central du générateur",
            "TASK-102 générateur à l'entropie système",
            "TASK-102 l'audio sérialisé",
        ],
        tests: &[
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_audio_never_advances_run_rng",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_dice_roll_uses_six_variations",
            ),
        ],
    },
    Holding {
        what: "critère 6 : grille consommable audible",
        guards: &[
            "TASK-103 un bit nouvellement posé",
            "TASK-103 copie locale de la grille",
            "TASK-103 un seul lecteur du loquet de case",
            "TASK-103 son d'état transposé",
            "TASK-103 contexte de blind optionnel",
            "TASK-103 événement créé ou émis par l'audio",
        ],
        tests: &[
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_hand_consumed_sound_fires_once_per_case",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_two_hands_marked_in_one_frame_produce_two_sounds",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_hand_consumed_is_not_pitched",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_no_sound_outside_a_blind",
            ),
        ],
    },
    Holding {
        what: "critère 7 : frontière avec l'Étape 4",
        guards: &[
            "TASK-96  dépendance inverse vers la crate audio",
            "TASK-95  backend audio dans une crate amont",
            "TASK-105 après le dépileur, dans la même image",
            "TASK-105 lecteur placé dans un ensemble du juice",
            "TASK-105 ressource de la mise en scène lue par l'audio",
            "TASK-108 le son nommé sous la cible",
            "critère 4  ressource de son déclarée ou employée",
        ],
        tests: &[
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_reader_runs_after_the_drainer",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_juice_plugin_still_registers_no_audio_resource",
            ),
            (
                "crates/ui_and_juice/src/events.rs",
                "test_no_audio_resource_registered",
            ),
        ],
    },
    Holding {
        what: "critère 8 : bus et Cargo",
        guards: &[
            "TASK-98  une seule ressource de volumes",
            "TASK-98  trois champs, dans la ressource",
            "TASK-98  copie d'un volume hors de la ressource",
            "TASK-98  poussée gardée par is_changed",
            "TASK-98  formule entendue remise à la façade",
            "TASK-96  backend épinglé, sans ses défauts",
            "TASK-96  feature son de Bevy dans la crate audio",
            "TASK-108 lecteur audio de Bevy dans le binaire",
            "TASK-108 l'arbre complet sans la feature proscrite",
        ],
        tests: &[
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_bus_volume_multiplication",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_bus_gain_pushed_only_when_changed",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_volume_clamps_above_one",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_no_symphonia_vorbis_in_tree",
            ),
            (
                "crates/audio_system/tests/audio_contract.rs",
                "test_no_symphonia_vorbis_in_workspace_tree",
            ),
            (
                "crates/audio_system/tests/assets_contract.rs",
                "test_every_audio_asset_has_a_credit",
            ),
        ],
    },
    Holding {
        what: "critère 9 : check-list 0.19",
        guards: &[
            "TASK-97  ressource porteuse, Resource seule",
            "TASK-98  Default ou Component dérivés sur la ressource",
            "TASK-100 ressource musicale, Resource seule",
            "TASK-102 la banque, Resource seule",
            "TASK-104 trois champs, Resource seule",
            "TASK-102 méthodes disparues de rand",
            "TASK-105 combinateurs supprimés en 0.19",
            "TASK-100 méthode disparue du minuteur",
            "TASK-109 énumération d'états de la v1",
        ],
        tests: &[],
    },
    Holding {
        what: "budget WASM, son compris",
        guards: &[
            "TASK-108 le plancher du coût du son",
            "TASK-108 référence écrite par un script",
            "TASK-107 le plafond du son, écrit une fois",
            "TASK-108 la cible monte le son, sur son backend réel",
        ],
        tests: &[
            (
                "crates/wasm_size/tests/boot.rs",
                "test_the_measured_game_boots",
            ),
            (
                "crates/audio_system/tests/assets_contract.rs",
                "test_audio_fits_the_wasm_budget",
            ),
        ],
    },
    Holding {
        what: "v1 → v2, 1 : synchronisation des stems",
        guards: &[
            "TASK-97  couches sous un même horodatage",
            "TASK-97  lecteur créé en pause",
        ],
        tests: &[(
            "crates/audio_system/tests/real_backend.rs",
            "test_four_layers_start_in_phase_and_stay",
        )],
    },
    Holding {
        what: "v1 → v2, 2 : réverbération sans DSP maison",
        guards: &[
            "TASK-97  réverbération en parallèle du bus",
            "TASK-96  le seul nœud de réverbération",
            "TASK-95  nœud audio écrit à la main dans le banc",
        ],
        tests: &[(
            "crates/audio_system/tests/real_backend.rs",
            "test_only_the_reverb_routing_leaves_a_tail",
        )],
    },
    Holding {
        what: "v1 → v2, 3 : backend OGG",
        guards: &["TASK-96  feature son de Bevy dans la crate audio"],
        tests: &[(
            "crates/audio_system/tests/audio_contract.rs",
            "test_no_symphonia_vorbis_in_workspace_tree",
        )],
    },
    Holding {
        what: "v1 → v2, 4 : features Cargo explicites",
        guards: &[
            "TASK-96  backend épinglé, sans ses défauts",
            "TASK-108 lecteur audio de Bevy dans le binaire",
            "TASK-108 feature 3D dans le binaire",
        ],
        tests: &[],
    },
    Holding {
        what: "v1 → v2, 5 : seuil du Climax à 75 %",
        guards: &["TASK-99  le seuil n'est écrit qu'une fois"],
        tests: &[(
            "crates/audio_system/tests/audio_contract.rs",
            "test_climax_threshold_is_75_percent",
        )],
    },
    Holding {
        what: "v1 → v2, 6 : demi-ton tempéré, plus de +0,05 linéaire",
        guards: &[
            "TASK-104 modèle de hauteur linéaire",
            "TASK-104 reset à l'entrée du décompte",
        ],
        tests: &[(
            "crates/audio_system/tests/audio_contract.rs",
            "test_pitch_semitone_formula",
        )],
    },
    Holding {
        what: "v1 → v2, 7 : une seule ressource de SFX",
        guards: &[
            "TASK-102 une seule banque de sons",
            "critère 4  ressource de son déclarée ou employée",
        ],
        tests: &[(
            "crates/audio_system/tests/audio_contract.rs",
            "test_juice_plugin_still_registers_no_audio_resource",
        )],
    },
    Holding {
        what: "v1 → v2, 8 : machine à états canonique",
        guards: &[
            "TASK-99  signature pure de target_gains",
            "TASK-109 énumération d'états de la v1",
        ],
        tests: &[],
    },
    Holding {
        what: "v1 → v2, 9 : `StepSource::Die`, jamais l'ancien nom",
        guards: &[
            "critère 10 noms proscrits",
            "TASK-105 le tic de jeton, une fois",
        ],
        tests: &[(
            "crates/audio_system/tests/audio_contract.rs",
            "test_timbre_matches_step_source",
        )],
    },
    Holding {
        what: "v1 → v2, 10 : le Climax monte au commit",
        guards: &["TASK-99  total anticipé par l'audio"],
        tests: &[(
            "crates/audio_system/tests/audio_contract.rs",
            "test_climax_threshold_is_75_percent",
        )],
    },
    Holding {
        what: "v1 → v2, 11 : signal sonore de la grille",
        guards: &["TASK-103 un bit nouvellement posé"],
        tests: &[(
            "crates/audio_system/tests/audio_contract.rs",
            "test_hand_consumed_sound_fires_once_per_case",
        )],
    },
    Holding {
        what: "v1 → v2, 12 : générateur local, jamais un flux de la run",
        guards: &[
            "TASK-102 générateur de la run nommé par l'audio",
            "TASK-102 générateur privé",
        ],
        tests: &[(
            "crates/audio_system/tests/audio_contract.rs",
            "test_audio_never_advances_run_rng",
        )],
    },
];

/// Les douze tests que le document source impose, par leur nom exact.
const IMPOSED_TESTS: [&str; 12] = [
    "test_climax_threshold_is_75_percent",
    "test_target_gains_are_pure",
    "test_tension_on_boss_and_last_hand",
    "test_music_survives_round_end_to_roll",
    "test_bus_volume_multiplication",
    "test_audio_never_advances_run_rng",
    "test_dice_roll_uses_six_variations",
    "test_hand_consumed_sound_fires_once_per_case",
    "test_pitch_semitone_formula",
    "test_pitch_resets_on_scoring_entry",
    "test_pitch_is_capped",
    "test_no_symphonia_vorbis_in_tree",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    fs::read_to_string(root().join(relative)).unwrap_or_else(|e| panic!("{relative} : {e}"))
}

/// Les libellés des gardes du workflow, avec leur nombre d'occurrences : le premier texte entre
/// guillemets d'une ligne qui commence par `require`, `forbid` ou `count`.
fn guard_labels() -> BTreeMap<String, usize> {
    let mut labels = BTreeMap::new();
    for line in read(CI).lines() {
        let line = line.trim_start();
        let is_guard = ["require ", "forbid ", "count "]
            .iter()
            .any(|verb| line.starts_with(verb));
        if let (true, Some(label)) = (is_guard, line.split('"').nth(1)) {
            *labels.entry(label.to_string()).or_insert(0) += 1;
        }
    }
    labels
}

/// Combien de fois `fn name() {` ouvre une ligne du fichier.
fn declarations(file: &str, name: &str) -> usize {
    let opening = format!("fn {name}() {{");
    read(file)
        .lines()
        .filter(|line| line.trim_start() == opening)
        .count()
}

#[test]
fn test_every_criterion_keeps_its_guardians() {
    let labels = guard_labels();
    assert!(
        labels.len() > 500,
        "le workflow n'est pas lu : {} gardes",
        labels.len()
    );
    for holding in CONCORDANCE {
        assert!(
            !holding.guards.is_empty() || !holding.tests.is_empty(),
            "« {} » n'est tenu par rien",
            holding.what
        );
        for guard in holding.guards {
            assert_eq!(
                labels.get(*guard).copied().unwrap_or(0),
                1,
                "« {} » : la garde « {guard} » n'est pas dans le workflow une fois et une seule",
                holding.what
            );
        }
        for (file, name) in holding.tests {
            assert_eq!(
                declarations(file, name),
                1,
                "« {} » : le test `{name}` n'est pas déclaré une fois dans {file}",
                holding.what
            );
        }
    }
}

/// Les neuf critères du document source (le deuxième et le huitième ont un bis dans le ticket,
/// le second seul a ses propres gardiens), le budget, et les douze lignes v1 → v2, de 1 à 12,
/// sans trou : une ligne sans contrepartie est une correction qui n'a pas été appliquée.
#[test]
fn test_nine_criteria_and_twelve_lines_are_all_held() {
    let titles: Vec<&str> = CONCORDANCE.iter().map(|holding| holding.what).collect();
    for criterion in 1..=9 {
        let prefix = format!("critère {criterion} :");
        assert_eq!(
            titles
                .iter()
                .filter(|title| title.starts_with(&prefix))
                .count(),
            1,
            "critère {criterion}"
        );
    }
    for line in 1..=12 {
        let prefix = format!("v1 → v2, {line} :");
        assert_eq!(
            titles
                .iter()
                .filter(|title| title.starts_with(&prefix))
                .count(),
            1,
            "ligne v1 → v2 numéro {line}"
        );
    }
    let distinct: BTreeSet<&str> = titles.iter().copied().collect();
    assert_eq!(distinct.len(), titles.len(), "un titre est répété");
}

/// **Un `cargo test --workspace` vert ne prouve pas qu'un test nommé existe** : un test absent
/// ne fait échouer aucune commande.
#[test]
fn test_the_twelve_imposed_tests_exist() {
    for name in IMPOSED_TESTS {
        assert_eq!(declarations(CONTRACT, name), 1, "`{name}`");
    }
}

// ------------------------------------------------------------------ le glossaire

/// Les identifiants que l'étape déclare hors de la crate audio, et où.
const DECLARED_ELSEWHERE: [(&str, &str); 2] = [
    ("blind_is_beaten", "crates/core_engine/src/blinds/mod.rs"),
    ("add_game_plugins", "crates/wasm_size/src/lib.rs"),
];

/// Déjà au glossaire avant cette étape de clôture : le bloc les nomme en prose, sans les redire
/// dans sa table.
const ALREADY_REGISTERED: [&str; 4] = [
    "PitchScaleTracker",
    "SoundEffectBank",
    "AdaptiveMusicManager",
    "AudioBusVolumes",
];

/// Les déclarations publiques de premier niveau d'un fichier source : `pub struct Nom`,
/// `pub fn nom(`… en colonne zéro. Les méthodes, indentées, n'en sont pas.
fn public_items(source: &str) -> Vec<String> {
    let mut items = Vec::new();
    for line in source.lines() {
        for kind in ["struct", "enum", "trait", "fn", "const", "type"] {
            if let Some(rest) = line.strip_prefix(&format!("pub {kind} ")) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                items.push(name);
            }
        }
    }
    items
}

/// Les identifiants de la table du § 2.6 du bloc : première cellule, éclatée sur ` / `.
fn block_identifiers(block: &str) -> Vec<String> {
    let table = block
        .split_once("## § 2.6")
        .expect("le bloc a son § 2.6")
        .1
        .split("\n## ")
        .next()
        .expect("table");
    table
        .lines()
        .filter(|line| line.starts_with("| `"))
        .filter_map(|line| line.split('|').nth(1))
        .flat_map(|cell| cell.split(" / "))
        .map(|name| {
            name.trim()
                .trim_matches('`')
                .trim_end_matches("()")
                .to_string()
        })
        .collect()
}

/// **Dans les deux sens.** Chaque déclaration publique de la crate audio est au bloc, et chaque
/// identifiant du bloc est déclaré une fois, et une seule, là où le bloc le dit.
#[test]
fn test_glossary_block_matches_the_code_both_ways() {
    let block = read(GLOSSARY_BLOCK);
    let in_block = block_identifiers(&block);
    let distinct: BTreeSet<&String> = in_block.iter().collect();
    assert_eq!(
        distinct.len(),
        in_block.len(),
        "un identifiant est au bloc deux fois"
    );
    assert!(in_block.len() >= 30, "le bloc n'est pas lu : {in_block:?}");

    let mut in_code = Vec::new();
    for module in ["lib", "backend", "bus", "music", "pitch", "sfx"] {
        in_code.extend(public_items(&read(&format!(
            "crates/audio_system/src/{module}.rs"
        ))));
    }
    let declared: BTreeSet<&String> = in_code.iter().collect();
    assert_eq!(
        declared.len(),
        in_code.len(),
        "une déclaration publique est répétée"
    );

    for name in &in_code {
        let known = in_block.contains(name) || ALREADY_REGISTERED.contains(&name.as_str());
        assert!(
            known,
            "`{name}` est public dans la crate audio et absent du bloc de glossaire"
        );
    }
    for name in ALREADY_REGISTERED {
        assert!(
            in_code.iter().any(|item| item == name),
            "`{name}` a quitté la crate"
        );
        assert!(
            block.contains(&format!("`{name}`")),
            "le bloc ne nomme pas `{name}`"
        );
    }
    for name in &in_block {
        let elsewhere = DECLARED_ELSEWHERE.iter().find(|(item, _)| item == name);
        match elsewhere {
            Some((_, file)) => assert_eq!(
                public_items(&read(file))
                    .iter()
                    .filter(|item| *item == name)
                    .count(),
                1,
                "`{name}` n'est pas déclaré une fois dans {file}"
            ),
            None => assert!(
                in_code.contains(name),
                "`{name}` est au bloc de glossaire et n'est pas déclaré dans la crate audio"
            ),
        }
    }
    // Les deux types que le ticket d'audit nomme, sous leur forme.
    for facade in ["AudioClip", "LayerHandle"] {
        assert!(
            block.contains(&format!("| `{facade}` | newtype de façade | 8 |")),
            "`{facade}` n'est pas au registre sous sa forme"
        );
    }
}
