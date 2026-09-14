//! La campagne : l'aiguillage, la section parallèle, et l'ordre de sortie.
//!
//! # Rien ne s'écrit depuis la section parallèle
//!
//! Chaque run tire sa propre graine et construit son propre générateur : il ne
//! dépend **jamais** du nombre de threads ni de l'ordre d'exécution. La section
//! parallèle n'a qu'une sortie, un `collect` sur un vecteur, **indexé** : la
//! bibliothèque de parallélisme restitue l'ordre des indices quel que soit
//! l'ordre d'achèvement, et c'est cette garantie qui remplace un écrivain
//! synchronisé.
//!
//! **Avec un écrivain partagé, chaque run resterait juste** — même graine, même
//! ligne — **et le tableau changerait d'ordre à chaque exécution**. Le critère
//! d'identité octet pour octet deviendrait invérifiable *sans qu'aucun run ne
//! soit faux* : la comparaison nocturne signalerait une dérive chaque nuit, et
//! personne ne saurait dire laquelle des deux exécutions ment. C'est le pire
//! cas possible — un défaut qui ne casse rien et détruit la seule propriété qui
//! rende deux campagnes comparables.
//!
//! **L'intervalle parallélisé porte le type du compte de runs**, et ce détail
//! tient toute la garantie : la bibliothèque n'implémente pas l'accès indexé
//! pour les intervalles de soixante-quatre bits. « Simplifier » la conversion
//! de l'index en élargissant l'intervalle ferait perdre l'ordre sans un mot.
//!
//! # Le budget de débit se compte par cellule
//!
//! La cible est dix mille runs en moins de soixante secondes sur huit cœurs,
//! soit **six millisecondes de temps mural** par run. **Mesuré : trente-huit
//! microsecondes sur un seul cœur** — la cible est tenue cent cinquante-cinq
//! fois avant tout parallélisme. Les deux leviers d'optimisation que le ticket
//! prévoyait — élaguer avant d'évaluer, restreindre aux figures formées — ne
//! sont donc pas implémentés : ce serait de la complexité spéculative contre un
//! budget dépassé de deux ordres de grandeur.
//!
//! Et le compte se lit **par cellule** : la matrice par défaut en porte quinze,
//! donc dix mille runs demandés en font cent cinquante mille joués.
//!
//! **Ne réduis jamais le nombre de runs pour tenir un chrono.** Une campagne de
//! deux mille runs sur quinze cellules donne cent trente runs par cellule : le
//! taux de victoire y a une marge de plusieurs points, les taux de conservation
//! des reliques rares reposent sur quelques dizaines d'observations, et le
//! rapport arbitrerait un seuil de quinze points avec un instrument qui n'en
//! distingue pas dix. Un chrono tenu en rognant l'échantillon produit les mêmes
//! tableaux avec les mêmes colonnes et ne mesure plus rien.

use crate::config::{PolicyKind, ShopPolicyKind, SimConfig, Verdict, WinRateRange, verdict};
use crate::outcome::{RunAggregates, RunOutcome};
use crate::policy::greedy::GreedyPolicy;
use crate::policy::grid_aware::GridAwarePolicy;
use crate::policy::random::RandomPolicy;
use crate::policy::shop::{BudgetShopPolicy, SynergyShopPolicy};
use crate::run::{simulate_with, simulate_with_obs};
use crate::trace::RunTrace;
use rayon::prelude::*;

/// L'unité des taux : **aucun flottant**. Une comparaison flottante fait
/// diverger la porte d'intégration d'une plateforme à l'autre, et la passe
/// nocturne signalerait une dérive inexistante.
const DIX_MILLIEMES: u32 = 10_000;

/// Construit les deux politiques d'après la configuration, puis joue le run.
///
/// **Les deux aiguillages sont exhaustifs, sans bras attrape-tout.** Un bras
/// attrape-tout compile, ne prévient de rien, et fait tourner une option sur
/// une autre sonde : la colonne de politique porterait un nom qui ne correspond
/// pas aux runs, et l'écart entre les deux sondes de main — la seule mesure
/// objective de l'ADR-001 — serait nul pour une raison de câblage. **Un
/// instrument qui rend un résultat sous un mauvais nom est pire qu'un
/// instrument en panne : la panne se voit.** Une sixième politique doit être
/// une erreur de compilation, pas une ligne silencieusement fausse.
///
/// **Les politiques se construisent ici, donc une par run.** Une politique
/// porte un état mutable ; deux runs qui partageraient une instance
/// échangeraient cet état selon l'ordre des threads, et la campagne deviendrait
/// irreproductible sans qu'aucun run ne soit faux.
#[must_use]
pub fn simulate_one(config: &SimConfig, seed: u64) -> (RunOutcome, RunAggregates) {
    match config.shop_policy {
        ShopPolicyKind::Budget => avec_sonde_de_main(config, seed, &mut BudgetShopPolicy::new()),
        ShopPolicyKind::Synergy => avec_sonde_de_main(config, seed, &mut SynergyShopPolicy::new()),
    }
}

/// Le run unique du mode de journal.
///
/// **Il ne passe pas par la section parallèle**, et ce n'est pas un choix : un
/// journal se remplit par emprunt mutable, que la fermeture parallèle ne peut
/// pas capturer. Le mode implique donc un run unique par construction, en plus
/// de l'impliquer par la ligne de commande.
#[must_use]
pub fn simulate_one_traced(config: &SimConfig, seed: u64) -> (RunOutcome, RunAggregates, RunTrace) {
    let mut journal = RunTrace::default();
    let (resultat, agregats) = match config.shop_policy {
        ShopPolicyKind::Budget => {
            trace_avec_sonde(config, seed, &mut BudgetShopPolicy::new(), &mut journal)
        }
        ShopPolicyKind::Synergy => {
            trace_avec_sonde(config, seed, &mut SynergyShopPolicy::new(), &mut journal)
        }
    };
    (resultat, agregats, journal)
}

fn trace_avec_sonde<S: crate::policy::ShopPolicy>(
    config: &SimConfig,
    seed: u64,
    achat: &mut S,
    journal: &mut RunTrace,
) -> (RunOutcome, RunAggregates) {
    match config.policy {
        PolicyKind::Greedy => {
            simulate_with_obs(config, seed, &mut GreedyPolicy::new(), achat, journal)
        }
        PolicyKind::GridAware => {
            simulate_with_obs(config, seed, &mut GridAwarePolicy::new(), achat, journal)
        }
        PolicyKind::Random => {
            simulate_with_obs(config, seed, &mut RandomPolicy::new(), achat, journal)
        }
    }
}

fn avec_sonde_de_main<S: crate::policy::ShopPolicy>(
    config: &SimConfig,
    seed: u64,
    achat: &mut S,
) -> (RunOutcome, RunAggregates) {
    match config.policy {
        PolicyKind::Greedy => simulate_with(config, seed, &mut GreedyPolicy::new(), achat),
        PolicyKind::GridAware => simulate_with(config, seed, &mut GridAwarePolicy::new(), achat),
        PolicyKind::Random => simulate_with(config, seed, &mut RandomPolicy::new(), achat),
    }
}

/// Les cellules de la matrice, **dans l'ordre du produit cartésien** des deux
/// vecteurs de la configuration.
///
/// Chacune porte **un seul** gobelet et **une seule** mise : la boucle de
/// manche ne lit que le premier de chaque vecteur, et lui passer la
/// configuration entière ferait tourner toute la campagne sur la première
/// cellule — en silence, avec un tableau parfaitement crédible.
#[must_use]
pub fn cellules(config: &SimConfig) -> Vec<SimConfig> {
    let mut cellules = Vec::with_capacity(config.cups.len() * config.stakes.len());
    for cup in &config.cups {
        for stake in &config.stakes {
            cellules.push(SimConfig {
                cups: vec![*cup],
                stakes: vec![*stake],
                ..config.clone()
            });
        }
    }
    cellules
}

/// Joue la campagne entière : les cellules en séquence, les runs d'une cellule
/// en parallèle.
///
/// **Chaque cellule rejoue les mêmes graines.** Deux gobelets se comparent alors
/// sur les mêmes graines, et l'écart entre eux ne mélange plus l'effet du
/// gobelet et le tirage de l'échantillon — ce que le seuil d'écart entre
/// gobelets exige, faute de quoi il arbitrerait sur du bruit.
#[must_use]
pub fn campagne(config: &SimConfig) -> Vec<(RunOutcome, RunAggregates)> {
    dans_le_pool(config.threads, || toutes_les_cellules(config))
}

/// Exécute le travail dans un pool **local**, jamais dans le pool global muté :
/// deux campagnes du même processus ne doivent pas se marcher dessus. L'échec
/// de construction se replie sur le pool par défaut plutôt que de faire échouer
/// la campagne.
fn dans_le_pool<T: Send>(threads: usize, travail: impl FnOnce() -> T + Send) -> T {
    match rayon::ThreadPoolBuilder::new().num_threads(threads).build() {
        Ok(pool) => pool.install(travail),
        Err(_) => travail(),
    }
}

/// Le nombre de fils que le pool met réellement au travail.
///
/// **Sans cette lecture, rien ne distingue un pool correctement dimensionné
/// d'un pool qui ignore l'option** : le tableau reste identique d'un nombre de
/// fils à l'autre — c'est même ce qu'on exige de lui —, donc le seul symptôme
/// serait un temps mural, que nul test ne peut assérer sans devenir instable.
#[must_use]
pub fn threads_effectifs(threads: usize) -> usize {
    dans_le_pool(threads, rayon::current_num_threads)
}

fn toutes_les_cellules(config: &SimConfig) -> Vec<(RunOutcome, RunAggregates)> {
    let mut resultats = Vec::new();
    for cellule in cellules(config) {
        let results: Vec<(RunOutcome, RunAggregates)> = (0..cellule.runs)
            .into_par_iter()
            .map(|index| simulate_one(&cellule, cellule.seed_base + index as u64))
            .collect(); // collect indexé : l'ordre est celui des indices
        resultats.extend(results);
    }
    resultats
}

/// Écrit le tableau, **après** la section parallèle et jamais dedans.
///
/// Seule la ligne part au fichier ; le second canal reste en mémoire pour le
/// rapport. Le terminateur est celui que la bibliothèque pose par défaut — un
/// saut de ligne, mesuré, et non le couple que son énumération déclare par
/// défaut de son côté.
pub fn ecrire_csv<W: std::io::Write>(
    sortie: W,
    resultats: &[(RunOutcome, RunAggregates)],
) -> Result<(), String> {
    let mut writer = csv::Writer::from_writer(sortie);
    for (outcome, _) in resultats {
        writer
            .serialize(outcome)
            .map_err(|erreur| format!("écriture du tableau : {erreur}"))?;
    }
    writer
        .flush()
        .map_err(|erreur| format!("vidage du tableau : {erreur}"))
}

/// Le taux de victoire, **en dix-millièmes entiers**.
///
/// Les runs abandonnés n'entrent pas dans le dénominateur : un défaut de
/// politique n'est pas une défaite du joueur, et le compter comme telle ferait
/// baisser le taux pour une raison qui n'est pas de jouabilité.
#[must_use]
pub fn taux_de_victoire(resultats: &[(RunOutcome, RunAggregates)]) -> u32 {
    let retenus = resultats
        .iter()
        .filter(|(ligne, _)| !ligne.abandoned)
        .count();
    let victoires = resultats.iter().filter(|(ligne, _)| ligne.victory).count();
    u32::try_from(victoires.saturating_mul(DIX_MILLIEMES as usize) / retenus.max(1))
        .unwrap_or(DIX_MILLIEMES)
}

/// La porte d'intégration : le taux mesuré, confronté à la fourchette.
///
/// **C'est le cœur éprouvable**, et le binaire ne fait que convertir son code
/// en code de sortie. Sans lui, la composition — mesurer, puis comparer — ne
/// serait vérifiable qu'en lançant un sous-processus, et un code de sortie ne
/// se décompose pas.
#[must_use]
pub fn porte(resultats: &[(RunOutcome, RunAggregates)], fourchette: WinRateRange) -> Verdict {
    verdict(taux_de_victoire(resultats), fourchette)
}
