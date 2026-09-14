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

pub mod blind;
pub mod config;
pub mod policy;
pub mod rng;
pub mod run;
pub mod state;
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
    #[arg(long)]
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

    /// Ce que l'invocation aurait fait. **Ce ticket parse, valide et sort** :
    /// la boucle de manche arrive à TASK-144 et la campagne à TASK-151.
    fn recapitulatif(&self) -> String {
        let campagne = self.campagne();
        let liste = |noms: Vec<String>| noms.join(", ");
        format!(
            "campagne non implémentée — rien n'a été simulé.\n  \
             runs           : {}\n  \
             graine de base : {}\n  \
             gobelets       : {}\n  \
             stakes         : {}\n  \
             politiques     : {:?} / {:?}\n  \
             threads        : {}\n  \
             sortie         : {}\n  \
             rapport        : {}\n  \
             journal        : {}\n  \
             porte          : {}\n\
             TASK-144 pose la boucle de manche, TASK-151 la campagne et son CSV.",
            campagne.runs,
            campagne.seed_base,
            liste(campagne.cups.into_iter().map(config::cup_nom).collect()),
            liste(campagne.stakes.iter().map(u8::to_string).collect()),
            campagne.policy,
            campagne.shop_policy,
            campagne.threads,
            self.out.as_ref().map_or_else(
                || "aucune".to_owned(),
                |chemin| chemin.display().to_string()
            ),
            if self.report { "demandé" } else { "non" },
            if self.trace { "demandé" } else { "non" },
            self.assert_win_rate
                .map_or_else(|| "aucune".to_owned(), |porte| format!("{porte:?}")),
        )
    }
}

/// **Code de sortie 2, et c'est délibéré.** Sortir à zéro ferait lire un succès
/// à tout script qui invoque le harnais aujourd'hui, alors que rien n'a tourné.
/// La ligne disparaît le jour où TASK-151 branche la campagne, sans qu'aucun
/// test ne bouge : les six tests éprouvent le parseur et le verdict
/// directement, jamais par le processus.
pub fn run() -> std::process::ExitCode {
    eprintln!("{}", Cli::parse().recapitulatif());
    std::process::ExitCode::from(2)
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
