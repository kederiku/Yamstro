//! Le déterminisme de la campagne, éprouvé **de l'extérieur**.
//!
//! Ce fichier est un test d'intégration : il se lie à la bibliothèque compilée
//! **sans** la compilation de test, donc il ne peut nommer aucune fixture
//! interne. C'est voulu — il n'éprouve que la surface publique, celle que la
//! campagne nocturne et le fixture d'accord emploieront.

use clap::Parser;
use core_engine::cups::CupId;
use sim_harness::campaign::{
    campagne, ecrire_csv, porte, simulate_one, taux_de_victoire, threads_effectifs,
};
use sim_harness::config::{PolicyKind, ShopPolicyKind, SimConfig, WinRateRange, verdict};
use sim_harness::outcome::RunOutcome;
use sim_harness::{Cli, executer};

fn cellule(runs: u32, threads: usize) -> SimConfig {
    SimConfig {
        runs,
        seed_base: 1,
        cups: vec![CupId::Standard],
        stakes: vec![1],
        policy: PolicyKind::GridAware,
        shop_policy: ShopPolicyKind::Budget,
        threads,
    }
}

fn csv(config: &SimConfig) -> Vec<u8> {
    let mut octets = Vec::new();
    ecrire_csv(&mut octets, &campagne(config)).expect("sérialisation");
    octets
}

#[test]
fn test_same_seed_same_outcome() {
    let config = cellule(1, 1);
    for seed in [1u64, 7, 4_242] {
        let (premier, aggregats_a) = simulate_one(&config, seed);
        let (second, aggregats_b) = simulate_one(&config, seed);
        // **Champ par champ**, les treize compteurs et la colonne des reliques
        // compris : une comparaison de score seul laisserait passer une dérive
        // de grille ou d'inventaire.
        assert_eq!(premier, second, "graine {seed}");
        assert_eq!(aggregats_a, aggregats_b, "graine {seed}");
    }
}

#[test]
fn test_same_seed_same_csv_regardless_of_threads() {
    let reference = csv(&cellule(1_000, 1));
    for threads in [2usize, 8] {
        // **Comparaison sur les octets**, jamais sur un parsage ligne à ligne,
        // qui masquerait un terminateur ou une espace.
        assert_eq!(
            csv(&cellule(1_000, threads)),
            reference,
            "le tableau diffère à {threads} threads"
        );
    }
    assert!(reference.len() > 1_000, "le tableau est vide");
}

#[test]
fn test_run_index_maps_to_seed() {
    let config = SimConfig {
        cups: vec![CupId::Standard, CupId::Fortune],
        ..cellule(50, 4)
    };
    let resultats = campagne(&config);
    assert_eq!(resultats.len(), 100, "deux cellules de cinquante runs");

    for (rang, (ligne, _)) in resultats.iter().enumerate() {
        let index = u64::try_from(rang % 50).unwrap_or_default();
        assert_eq!(ligne.seed, config.seed_base + index, "rang {rang}");
    }
    // **Les deux cellules rejouent les mêmes graines** : c'est ce qui rend la
    // comparaison entre gobelets appariée plutôt qu'échantillonnée.
    let graines = |tranche: &[(sim_harness::outcome::RunOutcome, _)]| {
        tranche.iter().map(|(l, _)| l.seed).collect::<Vec<u64>>()
    };
    assert_eq!(graines(&resultats[..50]), graines(&resultats[50..]));

    // **Et l'ordre du tableau, pas seulement celui du vecteur.** Mesuré au
    // banc : inverser les lignes à la sérialisation survit à toute assertion
    // portée sur le vecteur, alors que le tableau est le livrable.
    let octets = String::from_utf8(csv(&config)).expect("utf8");
    let colonne: Vec<u64> = octets
        .lines()
        .skip(1)
        .filter_map(|ligne| ligne.split(',').next()?.parse().ok())
        .collect();
    assert_eq!(
        colonne,
        resultats.iter().map(|(l, _)| l.seed).collect::<Vec<u64>>(),
        "le tableau n'est pas dans l'ordre du vecteur"
    );
}

#[test]
fn test_win_rate_is_a_rate_over_non_abandoned_runs() {
    // **Le calibrage masque les deux défauts.** Aucun run ne gagne aujourd'hui,
    // donc un taux qui compterait les victoires au lieu de les rapporter, ou
    // qui inclurait les abandons au dénominateur, rend zéro comme le bon. Il
    // faut des lignes fabriquées.
    let (base, agg) = simulate_one(&cellule(1, 1), 1);
    let ligne = |victory: bool, abandoned: bool| {
        (
            RunOutcome {
                victory,
                abandoned,
                ..base.clone()
            },
            agg.clone(),
        )
    };
    // Une victoire, deux défaites, un abandon : un sur trois retenus.
    let lot = vec![
        ligne(true, false),
        ligne(false, false),
        ligne(false, true),
        ligne(false, false),
    ];
    assert_eq!(taux_de_victoire(&lot), 3_333, "ce n'est pas un taux");
    assert_eq!(
        taux_de_victoire(&[]),
        0,
        "un lot vide ne divise pas par zéro"
    );
    assert_eq!(taux_de_victoire(&[ligne(true, false)]), 10_000);
}

#[test]
fn test_thread_pool_honours_the_requested_count() {
    // Le tableau est identique quel que soit le nombre de fils — c'est ce qu'on
    // exige de lui —, donc rien ne distinguerait un pool qui ignore l'option.
    for demandes in [1usize, 2, 4] {
        assert_eq!(threads_effectifs(demandes), demandes);
    }
}

#[test]
fn test_write_failure_is_reported() {
    // Sans vidage explicite, l'écrivain vide à sa destruction **en ignorant
    // l'erreur** : le tableau paraîtrait écrit et la campagne rendrait zéro.
    struct Bouche;
    impl std::io::Write for Bouche {
        fn write(&mut self, tampon: &[u8]) -> std::io::Result<usize> {
            Ok(tampon.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("disque plein"))
        }
    }
    let resultats = campagne(&cellule(2, 1));
    assert!(
        ecrire_csv(Bouche, &resultats).is_err(),
        "l'échec d'écriture n'est pas remonté"
    );
}

#[test]
fn test_binary_reports_its_verdict() {
    let invoquer = |arguments: &[&str]| {
        let mut ligne = vec!["sim_harness", "--cup", "standard", "--stake", "1"];
        ligne.extend_from_slice(arguments);
        executer(&Cli::parse_from(ligne))
    };

    // **Le code nominal vaut zéro.** Le bouchon rendait deux, et le laisser
    // ferait paraître en échec tout appel non gardé.
    let nominal = invoquer(&["--runs", "5"]);
    assert_eq!(nominal.code, 0);
    assert!(nominal.stderr.is_none());

    // La porte hors fourchette rend un, avec la valeur **et** l'intervalle.
    let refus = invoquer(&["--runs", "20", "--assert-win-rate", "0.25:0.40"]);
    assert_eq!(refus.code, 1);
    let ligne = refus.stderr.unwrap_or_default();
    assert!(ligne.contains("0.25") && ligne.contains("0.4"), "{ligne}");

    // Un drapeau sans effet le dit, plutôt que de laisser croire à un résultat
    // vide.
    let drapeaux = invoquer(&["--runs", "2", "--report", "--trace"]);
    assert_eq!(drapeaux.code, 0);
    let dit = drapeaux.stderr.unwrap_or_default();
    assert!(
        dit.contains("TASK-152") && dit.contains("TASK-153"),
        "{dit}"
    );

    // **Sans chemin de sortie, aucun tableau n'est écrit** : la sortie standard
    // reste libre pour le rapport console.
    let chemin = std::env::temp_dir().join("yamstro-test-151.csv");
    let _ = std::fs::remove_file(&chemin);
    assert_eq!(invoquer(&["--runs", "2"]).code, 0);
    assert!(!chemin.exists());
    assert_eq!(
        invoquer(&["--runs", "2", "--out", &chemin.display().to_string()]).code,
        0
    );
    assert!(chemin.exists(), "le tableau n'a pas été écrit");
    let _ = std::fs::remove_file(&chemin);

    // Un chemin impossible remonte l'échec plutôt que de rendre zéro.
    let impossible = invoquer(&["--runs", "2", "--out", "/impossible/yamstro.csv"]);
    assert_eq!(impossible.code, 1);
    assert!(impossible.stderr.is_some());
}

#[test]
fn test_outcome_order_follows_indices() {
    // L'ordre du vecteur est celui des indices **avant même la sérialisation**.
    let a = campagne(&cellule(200, 1));
    let b = campagne(&cellule(200, 8));
    let graines = |v: &[(sim_harness::outcome::RunOutcome, _)]| {
        v.iter().map(|(l, _)| l.seed).collect::<Vec<u64>>()
    };
    assert_eq!(graines(&a), graines(&b));
    assert_eq!(
        graines(&a),
        (1..=200u64).collect::<Vec<u64>>(),
        "l'ordonnanceur a rendu l'ordre"
    );
}

#[test]
fn test_matrix_cells_are_ordered_by_configuration() {
    let config = SimConfig {
        cups: vec![CupId::Fortune, CupId::Standard],
        stakes: vec![3, 1],
        ..cellule(2, 8)
    };
    let attendu = [
        ("fortune", 3u8),
        ("fortune", 1),
        ("standard", 3),
        ("standard", 1),
    ];
    for essai in 0..3 {
        let resultats = campagne(&config);
        assert_eq!(resultats.len(), 8, "quatre cellules de deux runs");
        let cellules: Vec<(String, u8)> = resultats
            .iter()
            .step_by(2)
            .map(|(ligne, _)| (ligne.cup.clone(), ligne.stake))
            .collect();
        let attendu: Vec<(String, u8)> =
            attendu.iter().map(|(c, s)| ((*c).to_owned(), *s)).collect();
        assert_eq!(cellules, attendu, "essai {essai}");
    }
}

#[test]
fn test_dispatch_is_exhaustive_and_faithful() {
    // **Les cinq politiques sont atteignables, et aucune n'en remplace une
    // autre.** Le nom écrit vient de la politique réellement construite : c'est
    // le seul contrôle qui attrape un bras de `match` mal câblé, qu'il y ait un
    // bras attrape-tout ou non.
    let mut mains: Vec<&'static str> = Vec::new();
    for kind in [
        PolicyKind::Greedy,
        PolicyKind::GridAware,
        PolicyKind::Random,
    ] {
        let config = SimConfig {
            policy: kind,
            ..cellule(1, 1)
        };
        let (ligne, _) = simulate_one(&config, 3);
        assert!(
            !mains.contains(&ligne.policy),
            "{kind:?} rend un nom déjà vu"
        );
        mains.push(ligne.policy);
    }
    assert_eq!(mains.len(), 3);

    let mut achats: Vec<&'static str> = Vec::new();
    for kind in [ShopPolicyKind::Budget, ShopPolicyKind::Synergy] {
        let config = SimConfig {
            shop_policy: kind,
            ..cellule(1, 1)
        };
        let (ligne, _) = simulate_one(&config, 3);
        assert!(
            !achats.contains(&ligne.shop_policy),
            "{kind:?} rend un nom déjà vu"
        );
        achats.push(ligne.shop_policy);
    }
    assert_eq!(achats.len(), 2);
}

#[test]
fn test_each_run_builds_its_own_policies() {
    // Une politique porte un état mutable. Deux runs qui partageraient une
    // instance échangeraient cet état selon l'ordre des threads, et la campagne
    // deviendrait irreproductible **sans qu'aucun run ne soit faux**.
    let config = SimConfig {
        policy: PolicyKind::Random,
        shop_policy: ShopPolicyKind::Synergy,
        ..cellule(300, 8)
    };
    let a = campagne(&config);
    let b = campagne(&SimConfig {
        threads: 1,
        ..config
    });
    assert_eq!(a, b, "l'ordre des threads a changé une décision");

    // Et deux runs voisins d'une même cellule ne se ressemblent pas : sans
    // cela, le test passerait sur une politique inerte.
    let distincts = a
        .iter()
        .map(|(ligne, _)| ligne.max_hand_score)
        .collect::<std::collections::BTreeSet<u64>>();
    assert!(
        distincts.len() > 10,
        "la campagne rend {} valeurs",
        distincts.len()
    );
}

#[test]
fn test_assert_win_rate_exits_one_on_a_real_campaign() {
    let config = cellule(200, 8);
    let resultats = campagne(&config);
    let mesure = taux_de_victoire(&resultats);

    // **La composition est éprouvée, pas seulement ses deux moitiés** : c'est
    // elle que le binaire appelle, et passer la mauvaise grandeur à la
    // fourchette ne se verrait nulle part ailleurs.
    //
    // La fourchette englobante **contient zéro**, et ce n'est pas un choix de
    // confort : au calibrage actuel aucun run ne gagne, et une fourchette qui
    // l'exclurait ferait échouer ce test pour une raison qu'il ne teste pas.
    let dedans = porte(&resultats, WinRateRange { min: 0, max: 1_000 });
    assert_eq!(dedans.code, 0, "mesure {mesure}");
    assert!(dedans.stderr.is_none());
    assert_eq!(verdict(mesure, WinRateRange { min: 0, max: 1_000 }).code, 0);

    let dehors = porte(
        &resultats,
        WinRateRange {
            min: 2_500,
            max: 4_000,
        },
    );
    assert_eq!(dehors.code, 1);
    let ligne = dehors.stderr.unwrap_or_default();
    assert!(
        ligne.contains("0.25"),
        "l'intervalle attendu manque : {ligne}"
    );
    assert!(
        ligne.contains("0.4"),
        "l'intervalle attendu manque : {ligne}"
    );
}

#[test]
fn test_throughput_target() {
    // **Il ne porte pas d'attribut d'exclusion, et c'est délibéré.** Un test
    // ignoré ne tourne jamais, donc ne garde rien. Mesuré : dix mille runs
    // prennent 386 ms sur un cœur en `--release` et 3,2 s en debug, contre un
    // budget de six millisecondes **par run**. La borne ci-dessous laisse donc
    // un facteur dix de marge : elle attrape une régression d'un ordre de
    // grandeur sans pouvoir devenir instable sur une machine partagée.
    let debut = std::time::Instant::now();
    let resultats = campagne(&cellule(10_000, 8));
    let ecoule = debut.elapsed();
    assert_eq!(resultats.len(), 10_000);
    assert!(
        ecoule < std::time::Duration::from_secs(30),
        "dix mille runs ont pris {ecoule:?}"
    );
}
