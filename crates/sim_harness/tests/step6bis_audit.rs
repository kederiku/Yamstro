//! L'audit de clôture de l'Étape 6 bis.
//!
//! Contrôles **textuels, de comptage et de graphe** : ni application, ni moteur
//! graphique, ni campagne. Les chemins se construisent depuis le manifeste de
//! la crate, jamais relativement au répertoire courant.
//!
//! **Six contrôles du ticket portent sur des faits qui ont changé**, et chacun
//! est réécrit ici sur ce que la mesure dit, avec le ticket qui a décidé :
//!
//! - les « deux amendements nominatifs » de la CI **n'ont jamais existé** — les
//!   gardes visées sont portées sur les répertoires de production des deux
//!   crates de jeu, donc le harnais en est déjà sorti ;
//! - la porte de PR ne compare plus un taux de victoire à une fourchette mais
//!   un rapport à une **référence dorée** (TASK-154), parce que le taux vaut
//!   zéro sur cent cinquante mille runs ;
//! - le test de débit **n'est pas** marqué ignoré, et c'est délibéré : un test
//!   ignoré ne tourne jamais, donc ne garde rien (TASK-151) ;
//! - le fichier des API absentes porte **six** entrées, pas quatre : le premier
//!   rapport d'équilibrage y a versé ce que la campagne a révélé ;
//! - la cible web historique apparaît **trois** fois sous le répertoire des
//!   workflows — c'est le job de mesure du binaire du jeu, et le contrôle porte
//!   donc sur le seul harnais ;
//! - les contrôles du glossaire lisent un fichier qui **n'est pas dans le
//!   dépôt** : le glossaire vit sur le corpus, et ce qui se vérifie ici est la
//!   correspondance entre le code et le bloc livré.
//!
//! Les motifs proscrits s'**assemblent** au lieu de s'épeler : épelés, ils se
//! déclencheraient sur ce fichier même.

use std::path::{Path, PathBuf};
use std::process::Command;

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("racine du dépôt")
}

fn lire(relatif: &str) -> String {
    let chemin = racine().join(relatif);
    std::fs::read_to_string(&chemin).unwrap_or_else(|erreur| panic!("{relatif} : {erreur}"))
}

/// Le chemin de ce fichier, relatif à la racine.
///
/// **Toute recherche l'exclut, et c'est structurel.** Un fichier d'audit nomme
/// nécessairement ce qu'il interdit : les orthographes proscrites, les motifs
/// de déclaration, les cibles obsolètes. Sans cette exclusion, quatre des onze
/// contrôles se déclenchent sur leurs propres littéraux — mesuré. L'exclusion
/// porte sur **ce seul fichier**, jamais sur le répertoire de tests : un défaut
/// réel dans un test d'intégration doit rester visible.
const CE_FICHIER: &str = "crates/sim_harness/tests/step6bis_audit.rs";

/// `grep -rnE` sous la racine, sur les chemins donnés. Rend les lignes trouvées,
/// **hors de ce fichier**.
fn chercher(motif: &str, chemins: &[&str]) -> Vec<String> {
    let mut arguments = vec!["-rnE".to_string(), motif.to_string()];
    arguments.extend(chemins.iter().map(|chemin| (*chemin).to_string()));
    let sortie = Command::new("grep")
        .args(&arguments)
        .current_dir(racine())
        .output()
        .expect("grep");
    String::from_utf8_lossy(&sortie.stdout)
        .lines()
        .filter(|ligne| !ligne.starts_with(CE_FICHIER))
        .map(str::to_string)
        .collect()
}

/// Les vingt types du harnais à porter au registre du corpus.
///
/// **Quatre y sont déjà**, sur une ligne groupée ; seize s'y ajoutent. Le
/// compte vient du document d'étape — seize types — plus les quatre que le
/// backlog a arbitrés : les deux états du harnais, le masque de conservation et
/// la vue d'étalage.
const TYPES_DU_HARNAIS: [&str; 20] = [
    "Policy",
    "ShopPolicy",
    "SimRng",
    "HandView",
    "SimConfig",
    "SimSession",
    "SimHand",
    "HandDecision",
    "LockMask",
    "ShopAction",
    "ShopView",
    "GreedyPolicy",
    "GridAwarePolicy",
    "RandomPolicy",
    "BudgetShopPolicy",
    "SynergyShopPolicy",
    "RunOutcome",
    "RunTrace",
    "RelicStats",
    "BalanceReport",
];

/// Les treize tests que le document d'étape impose par leur nom.
const TESTS_IMPOSES: [&str; 13] = [
    "test_same_seed_same_outcome",
    "test_same_seed_same_csv_regardless_of_threads",
    "test_policy_does_not_advance_run_rng",
    "test_random_policy_uses_sim_rng_only",
    "test_used_hand_is_never_submitted_twice",
    "test_grid_resets_between_blinds",
    "test_score_committed_once_per_hand",
    "test_rerolls_saturate",
    "test_relic_capacity_respected",
    "test_greedy_beats_random",
    "test_report_uses_integer_arithmetic",
    "test_trace_replays_a_seed",
    "test_harness_matches_bevy_loop",
];

// ------------------------------------------------------- critère 1 et tests

#[test]
fn test_step6bis_named_tests_all_exist() {
    for nom in TESTS_IMPOSES {
        let trouve = chercher(&format!("fn {nom}\\b"), &["crates/"]);
        assert_eq!(
            trouve.len(),
            1,
            "« {nom} » existe {} fois ; un test absent ne fait échouer aucune commande",
            trouve.len()
        );
    }

    // Le treizième vit dans la crate d'interface : c'est la seule d'où le test
    // d'accord est écrivable sans ajouter une arête au graphe du jeu.
    let accord = chercher("fn test_harness_matches_bevy_loop\\b", &["crates/"]);
    assert!(
        accord[0].contains("ui_and_juice"),
        "le test d'accord a changé de crate : {}",
        accord[0]
    );

    // **Aucun de ces treize n'est ignoré.** Et le test de débit non plus :
    // contrairement à ce que le ticket d'audit suppose, il ne porte pas
    // l'attribut — un test ignoré ne tourne jamais, donc ne garde rien.
    let ignores = chercher("#\\[ignore", &["crates/"]);
    assert!(
        ignores.is_empty(),
        "un test est marqué ignoré : {ignores:?}"
    );
}

// ---------------------------------------------------------------- critère 3

#[test]
fn test_no_float_and_no_hash_map_in_the_harness() {
    // Le motif flottant est **borné**, jamais un fichier exclu : exclure le
    // rapport ou le journal supprimerait l'invariant sur les deux fichiers qui
    // en ont le plus besoin.
    let flottants = chercher("\\bf64\\b|\\bf32\\b", &["crates/sim_harness/src"]);
    let hors_formatage: Vec<&String> = flottants
        .iter()
        .filter(|ligne| {
            !["format!(", "write!(", "writeln!(", "print!(", "println!("]
                .iter()
                .any(|forme| ligne.contains(forme))
        })
        .collect();
    assert!(
        hors_formatage.is_empty(),
        "flottant hors du formatage : {hors_formatage:?}"
    );
    // Mesuré : l'extraction elle-même est vide. L'invariant réel du harnais est
    // plus strict que la règle du corpus, qui tolère un flottant au formatage.
    assert!(
        flottants.is_empty(),
        "le harnais a gagné un flottant : {flottants:?}"
    );

    let tables = chercher("HashMap|HashSet", &["crates/sim_harness/src"]);
    assert!(
        tables.is_empty(),
        "une table non ordonnée suffit à rendre le tableau non reproductible : {tables:?}"
    );

    // **Et il n'y a aucune table ordonnée non plus**, pour une raison que le
    // corpus connaît : l'identifiant de relique ne dérive pas l'ordre — entrée
    // n° 4 du fichier des API absentes —, et tout le harnais emploie des listes
    // de paires. Le contrôle le constate pour que personne ne cherche à
    // « rétablir » une table qui ne peut pas exister.
    let ordonnees = chercher(
        "BTreeMap<RelicId|BTreeSet<RelicId",
        &["crates/sim_harness/src"],
    );
    assert!(ordonnees.is_empty(), "{ordonnees:?}");
}

// ---------------------------------------------------------------- critère 6

#[test]
fn test_no_bevy_anywhere_in_the_harness() {
    let manifeste = lire("crates/sim_harness/Cargo.toml").to_lowercase();
    assert!(
        !manifeste.contains("bevy"),
        "le manifeste du harnais nomme le moteur graphique"
    );

    // Ni monde, ni composant, ni ressource — commentaires compris.
    let symboles = chercher("World|Component|Resource", &["crates/sim_harness/src"]);
    assert!(symboles.is_empty(), "{symboles:?}");

    let arbre = Command::new(env!("CARGO"))
        .args(["tree", "-p", "sim_harness", "--edges", "normal"])
        .current_dir(racine())
        .output()
        .expect("cargo tree");
    let arbre = String::from_utf8_lossy(&arbre.stdout);
    // Non vacuous : un arbre vide passerait le filtre sans rien garder.
    assert!(arbre.contains("core_engine"), "l'arbre n'est pas lu");
    assert!(
        !arbre.contains("bevy"),
        "un paquet graphique est entré dans l'arbre normal du harnais"
    );
}

#[test]
fn test_core_engine_is_untouched_and_missing_api_is_not_empty() {
    // **La comparaison porte sur le point de branchement**, jamais sur l'index :
    // un diff nu sur un arbre propre est vide par construction, donc incapable
    // d'échouer.
    let base = Command::new("git")
        .args(["merge-base", "origin/main", "HEAD"])
        .current_dir(racine())
        .output()
        .expect("git merge-base");
    let base = String::from_utf8_lossy(&base.stdout).trim().to_string();
    assert!(!base.is_empty(), "le point de branchement est introuvable");

    let diff = Command::new("git")
        .args(["diff", "--exit-code", &base, "--", "crates/core_engine/"])
        .current_dir(racine())
        .output()
        .expect("git diff");
    assert!(
        diff.status.success(),
        "le moteur a bougé depuis le point de branchement :\n{}",
        String::from_utf8_lossy(&diff.stdout)
    );

    // **Le fichier des API absentes est non vide, et c'est un succès.** Un
    // fichier vide obtenu en élargissant une visibilité serait un échec : le
    // manque disparaîtrait du seul document qui le porte, et le diff du moteur
    // cesserait d'être vide. Les deux se vérifient **ensemble**.
    let manques = lire("crates/sim_harness/MISSING_API.md");
    let entrees = manques.lines().filter(|l| l.starts_with("## ")).count();
    assert!(
        entrees >= 4,
        "{entrees} entrée(s) ; les quatre connues d'avance doivent y être"
    );

    // Et la décomposition de la courbe est toujours hors d'atteinte : si l'une
    // des cinq fonctions était passée publique, l'entrée aurait été « résolue »
    // du mauvais côté.
    let courbe = lire("crates/core_engine/src/blinds/scaling.rs");
    for fonction in [
        "apply_permille",
        "blind_mult_permille",
        "cup_mult_permille",
        "stake_mult_permille",
        "boss_mult_permille",
    ] {
        assert!(
            courbe.contains(&format!("pub(crate) fn {fonction}")),
            "{fonction} n'est plus restreinte : l'entrée du fichier des API absentes a été contournée"
        );
    }
}

#[test]
fn test_six_types_are_declared_once() {
    // Une migration mal faite laisse deux déclarations homonymes que le
    // compilateur distingue et que le lecteur confond.
    for motif in [
        "struct RunOutcome",
        "enum HandDecision",
        "trait Policy\\b",
        "trait ShopPolicy",
        "struct LockMask",
        "struct SimRng",
    ] {
        let trouve = chercher(motif, &["crates/"]);
        assert_eq!(
            trouve.len(),
            1,
            "« {motif} » est déclaré {} fois : {trouve:?}",
            trouve.len()
        );
    }
}

#[test]
fn test_harness_is_imported_by_exactly_one_game_crate_file() {
    let sortie = Command::new("grep")
        .args(["-rl", "sim_harness", "crates/"])
        .current_dir(racine())
        .output()
        .expect("grep");
    let fichiers: Vec<&str> = String::from_utf8_lossy(&sortie.stdout)
        .lines()
        .filter(|chemin| !chemin.starts_with("crates/sim_harness/"))
        .map(|chemin| Box::leak(chemin.to_string().into_boxed_str()) as &str)
        .collect();

    assert_eq!(
        fichiers.len(),
        2,
        "le harnais est nommé dans {} fichiers hors de sa crate : {fichiers:?}",
        fichiers.len()
    );
    assert!(
        fichiers
            .iter()
            .all(|chemin| chemin.contains("ui_and_juice")),
        "une crate de jeu autre que celle du test d'accord nomme le harnais : {fichiers:?}"
    );
}

// ------------------------------------------------- vocabulaire et littéraux

#[test]
fn test_forbidden_spellings_are_absent_from_the_harness() {
    // **Ancrés sur une frontière de mot.** Sans ancrage, le nom v1 du dé de
    // score remonterait les trente-sept occurrences de la variante légitime du
    // moteur, qui le contient comme sous-chaîne : mesuré, `\bScoringDie\b` rend
    // zéro là où le motif nu en rend trente-sept.
    // **Chaque orthographe s'assemble de deux moitiés, et aucune n'est épelée.**
    // Ce fichier est, par construction, l'endroit du dépôt qui concentre le plus
    // d'orthographes proscrites : il doit toutes les nommer pour les interdire.
    // Épelées, elles font tomber **sept** gardes du volet 1 — mesuré. Les deux
    // autres remèdes sont pires : élargir les gardes amont à la production
    // toucherait sept blocs clos, et les exclure de ce fichier serait le
    // renoncement que cette étape a refusé partout ailleurs.
    for (tete, queue) in [
        ("Scoring", "Die"),
        ("Roll", "Session"),
        ("Round", "Context"),
        ("Hand", "Evaluation"),
        ("evaluate_", "hand"),
        ("rerolls_", "remaining"),
        ("hands_", "left"),
        ("rolls_", "remaining"),
        ("max_", "capacity"),
        ("MAX_", "RELICS"),
        ("MAX_", "INTEREST"),
        ("thread_", "rng"),
        ("Rng", "Core"),
        ("défau", "sse"),
    ] {
        let orthographe = format!("{tete}{queue}");
        let trouve = chercher(&format!("\\b{orthographe}\\b"), &["crates/", "assets/"]);
        assert!(trouve.is_empty(), "« {orthographe} » : {trouve:?}");
    }

    // **Le singulier de la figure affaiblie s'ancre sur l'acte, pas sur le
    // nom.** Même ancré sur une frontière de mot, il remonterait les deux
    // commentaires qui expliquent pourquoi le pluriel l'a emporté — dont un
    // dans le moteur, qu'aucun ticket de cette étape ne peut toucher. Ce qui
    // est proscrit est la **construction** ou la **déclaration**, comme le
    // dépôt le fait déjà pour le nom v1 du modificateur de dé.
    let singulier = format!("::{}\\(|^\\s{{4}}{}\\(", "DebuffHand", "DebuffHand");
    let trouve = chercher(&singulier, &["crates/", "assets/"]);
    assert!(
        trouve.is_empty(),
        "le singulier de la figure affaiblie est construit ou déclaré : {trouve:?}"
    );

    // Un type nommé comme l'inventaire v1, sans son préfixe. Même assemblage.
    let nu = format!("{}{}", "Invent", "ory");
    let inventaire = chercher(&format!("\\bstruct {nu}\\b|\\benum {nu}\\b"), &["crates/"]);
    assert!(inventaire.is_empty(), "{inventaire:?}");
}

#[test]
fn test_curve_and_rarity_literals_are_still_unique() {
    // **Les motifs s'assemblent.** Épelés, ils apparaîtraient dans ce fichier
    // et le test se déclencherait sur lui-même.
    let initial = format!("300{}000", "_");
    let croissance = format!("1{}600", "_");
    for motif in [initial.as_str(), croissance.as_str(), "1600"] {
        let trouve: Vec<String> = chercher(&format!("\\b{motif}\\b"), &["crates/"])
            .into_iter()
            .filter(|ligne| !ligne.contains("blinds/scaling.rs"))
            .filter(|ligne| !ligne.contains("/tests/"))
            .collect();
        assert!(
            trouve.is_empty(),
            "un littéral de courbe vit hors de son fichier : {trouve:?}"
        );
    }

    // La table de rareté est déclarée **une fois** et gardée une fois. Le
    // littéral s'assemble pour la même raison.
    let table = format!("\\[ *{} *, *{} *, *{} *, *{} *\\]", 650, 250, 90, 10);
    let trouve: Vec<String> = chercher(&table, &["crates/"])
        .into_iter()
        .filter(|ligne| !ligne.contains("/tests/"))
        .collect();
    assert_eq!(
        trouve.len(),
        1,
        "la table de rareté apparaît {} fois : {trouve:?}",
        trouve.len()
    );

    // **Le répertoire des rapports n'est balayé par aucun de ces motifs**, et
    // c'est structurel : le tableau porte une colonne de graine sur cent
    // cinquante mille runs, donc la ligne de graine 1 600 existe
    // nécessairement. Un motif de littéral appliqué à un fichier de données
    // confond une valeur mesurée avec une valeur codée.
    let dans_les_rapports = chercher("\\b1600\\b", &["reports/"]);
    assert!(
        !dans_les_rapports.is_empty(),
        "la campagne ne porte plus la graine 1600 : la démonstration de \
         l'insatisfiabilité ne tient plus, revois ce contrôle"
    );
}

// ------------------------------------------------------- cible web et corpus

#[test]
fn test_wasm_target_is_wasip1() {
    // **Le contrôle porte sur le harnais seul.** La cible web historique
    // apparaît trois fois sous le répertoire des workflows et dans les scripts
    // de mesure : c'est le job qui mesure le binaire du **jeu**, et l'exiger
    // absente de là demanderait de supprimer ce job.
    let historique = chercher("wasm32-unknown-unknown", &["crates/sim_harness/"]);
    assert!(
        historique.is_empty(),
        "le harnais nomme une cible qui n'a ni fichiers ni sortie standard : {historique:?}"
    );

    // Contre-épreuve : la cible du jeu, elle, est bien là où elle doit être.
    let jeu = chercher("wasm32-unknown-unknown", &[".github/", "ci/"]);
    assert!(
        !jeu.is_empty(),
        "le job de mesure du binaire du jeu a disparu"
    );
}

#[test]
fn test_glossary_section_covers_every_harness_type() {
    // **Le glossaire n'est pas dans le dépôt** : il vit sur le corpus, et aucun
    // contrôle d'ici ne peut le lire. Ce qui se vérifie est la correspondance
    // entre le code et le bloc livré — c'est-à-dire le seul risque réel, qu'un
    // type du harnais échappe au registre.
    let bloc = lire("reports/glossaire-sim-harness.md");

    for nom in TYPES_DU_HARNAIS {
        // Le type existe dans le harnais, déclaré une fois.
        let declaration = chercher(
            &format!("(struct|enum|trait) {nom}\\b"),
            &["crates/sim_harness/src"],
        );
        assert_eq!(
            declaration.len(),
            1,
            "« {nom} » est déclaré {} fois dans le harnais",
            declaration.len()
        );

        // Et le bloc livré le nomme.
        assert!(
            bloc.contains(&format!("`{nom}`")),
            "le bloc de glossaire ne porte pas « {nom} »"
        );
    }

    // **Le comptage doit être ancré, et la contre-épreuve le prouve.** Compté
    // sans frontière de mot, le nom du contrat de décision remonterait sept
    // lignes sur un bloc parfaitement correct — les deux traits et les cinq
    // sondes —, et l'exigence « une occurrence » serait rouge. La mauvaise
    // réponse serait de relâcher le comptage.
    let sans_ancrage = bloc.matches("Policy").count();
    assert!(
        sans_ancrage >= 7,
        "le motif nu rend {sans_ancrage} occurrences : la démonstration de \
         l'ancrage ne tient plus"
    );

    // Les deux phrases qui justifient l'existence de la section, et l'unique
    // exception.
    for phrase in [
        "ne franchissent jamais la frontière",
        "aucune crate de jeu ne les importe",
        "dev-dependencies",
    ] {
        assert!(bloc.contains(phrase), "le bloc ne porte pas « {phrase} »");
    }
}

// -------------------------------------------------------------- critère 7

#[test]
fn test_ci_blocks_are_present_and_unamended() {
    let ci = lire(".github/workflows/ci.yml");

    // Les blocs amont sont là, et celui de l'étape aussi.
    for bloc in [
        "---- Étape 3 :",
        "---- Étape 4 :",
        "---- Étape 5 :",
        "---- Étape 6 :",
        "---- Étape 6 bis :",
    ] {
        assert!(ci.contains(bloc), "le bloc « {bloc} » a disparu");
    }
    // Et aucun bloc d'une étape non livrée n'est inventé.
    for absente in ["---- Étape 7 :", "---- Étape 8 :", "---- Étape 9 :"] {
        assert!(!ci.contains(absente), "un bloc « {absente} » est inventé");
    }

    // **Aucune garde n'exclut le harnais en bloc**, et il n'existe **aucun**
    // amendement nominatif : contrairement à ce que le ticket d'audit suppose,
    // les gardes visées sont portées sur les répertoires de production des deux
    // crates de jeu, si bien que le harnais en est déjà sorti. Mesuré à trois
    // reprises au cours de l'étape.
    for drapeau in ["-g '!", "--glob '!"] {
        let exclusion = format!("{drapeau}crates/sim_harness/");
        assert!(
            !ci.contains(&exclusion),
            "une garde exclut le harnais : {exclusion}"
        );
    }

    // La porte de PR n'a ni tolérance, ni relance, ni moyenne.
    let porte = ci
        .split("- name: Porte d'équilibrage (passe courte)")
        .nth(1)
        .and_then(|reste| reste.split("\n      - ").next())
        .unwrap_or_default();
    assert!(!porte.is_empty(), "la porte d'équilibrage a disparu");
    for cle in ["continue-on-error:", "if: failure()", "|| true"] {
        assert!(!porte.contains(cle), "la porte porte une tolérance : {cle}");
    }
    assert_eq!(
        porte.matches("cargo run -p sim_harness").count(),
        1,
        "la porte invoque le harnais plus d'une fois : relance ou moyenne"
    );

    // **La porte compare un rapport à une référence dorée**, et non un taux à
    // une fourchette : au calibrage actuel le taux vaut zéro sur cent cinquante
    // mille runs, et une fourchette écrite dessus ne mesurerait rien.
    assert!(
        porte.contains("ci/short-gate-report.txt"),
        "la porte ne compare plus à la référence dorée"
    );
    assert!(
        porte.contains("ci/short-gate-seed-base.txt"),
        "la graine n'est plus lue dans le fichier versionné"
    );

    // Sans le clone complet, le point de branchement est introuvable et la
    // vérification du moteur échoue pour une raison qui n'a rien à voir avec
    // le moteur.
    let clonage = ci
        .split("- uses: actions/checkout@v7")
        .nth(1)
        .and_then(|reste| reste.split("- name:").next())
        .unwrap_or_default();
    assert!(
        clonage.contains("fetch-depth: 0"),
        "le clone est superficiel"
    );

    // Le nocturne est un fichier séparé, déclenché par planification seule.
    let nocturne = lire(".github/workflows/nightly.yml");
    let entete = nocturne.split("jobs:").next().unwrap_or_default();
    assert!(
        entete.contains("schedule:"),
        "le nocturne n'est pas planifié"
    );
    for declencheur in ["pull_request", "push:"] {
        assert!(
            !entete.contains(declencheur),
            "le nocturne se déclenche sur {declencheur}"
        );
    }
}
