//! Harnais de simulation : la description d'une campagne et son interface.
//!
//! **La crate porte une bibliothèque en plus de son binaire, et ce n'est pas
//! un confort.** Une crate binaire seule ne peut être dépendance de personne —
//! `error[E0433]: cannot find module or crate` —, or le raccord G du backlog
//! de l'étape fait prendre ce harnais en `[dev-dependencies]` par la crate de
//! mise en scène, pour le fixture d'accord de TASK-155. Sans cible
//! bibliothèque, ce test n'a aucune forme possible.
//!
//! Le binaire, lui, tient en trois lignes : il appelle [`run`].

// **Les paniques se mécanisent par un lint, pas par une recherche textuelle.**
// Les tests unitaires du harnais vivent sous `#[cfg(test)]` dans les mêmes
// fichiers que la production, où le dépaquetage forcé est légitime : une
// recherche serait rouge dès le premier test, et la seule correction
// disponible serait d'exclure un fichier entier — c'est-à-dire de perdre
// l'invariant sur la production de ce fichier. Le lint, lui, connaît la
// compilation de test.
//
// **L'attribut vit ici, et pas sur le binaire.** Celui-ci fait six lignes ;
// tout le harnais est dans cette bibliothèque. Posé là-bas, il garderait
// `fn main` et rien d'autre.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod blind;
pub mod campaign;
pub mod config;
pub mod outcome;
pub mod policy;
pub mod report;
pub mod rng;
pub mod run;
pub mod state;
pub mod trace;
pub mod view;

use clap::Parser;
use config::{PolicyKind, ShopPolicyKind, SimConfig, WinRateRange};
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct Cli {
    // Le conflit est déclaré **ici seulement**. `clap` le rend symétrique, et
    // le redéclarer sur `--seed` le rendrait insensible au retrait de l'une
    // des deux : mesuré au banc, le mutant survivait à la suite entière.
    #[arg(long, required_unless_present = "seed", conflicts_with = "seed")]
    runs: Option<u32>,
    #[arg(long = "seed-base", default_value_t = 1)]
    seed_base: u64,
    #[arg(long, value_parser = config::parse_cup)]
    cup: Vec<core_engine::cups::CupId>,
    #[arg(long)]
    stake: Vec<u8>,
    #[arg(long, value_enum, default_value = "grid-aware")]
    policy: PolicyKind,
    #[arg(long = "shop-policy", value_enum, default_value = "budget")]
    shop_policy: ShopPolicyKind,
    #[arg(long)]
    threads: Option<usize>,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    report: bool,
    #[arg(long = "assert-win-rate", value_parser = config::parse_win_rate)]
    assert_win_rate: Option<WinRateRange>,
    #[arg(long)]
    seed: Option<u64>,
    // **Le mode de journal exige une graine et exclut les deux sorties de
    // campagne.** L'exigence de graine passe par l'incompatibilité avec le
    // nombre de runs, et non par une clause de dépendance : celle-ci ne se
    // déclenche pas sur un drapeau booléen, mesuré — l'invocation passait et
    // journalisait la graine par défaut. Le nombre de runs étant déjà requis
    // sauf en présence d'une graine, l'incompatibilité suffit et se vérifie. Il répond à « pourquoi *ce* run a fait ce score » : il n'a
    // pas d'échantillon, et un rapport agrégé sur une observation afficherait
    // des médianes valant la valeur unique et des marqueurs partout — un
    // résultat produit par l'absence d'échantillon, qu'un lecteur prendrait
    // pour une mesure. L'erreur arrive **avant** que le run ne tourne.
    #[arg(long, conflicts_with_all = ["out", "report", "runs"])]
    trace: bool,
}

impl Cli {
    /// La campagne que cette invocation décrit.
    ///
    /// Les cinq drapeaux de sortie — `--out`, `--report`, `--trace`, `--seed`
    /// et la fourchette d'assertion — n'y entrent pas : ils décrivent ce qu'on
    /// fait du résultat, pas ce qu'on mesure.
    fn campagne(&self) -> SimConfig {
        SimConfig {
            // **Une graine vaut un run**, et c'est l'incompatibilité déclarée
            // qui le garantit : `--runs` ne peut pas être présent en même
            // temps, donc le repli vaut un. Une branche explicite sur
            // `self.seed` serait équivalente aujourd'hui — le banc l'a montrée
            // insensible — et fausse demain, si le conflit était levé pour
            // rejouer N runs depuis une graine donnée.
            // `test_seed_and_runs_are_mutually_exclusive` garde les deux moitiés.
            runs: self.runs.unwrap_or(1),
            seed_base: self.seed.unwrap_or(self.seed_base),
            cups: if self.cup.is_empty() {
                config::cups_par_defaut()
            } else {
                self.cup.clone()
            },
            stakes: if self.stake.is_empty() {
                config::stakes_par_defaut()
            } else {
                self.stake.clone()
            },
            policy: self.policy,
            shop_policy: self.shop_policy,
            // Résolu ici plutôt que porté en sentinelle : `SimConfig.threads`
            // est un nombre de threads, et zéro n'en est pas un. Le compte ne
            // change aucun résultat — TASK-151 l'exige — il ne change que le
            // temps mural.
            threads: self
                .threads
                .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from)),
        }
    }
}

/// **Code de sortie 2, et c'est délibéré.** Sortir à zéro ferait lire un succès
/// à tout script qui invoque le harnais aujourd'hui, alors que rien n'a tourné.
/// La ligne disparaît le jour où TASK-151 branche la campagne, sans qu'aucun
/// test ne bouge : les six tests éprouvent le parseur et le verdict
/// directement, jamais par le processus.
pub fn run() -> std::process::ExitCode {
    let verdict = executer(&Cli::parse());
    if let Some(ligne) = verdict.stderr.as_ref() {
        eprintln!("{ligne}");
    }
    std::process::ExitCode::from(verdict.code)
}

/// Ce que l'invocation fait, **sous une forme qu'un test peut lire**.
///
/// Un code de sortie ne se décompose pas : sans ce cœur, ni le code nominal, ni
/// le refus de la porte, ni l'écriture conditionnelle du tableau ne seraient
/// éprouvables autrement qu'en lançant un sous-processus.
#[must_use]
pub fn executer(cli: &Cli) -> config::Verdict {
    executer_avec_sortie(cli, &mut std::io::stdout())
}

/// La même chose, **la sortie standard passée en paramètre**.
///
/// Sans elle, rien ne distingue un rapport affiché d'un rapport oublié : le
/// code de sortie est le même, la campagne tourne dans les deux cas, et
/// l'affichage est un effet de bord qu'aucun test ne peut observer.
pub fn executer_avec_sortie(cli: &Cli, sortie: &mut impl std::io::Write) -> config::Verdict {
    let campagne = cli.campagne();

    // **Un drapeau sans effet le dit.** Silencieusement ignoré, il fait croire
    // à un rapport vide plutôt qu'à un rapport absent.
    let mut messages: Vec<String> = Vec::new();

    // **Le journal court avant tout le reste, et sort seul.** Le mode implique
    // un run unique : la section parallèle est contournée, et les deux autres
    // sorties sont exclues par la ligne de commande.
    if cli.trace {
        let (_, _, journal) = campaign::simulate_one_traced(&campagne, campagne.seed_base);
        let _ = write!(sortie, "{}", journal.rendu());
        return config::Verdict {
            code: 0,
            stderr: None,
        };
    }

    let resultats = campaign::campagne(&campagne);

    // **Le rapport est une sortie standard**, et le tableau n'en reçoit rien :
    // une ligne de résumé dans le fichier casserait le tri, le tableau croisé
    // et la comparaison d'empreintes.
    if cli.report {
        let _ = write!(
            sortie,
            "{}",
            report::rendu(&report::agreger(&campagne, &resultats))
        );
    }

    // **Sans chemin de sortie, aucun tableau n'est écrit** : la campagne tourne,
    // la porte s'applique, et la sortie standard reste libre pour le rapport
    // console, qui est une sortie standard par nature.
    if let Some(chemin) = cli.out.as_ref() {
        let ecriture = std::fs::File::create(chemin)
            .map_err(|erreur| format!("{} : {erreur}", chemin.display()))
            .and_then(|fichier| campaign::ecrire_csv(fichier, &resultats));
        if let Err(message) = ecriture {
            messages.push(message);
            return config::Verdict {
                code: 1,
                stderr: Some(messages.join("\n")),
            };
        }
    }

    // La porte d'intégration : le code **1** hors fourchette, avec la valeur
    // mesurée et l'intervalle attendu sur la sortie d'erreur.
    let Some(fourchette) = cli.assert_win_rate else {
        return config::Verdict {
            code: 0,
            stderr: (!messages.is_empty()).then(|| messages.join("\n")),
        };
    };
    let verdict = campaign::porte(&resultats, fourchette);
    messages.extend(verdict.stderr);
    config::Verdict {
        code: verdict.code,
        stderr: (!messages.is_empty()).then(|| messages.join("\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::cups::CupId;

    fn cli(ligne: &str) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(ligne.split_whitespace())
    }

    #[test]
    fn test_no_bevy_in_dependency_tree() {
        let sortie = std::process::Command::new(env!("CARGO"))
            .args(["tree", "-p", "sim_harness", "--edges", "normal"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output();
        let sortie = match sortie {
            Ok(sortie) => sortie,
            Err(erreur) => panic!("cargo tree n'a pas pu être lancé : {erreur}"),
        };
        assert!(
            sortie.status.success(),
            "{}",
            String::from_utf8_lossy(&sortie.stderr)
        );
        let arbre = String::from_utf8_lossy(&sortie.stdout);
        // **Non vacuous** : un arbre vide ou tronqué passerait le filtre sans
        // rien garder. Le harnais tire le moteur, donc il est là.
        assert!(
            arbre.contains("core_engine"),
            "l'arbre ne porte pas le moteur, le filtre ne garde rien : {arbre}"
        );
        let intrus: Vec<&str> = arbre
            .lines()
            .filter(|ligne| ligne.contains("bevy"))
            .collect();
        assert!(intrus.is_empty(), "{intrus:?}");
    }

    #[test]
    fn test_cli_parses_the_documented_invocations() {
        let premiere = cli(
            "sim_harness --runs 10000 --seed-base 1 --cup standard --stake 1 \
             --policy grid-aware --shop-policy budget \
             --threads 8 --out reports/balance.csv --report",
        )
        .expect("première invocation documentée");
        assert_eq!(
            premiere.campagne(),
            SimConfig {
                runs: 10_000,
                seed_base: 1,
                cups: vec![CupId::Standard],
                stakes: vec![1],
                policy: PolicyKind::GridAware,
                shop_policy: ShopPolicyKind::Budget,
                threads: 8,
            }
        );
        assert_eq!(premiere.out, Some(PathBuf::from("reports/balance.csv")));
        assert!(premiere.report);

        let deuxieme = cli(
            "sim_harness --runs 500 --seed-base 1 --cup standard --stake 1 \
             --policy grid-aware --assert-win-rate 0.25:0.40",
        )
        .expect("deuxième invocation documentée");
        assert_eq!(
            deuxieme.assert_win_rate,
            Some(WinRateRange {
                min: 2500,
                max: 4000
            })
        );

        let troisieme = cli("sim_harness --seed 12345 --trace").expect("troisième invocation");
        assert!(troisieme.trace);
        let campagne = troisieme.campagne();
        assert_eq!(campagne.runs, 1, "une graine vaut un run");
        assert_eq!(campagne.seed_base, 12345, "la graine est la base");

        // Sans `--cup` ni `--stake`, la matrice par défaut.
        assert_eq!(campagne.cups, config::cups_par_defaut());
        assert_eq!(campagne.cups.len(), 5);
        assert_eq!(campagne.stakes, vec![1, 3, 4], "raccord F");

        // Répétés, les deux drapeaux portent chacun leur liste : c'est leur
        // produit que la campagne parcourra.
        let matrice = cli("sim_harness --runs 1 --cup standard --cup fortune --stake 1 --stake 4")
            .expect("drapeaux répétés")
            .campagne();
        assert_eq!(matrice.cups, vec![CupId::Standard, CupId::Fortune]);
        assert_eq!(matrice.stakes, vec![1, 4]);
    }

    #[test]
    fn test_seed_and_runs_are_mutually_exclusive() {
        let erreur = cli("sim_harness --seed 1 --runs 10")
            .expect_err("les deux ensemble doivent être refusés, pas ignorés");
        assert_eq!(
            erreur.kind(),
            clap::error::ErrorKind::ArgumentConflict,
            "le refus doit venir de l'outil, pas d'un contrôle après coup"
        );

        assert_eq!(
            cli("sim_harness --seed 1")
                .expect("une graine seule")
                .campagne()
                .runs,
            1
        );

        // Et sans l'une ni l'autre, l'outil refuse plutôt que d'inventer une
        // taille de campagne.
        assert_eq!(
            cli("sim_harness --cup standard")
                .expect_err("ni graine ni nombre de runs")
                .kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn test_missing_api_file_exists() {
        let chemin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("MISSING_API.md");
        let contenu = std::fs::read_to_string(&chemin)
            .unwrap_or_else(|erreur| panic!("{} : {erreur}", chemin.display()));
        assert!(!contenu.trim().is_empty(), "le fichier est vide");
        // Le format du § 2.7 : une entrée par manque, chacune portant sa
        // signature souhaitée et son étape propriétaire.
        assert!(
            contenu.contains("Étape propriétaire"),
            "l'étape propriétaire manque"
        );
        assert!(contenu.contains("```rust"), "la signature souhaitée manque");
        assert!(
            contenu.contains("CupId"),
            "le manque connu n'est pas inscrit"
        );
    }
}
