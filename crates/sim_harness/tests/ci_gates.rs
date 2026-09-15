//! Les portes de CI, éprouvées **sur les fichiers qui les portent**.
//!
//! Ces tests lisent les workflows et les fichiers de `ci/` ; ils ne lancent
//! aucune CI. Leurs chemins se construisent depuis le manifeste de la crate,
//! jamais relativement au répertoire courant : `cargo test` n'en garantit pas
//! la valeur.
//!
//! **Le fichier n'est pas un lecteur de YAML.** Une porte de CI est une ligne
//! de shell dans un bloc de texte ; ce qu'on éprouve, c'est cette ligne, et un
//! analyseur qui la normaliserait masquerait précisément les fautes de frappe
//! qui la rendent muette.

use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use sim_harness::campaign::{campagne, porte, taux_de_victoire};
use sim_harness::config::{PolicyKind, ShopPolicyKind, SimConfig, WinRateRange};
use sim_harness::{Cli, executer_avec_sortie};

/// La racine du dépôt. Le manifeste est en `crates/sim_harness/`.
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

fn ci_yml() -> String {
    lire(".github/workflows/ci.yml")
}

fn nightly_yml() -> String {
    lire(".github/workflows/nightly.yml")
}

/// Le corps d'une étape nommée, de sa ligne `- name:` à l'étape suivante.
///
/// Découpé sur l'indentation plutôt que par un analyseur : c'est le texte
/// exécuté qui nous intéresse, y compris ses commentaires, que toute
/// normalisation effacerait.
fn etape(yaml: &str, nom: &str) -> String {
    let entete = format!("- name: {nom}");
    let debut = yaml
        .find(&entete)
        .unwrap_or_else(|| panic!("étape « {nom} » absente"));
    let reste = &yaml[debut..];
    let fin = reste
        .match_indices("\n      - ")
        .map(|(index, _)| index)
        .find(|index| *index > 0)
        .unwrap_or(reste.len());
    reste[..fin].to_string()
}

/// La graine de base versionnée, telle que la CI la lit.
fn seed_base_versionnee() -> u64 {
    lire("ci/short-gate-seed-base.txt")
        .trim()
        .parse()
        .expect("la graine de base est un entier")
}

/// Les arguments de la passe courte, **extraits de la CI elle-même**.
///
/// C'est ce qui empêche la référence dorée et le workflow de diverger : le test
/// rejoue la commande écrite dans le fichier, pas une copie posée à côté.
fn arguments_de_la_porte() -> Vec<String> {
    let bloc = etape(&ci_yml(), "Porte d'équilibrage (passe courte)");
    let debut = bloc
        .find("sim_harness --release --")
        .expect("la commande de la porte");
    let apres = &bloc[debut + "sim_harness --release --".len()..];
    let fin = apres.find('>').unwrap_or(apres.len());
    apres[..fin]
        .replace("\\\n", " ")
        .replace(
            "\"$(cat ci/short-gate-seed-base.txt)\"",
            &seed_base_versionnee().to_string(),
        )
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn cellule_courte() -> SimConfig {
    SimConfig {
        runs: 500,
        seed_base: seed_base_versionnee(),
        cups: vec![core_engine::cups::CupId::Standard],
        stakes: vec![1],
        policy: PolicyKind::GridAware,
        shop_policy: ShopPolicyKind::Budget,
        threads: 4,
    }
}

/// Exécute un script de `ci/` depuis la racine du dépôt.
fn script(nom: &str, arguments: &[&str]) -> (i32, String) {
    let sortie = Command::new(racine().join(nom))
        .args(arguments)
        .current_dir(racine())
        .output()
        .unwrap_or_else(|erreur| panic!("{nom} : {erreur}"));
    let mut texte = String::from_utf8_lossy(&sortie.stdout).into_owned();
    texte.push_str(&String::from_utf8_lossy(&sortie.stderr));
    (sortie.status.code().unwrap_or(-1), texte)
}

// ---------------------------------------------------------------- passe courte

#[test]
fn test_short_gate_uses_a_fixed_seed_base() {
    let bloc = etape(&ci_yml(), "Porte d'équilibrage (passe courte)");

    for attendu in [
        "--runs 500",
        "--cup standard",
        "--stake 1",
        "--policy grid-aware",
        "--shop-policy budget",
        "--report",
    ] {
        assert!(bloc.contains(attendu), "la porte ne porte pas {attendu}");
    }

    // La graine vient du fichier versionné, et de rien d'autre.
    assert!(
        bloc.contains("--seed-base \"$(cat ci/short-gate-seed-base.txt)\""),
        "la graine n'est pas lue dans le fichier versionné"
    );
    // **Ancré sur l'acte, pas sur les noms.** Interdire les mots « date » ou
    // « run_id » interdirait le commentaire qui explique pourquoi ils sont
    // proscrits — le défaut le plus fréquent de ce corpus. Ce qui rend une
    // graine volatile, c'est une **substitution de commande** ; il n'y en a
    // qu'une, et c'est la lecture du fichier versionné.
    assert_eq!(
        bloc.matches("$(").count(),
        1,
        "la porte porte une seconde substitution de commande : la graine peut dériver"
    );
    assert!(
        !bloc.contains("${{"),
        "la porte lit une expression du service : elle cesserait d'être un test doré"
    );

    // **La graine versionnée n'est pas le défaut du drapeau.** À `1`, retirer
    // `--seed-base` de la commande ne changerait aucun résultat, et le drapeau
    // serait décoratif : aucun test ne pourrait prouver qu'il est lu.
    assert_ne!(
        seed_base_versionnee(),
        1,
        "la graine versionnée vaut le défaut du drapeau : le drapeau ne porte rien"
    );
}

#[test]
fn test_short_gate_reference_is_byte_exact() {
    // La commande de la CI, rejouée par la bibliothèque, doit rendre
    // **exactement** le fichier de référence. C'est la porte elle-même, jouée
    // en local : un chiffre d'équilibrage qui bouge échoue ici avant d'échouer
    // sur la PR, et le message dit quoi ré-enregistrer.
    let mut ligne = vec!["sim_harness".to_string()];
    ligne.extend(arguments_de_la_porte());
    let cli = Cli::parse_from(&ligne);

    let mut produit = Vec::new();
    let verdict = executer_avec_sortie(&cli, &mut produit);
    assert_eq!(verdict.code, 0, "{:?}", verdict.stderr);

    let reference = lire("ci/short-gate-report.txt");
    let produit = String::from_utf8(produit).expect("le rapport est du texte");
    assert_eq!(
        produit, reference,
        "la passe courte ne rend plus la référence dorée. Si le changement est \
         voulu, ré-enregistre ci/short-gate-report.txt dans la même PR."
    );

    // **Non vacuous** : une référence vide passerait la comparaison sans rien
    // garder.
    assert!(
        reference.contains("taux de victoire par cellule"),
        "la référence ne porte pas de rapport"
    );
}

#[test]
fn test_gate_exits_one_out_of_range() {
    // Le code de sortie et l'intervalle sur `stderr` sont déjà éprouvés par
    // `test_assert_win_rate_exits_one_on_a_real_campaign` (TASK-151). Ce test
    // porte les **deux clauses que cet autre ne couvre pas** : la valeur
    // mesurée dans le message, et l'égalité au dix-millième entre deux
    // exécutions — sans laquelle la porte serait un tirage, pas un test doré.
    let config = cellule_courte();
    let premiere = taux_de_victoire(&campagne(&config));
    let seconde = taux_de_victoire(&campagne(&config));
    assert_eq!(
        premiere, seconde,
        "deux exécutions de la passe courte rendent des taux différents"
    );

    let refus = porte(
        &campagne(&config),
        WinRateRange {
            min: 2_500,
            max: 4_000,
        },
    );
    assert_eq!(refus.code, 1);
    let message = refus.stderr.unwrap_or_default();
    let mesure = format!("{}.{:04}", premiere / 10_000, premiere % 10_000);
    assert!(
        message.contains(&mesure),
        "la valeur mesurée « {mesure} » manque au message : {message}"
    );
}

#[test]
fn test_gate_has_no_tolerance_and_no_retry() {
    let bloc = etape(&ci_yml(), "Porte d'équilibrage (passe courte)");
    // Les trois formes de tolérance sont des **clés**, pas des mots : le
    // commentaire qui explique pourquoi elles sont proscrites doit pouvoir les
    // nommer, et le ticket l'exige même.
    for cle in ["continue-on-error:", "if: failure()", "|| true"] {
        assert!(!bloc.contains(cle), "la porte porte une tolérance : {cle}");
    }

    // **Une seule invocation.** C'est l'acte qui distingue une porte d'une
    // moyenne sur plusieurs graines comme d'une relance : les deux passent par
    // un second appel, et aucune formulation ne les en dispense.
    assert_eq!(
        bloc.matches("cargo run -p sim_harness").count(),
        1,
        "la porte invoque le harnais plus d'une fois : relance ou moyenne"
    );

    // **La fourchette est écrite une seule fois dans le dépôt.** Deux
    // occurrences, c'est deux endroits à corriger le jour du recalibrage, et
    // l'une des deux restera.
    let occurrences = [
        ci_yml().matches("25 % – 40 %").count(),
        nightly_yml().matches("25 % – 40 %").count(),
        lire("ci/short-gate-seed-base.txt")
            .matches("25 % – 40 %")
            .count(),
    ]
    .iter()
    .sum::<usize>();
    assert_eq!(
        occurrences, 1,
        "la fourchette apparaît {occurrences} fois, elle doit l'être une seule"
    );
}

// ------------------------------------------------------------------- nocturne

#[test]
fn test_nightly_compares_two_architectures() {
    let yaml = nightly_yml();

    // **Deux runners, et la matrice le déclare.** Le chercher dans le fichier
    // entier ne garde rien : la seconde architecture y est nommée une seconde
    // fois, dans le chemin de l'artefact que la comparaison télécharge. Mesuré
    // au banc, retirer le runner de la matrice survivait.
    let matrice = yaml
        .split("matrix:")
        .nth(1)
        .and_then(|reste| reste.split("runs-on:").next())
        .unwrap_or_default();
    assert!(
        matrice.contains("ubuntu-latest"),
        "le runner x86 n'est pas dans la matrice"
    );
    assert!(
        matrice.contains("ubuntu-24.04-arm"),
        "le runner ARM n'est pas dans la matrice : le job ne compare rien"
    );

    // Même graine de base, lue au même endroit que la passe courte, et même
    // matrice — sans quoi la comparaison d'empreintes ne compare rien.
    assert!(
        yaml.contains("ci/short-gate-seed-base.txt"),
        "le nocturne n'emploie pas la graine versionnée"
    );
    assert!(
        yaml.contains("--runs 100000"),
        "la taille de la passe longue"
    );

    // La comparaison porte sur les **octets du fichier**, et elle échoue.
    assert!(
        yaml.contains("sha256sum") || yaml.contains("shasum"),
        "aucune empreinte n'est calculée"
    );
    // **Le message ne suffit pas : c'est l'échec qui porte l'invariant.** Le
    // garder en retirant la sortie non nulle laisserait le job vert sur une
    // rupture du déterminisme, c'est-à-dire sur la seule chose que ce job
    // existe pour attraper.
    let comparaison = yaml
        .split("Déterminisme entre les deux architectures")
        .nth(1)
        .unwrap_or_default();
    assert!(
        comparaison.contains("empreintes divergent"),
        "la divergence n'est pas dite"
    );
    assert!(
        comparaison.contains("exit 1"),
        "l'écart d'empreinte ne fait rien échouer"
    );

    // Il archive le tableau **et** le rapport.
    // L'archivage se lit dans le job qui **produit**, pas dans celui qui
    // consomme : le nom de l'artefact apparaît des deux côtés, et le chercher
    // dans le fichier entier laisse survivre un renommage à la source.
    let production = yaml.split("comparaison:").next().unwrap_or_default();
    assert!(production.contains("upload-artifact"), "aucun archivage");
    assert!(
        production.contains("name: rapports-"),
        "les rapports ne sont pas archivés"
    );
    assert!(
        production.contains("name: csv-"),
        "les tableaux ne sont pas archivés"
    );
}

#[test]
fn test_nightly_is_not_a_pr_gate() {
    let yaml = nightly_yml();
    let entete = yaml.split("jobs:").next().unwrap_or_default().to_string();

    assert!(
        entete.contains("schedule:"),
        "le nocturne n'est pas planifié"
    );
    assert!(
        entete.contains("workflow_dispatch:"),
        "le nocturne ne se déclenche pas à la main"
    );
    for declencheur in ["pull_request", "push:"] {
        assert!(
            !entete.contains(declencheur),
            "le nocturne se déclenche sur {declencheur} : cent mille runs par PR"
        );
    }

    // La dérive signale, elle ne fait pas échouer : le comparateur rend
    // toujours zéro, et c'est sa propriété, pas un `|| true` dans le YAML.
    let (code, _) = script("ci/compare-nightly.sh", &["/nexiste/pas", "/nexiste/pas"]);
    assert_eq!(code, 0, "le comparateur fait échouer le job");
}

#[test]
fn test_nightly_first_run_has_no_reference() {
    let veille = std::env::temp_dir().join("yamstro-veille-absente");
    let _ = std::fs::remove_dir_all(&veille);
    let nuit = std::env::temp_dir().join("yamstro-nuit");
    std::fs::create_dir_all(&nuit).expect("répertoire de la nuit");
    std::fs::write(
        nuit.join("grid-aware-budget.txt"),
        lire("ci/short-gate-report.txt"),
    )
    .expect("rapport de la nuit");

    let (code, texte) = script(
        "ci/compare-nightly.sh",
        &[
            veille.to_str().expect("chemin"),
            nuit.to_str().expect("chemin"),
        ],
    );
    assert_eq!(code, 0, "la première nuit échoue : {texte}");
    assert!(
        texte.contains("aucune référence de la veille"),
        "la première nuit ne le dit pas : {texte}"
    );
}

#[test]
fn test_drift_comparator_is_integer() {
    let base = std::env::temp_dir().join("yamstro-derive");
    let veille = base.join("veille");
    let nuit = base.join("nuit");
    for repertoire in [&veille, &nuit] {
        let _ = std::fs::remove_dir_all(repertoire);
        std::fs::create_dir_all(repertoire).expect("répertoire");
    }

    let rapport = |cellule: &str| {
        format!("-- taux de victoire par cellule --\n  standard     mise 1     {cellule}\n")
    };
    std::fs::write(veille.join("grid-aware-budget.txt"), rapport("30,00 %")).expect("veille");

    // 2,90 points : sous le seuil, rien de signalé.
    std::fs::write(nuit.join("grid-aware-budget.txt"), rapport("32,90 %")).expect("nuit");
    let (code, texte) = script(
        "ci/compare-nightly.sh",
        &[
            veille.to_str().expect("chemin"),
            nuit.to_str().expect("chemin"),
        ],
    );
    assert_eq!(code, 0);
    // La clé est **normalisée** par le comparateur : l'espérer alignée ferait
    // passer cette assertion pour la mauvaise raison.
    assert!(
        !texte.contains("standard mise 1 :"),
        "2,90 points ont été signalés : {texte}"
    );

    // 3,10 points : au-dessus, la cellule est signalée.
    std::fs::write(nuit.join("grid-aware-budget.txt"), rapport("33,10 %")).expect("nuit");
    let (code, texte) = script(
        "ci/compare-nightly.sh",
        &[
            veille.to_str().expect("chemin"),
            nuit.to_str().expect("chemin"),
        ],
    );
    assert_eq!(code, 0, "la dérive fait échouer le job");
    assert!(
        texte.contains("standard mise 1 : 3000 -> 3310"),
        "3,10 points n'ont pas été signalés : {texte}"
    );

    // **L'alignement change sans que la cellule change.** Le rapport aligne ses
    // colonnes sur le plus long libellé : une nuit dont une cellule au nom plus
    // long apparaîtrait décalerait toutes les autres, et un comparateur qui
    // prendrait le libellé brut pour clé n'aurait alors plus **aucune** cellule
    // en commun avec la veille — il passerait en silence, chaque nuit.
    std::fs::write(
        nuit.join("grid-aware-budget.txt"),
        "-- taux de victoire par cellule --\n  standard   mise 1   33,10 %\n",
    )
    .expect("nuit");
    let (_, texte) = script(
        "ci/compare-nightly.sh",
        &[
            veille.to_str().expect("chemin"),
            nuit.to_str().expect("chemin"),
        ],
    );
    assert!(
        texte.contains("standard mise 1 : 3000 -> 3310"),
        "un alignement différent fait perdre la cellule : {texte}"
    );

    // **La comparaison se fait en dix-millièmes.** Un flottant ferait diverger
    // le seuil d'une plateforme à l'autre, sur la seule grandeur que le job
    // existe pour surveiller.
    let source = lire("ci/compare-nightly.sh");
    for flottant in ["bc ", "| bc", "awk", "%f", "float"] {
        assert!(
            !source.contains(flottant),
            "le comparateur emploie {flottant} : la comparaison n'est plus entière"
        );
    }
    assert!(
        source.contains("300"),
        "le seuil de trois points n'est pas écrit en dix-millièmes"
    );
}

// ------------------------------------------------------- bloc d'invariants

#[test]
fn test_new_invariant_block_is_wired() {
    let yaml = ci_yml();
    let bloc = etape(&yaml, "Invariants normatifs Étape 6 bis");

    // Les cinq contrôles réellement manquants.
    // Le manifeste est balayé **sans tenir compte de la casse et sans
    // frontière de mot** : c'est ce qui attrape une feature activée autant
    // qu'une brique du moteur graphique glissée en dépendance de test.
    assert!(bloc.contains("Cargo.toml"), "le manifeste n'est pas balayé");
    assert!(
        bloc.contains("-in 'bevy'"),
        "le manifeste est balayé en tenant compte de la casse"
    );
    assert!(
        bloc.contains("--edges normal"),
        "l'arbre normal n'est pas lu"
    );
    assert!(
        bloc.contains("merge-base"),
        "la règle n°1 n'est pas vérifiée"
    );
    // **C'est l'invocation qui garde, pas le motif.** Remplacer `require` par
    // une commande sans effet laisse le motif dans le fichier et la garde
    // muette — mesuré au banc, le mutant survivait.
    assert!(
        bloc.contains(r#"require "critère 6  les paniques sont mécanisées par le lint""#),
        "la contre-épreuve de présence du lint manque"
    );
    assert!(
        bloc.contains("clippy::unwrap_used"),
        "la contre-épreuve ne cherche pas le lint"
    );
    // Et l'attribut est **là où il garde quelque chose**. Le binaire fait six
    // lignes ; posé sur lui, le lint garderait `fn main`. La contre-épreuve de
    // la CI pointe la bibliothèque, ce test vérifie qu'elle y est.
    assert!(
        lire("crates/sim_harness/src/lib.rs")
            .contains("deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)"),
        "la bibliothèque ne porte pas le lint"
    );
    assert!(
        !lire("crates/sim_harness/src/main.rs").contains("clippy::unwrap_used"),
        "le lint est sur le binaire, où il ne garde que six lignes"
    );
    assert!(
        bloc.contains("HashMap") && bloc.contains("f64"),
        "le flottant et la table de hachage ne sont pas balayés sur la crate"
    );

    // Les blocs amont sont là, et aucun bloc d'une étape non livrée n'est
    // inventé : leurs tickets ne sont pas écrits.
    for etape_amont in [
        "---- Étape 3 :",
        "---- Étape 4 :",
        "---- Étape 5 :",
        "---- Étape 6 :",
    ] {
        assert!(
            yaml.contains(etape_amont),
            "le bloc de l'{etape_amont} a disparu"
        );
    }
    // Ancré sur l'**en-tête de bloc**. Les étapes aval sont nommées neuf fois
    // dans les commentaires du volet 1, légitimement : c'est déclarer un bloc
    // qui est proscrit, pas prononcer le nom.
    for absente in ["---- Étape 7 :", "---- Étape 8 :", "---- Étape 9 :"] {
        assert!(
            !yaml.contains(absente),
            "un bloc « {absente} » est inventé : son ticket n'est pas livré"
        );
    }
}

#[test]
fn test_no_guard_excludes_the_harness_wholesale() {
    // **Une exclusion nominative de fichier reste permise ; l'exclusion du
    // répertoire est un renoncement.** Elle supprimerait l'invariant sur tout
    // le harnais d'un coup, et un second site fautif y resterait vert.
    for (nom, source) in [
        (".github/workflows/ci.yml", ci_yml()),
        (".github/workflows/nightly.yml", nightly_yml()),
    ] {
        // **Ancré sur le drapeau, pas sur la chaîne.** Écrite seule, la chaîne
        // apparaît légitimement dans le commentaire qui explique pourquoi
        // l'exclusion du répertoire est un renoncement ; c'est son emploi
        // **comme motif d'exclusion** qui est proscrit.
        for drapeau in ["-g '!", "--glob '!"] {
            for repertoire in ["crates/sim_harness/'", "crates/sim_harness/**"] {
                let exclusion = format!("{drapeau}{repertoire}");
                assert!(
                    !source.contains(&exclusion),
                    "{nom} exclut le harnais en bloc : {exclusion}"
                );
            }
        }
    }
}

#[test]
fn test_engine_diff_is_compared_to_the_merge_base() {
    let bloc = etape(&ci_yml(), "Invariants normatifs Étape 6 bis");

    // La forme qui mord : la base de comparaison est le point de branchement,
    // pas l'index. Un `git diff` nu sur un checkout frais est vide par
    // construction — un contrôle incapable d'échouer.
    assert!(
        bloc.contains("git merge-base origin/main HEAD"),
        "la base de comparaison n'est pas le point de branchement"
    );
    // **Depuis la passe `rand` 0.10 du 15 septembre 2026, la règle porte sur
    // les commits du harnais, pas sur l'arbre entier.** « Le moteur n'a pas
    // bougé depuis qu'on a branché » était la forme de l'étape ; gardée après
    // la clôture, elle interdisait tout changement du moteur sur n'importe
    // quelle PR. La règle n°1 dit que le harnais ne modifie jamais le moteur :
    // aucun commit au périmètre `(sim_harness)` ne touche `crates/core_engine/`.
    assert!(
        bloc.contains("git rev-list \"$base..HEAD\""),
        "les commits depuis le point de branchement ne sont pas parcourus"
    );
    assert!(
        bloc.contains("*'(sim_harness)'*"),
        "le périmètre du harnais n'est pas ce qui déclenche la règle"
    );
    assert!(
        bloc.contains("grep -q '^crates/core_engine/'"),
        "l'écart ne fait rien échouer"
    );
    assert!(
        !bloc.contains("origin/${{ github.base_ref }}"),
        "la base de la PR est vide sur un push : `git diff origin/` rend \
         « bad revision » à chaque exécution"
    );

    // **Sans cela, `origin/main` n'est pas dans le clone.** L'option ressemble
    // à un réglage de performance ; c'est ce qui rend la règle n°1 exécutable.
    //
    // **Ancré sur l'étape, pas sur le fichier.** Le commentaire qui explique
    // l'option la cite forcément : le chercher dans le fichier entier rend la
    // garde muette au retrait de l'option elle-même — mesuré au banc, le
    // mutant survivait.
    let yaml = ci_yml();
    let checkout = yaml
        .split("- uses: actions/checkout@v7")
        .nth(1)
        .and_then(|reste| reste.split("- name:").next())
        .unwrap_or_default();
    assert!(
        checkout.contains("fetch-depth: 0"),
        "le clone est superficiel : le point de branchement est introuvable"
    );
    assert!(
        yaml.contains("origin/main est une révision inconnue"),
        "l'option n'est pas commentée : on l'« optimisera » un jour de CI lente"
    );

    // Et la règle tient **ici**, sur l'arbre courant : aucun commit du harnais
    // depuis le point de branchement ne touche le moteur.
    let git = |args: &[&str]| {
        let sortie = Command::new("git")
            .args(args)
            .current_dir(racine())
            .output()
            .expect("git");
        String::from_utf8_lossy(&sortie.stdout).trim().to_string()
    };
    let base = git(&["merge-base", "origin/main", "HEAD"]);
    let plage = format!("{base}..HEAD");
    for sha in git(&["rev-list", &plage]).lines() {
        let sujet = git(&["log", "-1", "--format=%s", sha]);
        if !sujet.contains("(sim_harness)") {
            continue;
        }
        let fichiers = git(&["diff-tree", "--no-commit-id", "--name-only", "-r", sha]);
        assert!(
            !fichiers
                .lines()
                .any(|fichier| fichier.starts_with("crates/core_engine/")),
            "le commit du harnais {sha} touche le moteur :\n{fichiers}"
        );
    }
}

#[test]
fn test_forbidden_literals_are_still_covered() {
    let yaml = ci_yml();

    // Les deux gardes existent **déjà**, posées en amont : le littéral de la
    // table de rareté est interdit dans le harnais depuis TASK-148, et la
    // courbe est gardée structurellement sur `crates/` entier.
    //
    // **Ancré sur le libellé de la garde, jamais sur une copie de son motif.**
    // Recopier le motif ici ferait tomber les gardes elles-mêmes — mesuré :
    // deux échecs du volet 1, déclenchés par ce fichier — et créerait un
    // second endroit à corriger le jour où le motif change, ce qui est
    // exactement ce que ce test existe pour empêcher.
    assert!(
        yaml.contains("table de rareté recopiée dans le harnais"),
        "le littéral de rareté n'est plus interdit dans le harnais"
    );
    assert!(
        yaml.contains("seconde courbe de difficulté"),
        "la seconde courbe n'est plus gardée"
    );

    // **Le nouveau bloc ne les duplique pas** : un second endroit à corriger le
    // jour où le chemin d'exclusion change.
    let bloc = etape(&yaml, "Invariants normatifs Étape 6 bis");
    for libelle in [
        "table de rareté recopiée dans le harnais",
        "seconde courbe de difficulté",
    ] {
        assert!(
            !bloc.contains(libelle),
            "le nouveau bloc duplique une garde existante : {libelle}"
        );
    }

    // Et le harnais ne les porte pas. **Le littéral de rareté s'assemble ici**
    // plutôt qu'il ne s'écrit : épelé d'un trait, il ferait tomber la garde
    // qui l'interdit — et ce qu'on cherche reste bien la chaîne réelle.
    let rarete = format!("[{}, {}, {}, {}]", 650, 250, 90, 10);
    for interdit in ["300_000", "1_600", rarete.as_str()] {
        // **Porté sur la production, pas sur l'arbre entier.** Ce fichier-ci
        // doit pouvoir nommer ce qu'il interdit, sans quoi le test se
        // déclenche sur lui-même — le défaut le plus fréquent de ce corpus.
        let trouve = Command::new("grep")
            .args(["-rn", "-F", interdit, "crates/sim_harness/src"])
            .current_dir(racine())
            .output()
            .expect("grep");
        assert!(
            !trouve.status.success(),
            "le harnais porte {interdit} :\n{}",
            String::from_utf8_lossy(&trouve.stdout)
        );
    }
}
