//! Le journal d'un run : **une preuve, pas un affichage**.
//!
//! Le tableau répond à « qu'est-ce qui s'est passé sur dix mille runs ». Le
//! journal répond à « **pourquoi ce run-là a fait ce score** », et c'est la
//! seule sortie du harnais qu'un humain lit ligne à ligne quand un chiffre du
//! rapport paraît faux. Il est donc **comparable** avant d'être joli : une
//! ligne, un fait ; pas de récapitulatif, pas de tableau, pas d'indentation
//! variable. Deux journaux qui divergent le font à la **première** ligne où le
//! jeu diverge, et pas trois lignes plus loin dans un bloc reformaté.
//!
//! # Rien ne se construit hors du mode, et le type le garantit
//!
//! Une campagne de dix mille runs qui bâtirait un journal par run allouerait
//! des millions de chaînes pour les jeter aussitôt. Le § 2.1 du ticket en fait
//! une condition de tenue du budget de débit, et il a raison — mais un `if` sur
//! l'**affichage** au lieu de la **construction** est un défaut qu'aucun test
//! ne peut observer : une allocation qui n'a pas lieu ne se mesure pas.
//!
//! **La ligne est donc une fermeture, et l'observateur muet ne l'appelle
//! jamais.** Le formatage est effacé à la compilation, pas évité à l'exécution.
//! Ce que la discipline demandait devient une propriété du type.
//!
//! # Comparable veut dire : rien qui change sans que le jeu ait changé
//!
//! Ni horodatage, ni durée, ni adresse, ni chemin absolu, ni numéro de fil.
//! Aucune de ces quantités n'est une propriété du run ; chacune fait diverger
//! deux journaux identiques et rend la comparaison inutilisable dès la première
//! ligne — c'est-à-dire supprime la seule chose que ce mode apporte.
//!
//! Aucune table non ordonnée non plus : l'inventaire se parcourt par slots,
//! dans l'ordre d'application, et les dés par identifiant — jamais par
//! position. À l'Étape 9, *La Meule* retire un dé **en cours de manche** : les
//! positions se décalent, les identifiants non, et un journal indexé par
//! position décrirait alors les mauvais dés, sans erreur ni panique.

use core_engine::dice::DieId;
use core_engine::scoring::{ScoreAction, ScoreStep, StepSource};

/// Ce qui reçoit les lignes du journal.
///
/// **La ligne arrive sous forme de fermeture**, et l'implémentation muette ne
/// l'appelle pas : c'est ce qui rend « aucune chaîne construite hors du mode »
/// vérifiable par la compilation plutôt que par la vigilance du prochain
/// lecteur.
pub trait Observateur {
    fn note<F: FnOnce() -> String>(&mut self, ligne: F);
}

/// L'observateur muet. Aucune ligne, aucune allocation, aucun coût.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Muet;

impl Observateur for Muet {
    fn note<F: FnOnce() -> String>(&mut self, _ligne: F) {}
}

/// Le journal d'un run. **Un seul champ**, comme le document d'étape l'écrit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunTrace {
    pub lines: Vec<String>,
}

impl Observateur for RunTrace {
    fn note<F: FnOnce() -> String>(&mut self, ligne: F) {
        self.lines.push(ligne());
    }
}

impl RunTrace {
    #[must_use]
    pub fn rendu(&self) -> String {
        let mut sortie = String::new();
        for ligne in &self.lines {
            sortie.push_str(ligne);
            sortie.push('\n');
        }
        sortie
    }
}

/// Le multiplicateur, **entier brut d'abord, forme lisible ensuite**.
///
/// Un journal qui n'écrirait que la forme lisible perdrait la précision qui
/// rend la comparaison utile : deux exécutions dont le multiplicateur diffère
/// d'un centième affichent la même valeur à deux décimales, et la comparaison
/// serait **vide sur une divergence réelle** — le journal cesserait d'être une
/// preuve pour devenir une décoration rassurante.
///
/// La forme lisible se produit **par quotient et reste**, jamais par une
/// division flottante : un arrondi qui diffère d'une plateforme à l'autre ferait
/// échouer le critère de reproductibilité en imprimant des chiffres justes.
#[must_use]
pub fn mult_lisible(mult: i64) -> String {
    let signe = if mult < 0 { "-" } else { "" };
    let absolu = mult.unsigned_abs();
    format!("{mult} ({signe}{},{:02})", absolu / 100, absolu % 100)
}

/// Le libellé d'une source de pas.
///
/// **La variante s'appelle `Die`.** Le nom v1 est proscrit sur tout le dépôt,
/// sans exclusion de chemin, et ce fichier est celui où il vient le plus
/// naturellement sous la plume — le journal décrit littéralement le dé qui
/// marque. Une seule occurrence, **fût-elle dans un texte destiné à un
/// humain**, met la CI au rouge.
#[must_use]
pub fn libelle_source(source: StepSource) -> String {
    match source {
        StepSource::HandBase { hand } => format!("HandBase({hand:?})"),
        StepSource::Die { die_id, value } => format!("Die(id={},valeur={value})", die_id.0),
        StepSource::Relic { uid, def } => format!("Relic(uid={uid},{def:?})"),
        StepSource::Seal { die_id, seal } => format!("Seal(id={},{seal:?})", die_id.0),
    }
}

/// Le libellé d'une action, **avec son entier brut**.
///
/// Les deux unités cohabitent dans la même énumération et c'est la confusion la
/// plus coûteuse du module : un ajout se compte en centièmes, une multiplication
/// en pourcentage. Le journal écrit l'entier, jamais une interprétation.
#[must_use]
pub fn libelle_action(action: ScoreAction) -> String {
    match action {
        ScoreAction::AddChips(valeur) => format!("AddChips({valeur})"),
        ScoreAction::AddMult(valeur) => format!("AddMult({valeur})"),
        ScoreAction::MultiplyMult(valeur) => format!("MultiplyMult({valeur})"),
    }
}

/// Une ligne de pas de score, **au format que le test relit**.
///
/// Le format est délibérément lisible par une machine autant que par un
/// humain : c'est ce qui permet au test d'éprouver l'égalité du score sur
/// **chaque** ligne du journal réel, et non sur des valeurs fabriquées à côté.
#[must_use]
pub fn ligne_de_pas(step: &ScoreStep) -> String {
    format!(
        "      pas  source={}  action={}  chips={}  mult={}  score={}",
        libelle_source(step.source),
        libelle_action(step.action),
        step.chips_after,
        mult_lisible(step.mult_after),
        step.score_after
    )
}

/// Le score que la formule du moteur donne pour un couple de valeurs.
///
/// **Arrondi au plus proche et plancher à zéro**, exactement comme le moteur :
/// une forme tronquée diverge dès que le produit n'est pas un multiple de cent,
/// et un multiplicateur négatif rend zéro, jamais un score négatif.
///
/// **Le piège est l'inverse de ce que le ticket annonce.** Il prévoit qu'une
/// égalité tronquée ferait échouer un journal juste « une ligne sur deux ».
/// Mesuré : elle diffère sur **zéro** pas quand aucune relique ne multiplie, et
/// sur **2,3 %** quand trois multiplient — parce qu'un ajout travaille en
/// centièmes entiers et laisse le multiplicateur multiple de cent. La forme
/// tronquée **passerait** donc sur la grande majorité des graines, et le défaut
/// dormirait jusqu'à une relique multiplicative. Seul un cas construit à la
/// main les sépare de façon fiable.
#[must_use]
pub fn score_reconstitue(chips: u64, mult: i64) -> u64 {
    let produit = u128::from(chips).saturating_mul(u128::from(mult.max(0).unsigned_abs()));
    u64::try_from(produit.saturating_add(50) / 100).unwrap_or(u64::MAX)
}

/// Les dés d'une main, **adressés par identifiant**.
#[must_use]
pub fn libelle_des(dice: &[core_engine::dice::Die]) -> String {
    let mut sortie = String::new();
    for die in dice {
        if !sortie.is_empty() {
            sortie.push(' ');
        }
        sortie.push_str(&format!("{}:{}", die.id.0, die.current_value));
    }
    sortie
}

/// Les identifiants **conservés** par un masque de verrouillage.
#[must_use]
pub fn libelle_verrous(ids: impl Iterator<Item = DieId>) -> String {
    let mut sortie = String::new();
    for id in ids {
        if !sortie.is_empty() {
            sortie.push(' ');
        }
        sortie.push_str(&id.0.to_string());
    }
    if sortie.is_empty() {
        "aucun".to_owned()
    } else {
        sortie
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::simulate_one_traced;
    use crate::config::{PolicyKind, ShopPolicyKind, SimConfig};
    use core_engine::cups::CupId;
    use core_engine::dice::{Die, DieId};
    use core_engine::hands::YahtzeeHand;
    use core_engine::relics::RelicId;

    fn cellule(seed: u64) -> SimConfig {
        SimConfig {
            runs: 1,
            seed_base: seed,
            cups: vec![CupId::Standard],
            stakes: vec![1],
            policy: PolicyKind::GridAware,
            shop_policy: ShopPolicyKind::Budget,
            threads: 1,
        }
    }

    fn pas(chips: u64, mult: i64, score: u64) -> ScoreStep {
        ScoreStep {
            source: StepSource::HandBase {
                hand: YahtzeeHand::Chance,
            },
            action: ScoreAction::AddChips(chips),
            chips_after: chips,
            mult_after: mult,
            score_after: score,
        }
    }

    /// Relit une ligne de pas : les trois valeurs que l'égalité compare.
    fn relire(ligne: &str) -> Option<(u64, i64, u64)> {
        let champ = |cle: &str| -> Option<&str> {
            ligne
                .split_whitespace()
                .find_map(|mot| mot.strip_prefix(cle))
        };
        Some((
            champ("chips=")?.parse().ok()?,
            champ("mult=")?.parse().ok()?,
            champ("score=")?.parse().ok()?,
        ))
    }

    #[test]
    fn test_trace_equality_uses_the_rounding_of_final_score() {
        // **Le cas qui sépare les deux formes.** L'arrondi rend 2, la
        // troncature 1 — et c'est le seul contrôle fiable, la troncature
        // passant sur la grande majorité des pas réels (§ du fichier).
        assert_eq!(score_reconstitue(1, 150), 2);
        assert_eq!(150 / 100, 1, "la forme tronquée diverge bien ici");
        assert_eq!(score_reconstitue(1, 149), 1);
        // Plancher : un multiplicateur négatif rend zéro, jamais un score
        // négatif.
        assert_eq!(score_reconstitue(100, -400), 0);
        assert_eq!(score_reconstitue(0, 400), 0);
        // Et la formule du moteur, sur des valeurs plus grandes.
        assert_eq!(score_reconstitue(35, 300), 105);
    }

    #[test]
    fn test_trace_logs_raw_mult() {
        // Deux valeurs voisines rendent **deux lignes différentes** : un
        // journal réduit à deux décimales ne le garantirait pas.
        let a = ligne_de_pas(&pas(10, 612, 61));
        let b = ligne_de_pas(&pas(10, 613, 61));
        assert!(a.contains("mult=612 (6,12)"), "{a}");
        assert!(b.contains("mult=613 (6,13)"), "{b}");
        assert_ne!(a, b, "deux multiplicateurs voisins rendent la même ligne");
        // Le signe est porté par la forme lisible aussi.
        assert_eq!(mult_lisible(-250), "-250 (-2,50)");
        assert_eq!(mult_lisible(0), "0 (0,00)");
        assert_eq!(mult_lisible(5), "5 (0,05)");
    }

    #[test]
    fn test_trace_names_step_source_die() {
        let ligne = ligne_de_pas(&ScoreStep {
            source: StepSource::Die {
                die_id: DieId(2),
                value: 4,
            },
            ..pas(10, 100, 10)
        });
        assert!(ligne.contains("Die(id=2,valeur=4)"), "{ligne}");
        // Les quatre variantes portent leurs champs, et l'identité jamais un
        // libellé venu du moteur.
        let relique = libelle_source(StepSource::Relic {
            uid: 7,
            def: RelicId::PolishedStone,
        });
        assert!(relique.contains("uid=7") && relique.contains("PolishedStone"));
    }

    #[test]
    fn test_trace_addresses_dice_by_id() {
        // Les identifiants, jamais les positions : à l'Étape 9 un boss retire
        // un dé en cours de manche, et les positions se décalent.
        let mut des = vec![Die::new(DieId(4), 6), Die::new(DieId(1), 6)];
        des[0].current_value = 3;
        des[1].current_value = 5;
        assert_eq!(libelle_des(&des), "4:3 1:5");
        assert_eq!(libelle_verrous([DieId(3), DieId(0)].into_iter()), "3 0");
        assert_eq!(libelle_verrous(core::iter::empty()), "aucun");
    }

    #[test]
    fn test_trace_is_only_produced_under_the_flag() {
        // **L'observateur muet ne garde rien, et le résultat est le même.**
        // Une allocation qui n'a pas lieu ne s'observe pas depuis un test :
        // c'est le type qui l'assure, la fermeture de formatage n'étant jamais
        // appelée. Ce qui se vérifie ici est que les deux modes décrivent le
        // même run.
        let config = cellule(77);
        let mut muet = Muet;
        muet.note(|| panic!("l'observateur muet a appelé la fermeture"));
        assert_eq!(muet, Muet);

        let (sans, agregats_sans) = crate::run::simulate_with(
            &config,
            77,
            &mut crate::policy::grid_aware::GridAwarePolicy::new(),
            &mut crate::policy::shop::BudgetShopPolicy::new(),
        );
        let (avec, agregats_avec, journal) = simulate_one_traced(&config, 77);
        assert_eq!(sans, avec, "le journal change le run");
        assert_eq!(agregats_sans, agregats_avec);
        assert!(!journal.lines.is_empty(), "le journal est vide");
    }

    #[test]
    fn test_trace_replays_a_seed() {
        let config = cellule(12_345);
        let (resultat, _, premier) = simulate_one_traced(&config, 12_345);
        let (_, _, second) = simulate_one_traced(&config, 12_345);

        // Premier volet : deux exécutions identiques **octet pour octet**.
        assert_eq!(
            premier.rendu().as_bytes(),
            second.rendu().as_bytes(),
            "deux journaux de même graine diffèrent"
        );
        assert!(premier.lines.len() > 10, "le journal est trop court");

        // **Second volet, et c'est lui qui fait du journal une preuve.**
        // L'égalité du score se vérifie sur **chaque** ligne de pas du journal
        // réel — pas sur des valeurs fabriquées à côté.
        let mut pas_vus = 0u32;
        let mut commits = 0u32;
        let mut dernier_score = 0u64;
        let mut somme = 0u64;
        let mut precedent = 0u64;
        for ligne in &premier.lines {
            if let Some((chips, mult, score)) = relire(ligne) {
                assert_eq!(
                    score_reconstitue(chips, mult),
                    score,
                    "l'égalité du score tombe : {ligne}"
                );
                somme = somme.saturating_add(score.saturating_sub(precedent));
                precedent = score;
                dernier_score = score;
                pas_vus += 1;
            } else if let Some(reste) = ligne.trim().strip_prefix("commit score=") {
                let commis: u64 = reste
                    .split_whitespace()
                    .next()
                    .and_then(|mot| mot.parse().ok())
                    .unwrap_or_default();
                // Le dernier pas de la main égale le score commis…
                assert_eq!(dernier_score, commis, "{ligne}");
                // …et la somme télescopique des deltas l'égale aussi.
                assert_eq!(somme, commis, "somme télescopique : {ligne}");
                somme = 0;
                precedent = 0;
                dernier_score = 0;
                commits += 1;
            }
        }
        assert!(pas_vus > 5, "trop peu de pas relus : {pas_vus}");
        assert!(commits > 0, "aucune main commise dans le journal");
        assert!(!resultat.abandoned);
    }

    #[test]
    fn test_trace_has_one_line_per_score_step() {
        // Autant de lignes de pas que le journal de score en porte, ni
        // fusionnées ni omises.
        let config = cellule(31);
        let (_, _, journal) = simulate_one_traced(&config, 31);
        let pas: Vec<&String> = journal
            .lines
            .iter()
            .filter(|l| l.trim_start().starts_with("pas  "))
            .collect();
        let commits = journal
            .lines
            .iter()
            .filter(|l| l.trim_start().starts_with("commit "))
            .count();
        assert!(commits > 0);
        assert!(
            pas.len() >= commits * 2,
            "une main commise sans ses pas : {} pas pour {commits} commits",
            pas.len()
        );
        // Chaque ligne de pas est relisible : le format est stable.
        for ligne in pas {
            assert!(relire(ligne).is_some(), "ligne illisible : {ligne}");
        }
    }

    #[test]
    fn test_trace_contains_no_volatile_field() {
        // Ni horodatage, ni durée, ni adresse, ni chemin, ni numéro de fil :
        // chacun ferait diverger deux journaux identiques.
        let (_, _, journal) = simulate_one_traced(&cellule(5), 5);
        let texte = journal.rendu();
        for volatil in ["0x", "/Users", "/home", "thread", "ms)", "µs", "202"] {
            assert!(
                !texte.contains(volatil),
                "une valeur volatile est journalisée : {volatil}"
            );
        }
    }

    #[test]
    fn test_trace_lines_are_one_fact_each() {
        // Une ligne, un fait : aucune ligne vide, aucun retour interne, et un
        // préfixe stable par nature de fait. C'est ce qui rend la comparaison
        // exploitable — deux journaux divergent à la **première** ligne où le
        // jeu diverge.
        let (_, _, journal) = simulate_one_traced(&cellule(9), 9);
        for ligne in &journal.lines {
            assert!(!ligne.is_empty(), "une ligne vide");
            assert!(!ligne.contains('\n'), "une ligne en porte deux : {ligne}");
        }
        let prefixes: Vec<&str> = journal
            .lines
            .iter()
            .map(|l| l.split_whitespace().next().unwrap_or(""))
            .collect();
        for attendu in [
            "run", "manche", "main", "jet", "soumet", "pas", "commit", "fin",
        ] {
            assert!(prefixes.contains(&attendu), "aucune ligne « {attendu} »");
        }

        // **Le tirage qui suit une relance est une ligne à part entière.**
        // Mesuré au banc : le renommer ou le supprimer survit à toute assertion
        // sur la présence d'un tirage, le tirage initial en produisant déjà
        // une — le journal pouvait perdre entièrement ce que la relance avait
        // donné sans qu'aucun test ne bronche.
        let gardes = journal
            .lines
            .iter()
            .filter(|l| l.trim_start().starts_with("garde "))
            .count();
        assert!(
            gardes > 0,
            "aucune relance dans ce run : le décor ne prouve rien"
        );
        let jets_de_relance = journal
            .lines
            .iter()
            .filter(|l| l.trim_start().starts_with("jet ") && l.contains("relances="))
            .count();
        assert_eq!(
            jets_de_relance, gardes,
            "une relance sans son tirage, ou l'inverse"
        );
    }
}
