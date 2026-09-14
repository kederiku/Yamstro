//! Le rapport agrégé : **des chiffres, et aucune conclusion**.
//!
//! Il lit les **deux canaux** d'une campagne. Le taux de victoire, l'ante de
//! défaite et les anomalies se lisent sur la ligne de sortie ; le rapport score
//! sur cible par ante et les trois indicateurs par relique se lisent sur les
//! agrégats. Aucune de ces quatre grandeurs n'est reconstituée depuis une
//! colonne du tableau : elles n'y sont pas, et un chiffre bâti sur les colonnes
//! serait plausible, stable, reproductible — et faux.
//!
//! # Rien en flottant, nulle part, pas même à l'affichage
//!
//! L'addition flottante n'est **pas associative** et l'ordre de réduction d'un
//! agrégat parallèle n'est pas garanti : deux architectures, ou deux nombres de
//! fils, peuvent produire deux derniers bits différents. Le tableau resterait
//! juste, chaque run resterait juste, et **seule la ligne de résumé
//! changerait** — la comparaison d'empreintes de la passe nocturne signalerait
//! une dérive que personne ne saurait attribuer.
//!
//! Les taux se comptent donc en **dix-millièmes entiers**, la multiplication
//! **avant** la division, sur un dénominateur protégé. L'affichage lui-même se
//! fait par quotient et reste.
//!
//! # Les médianes sont des statistiques d'ordre
//!
//! Une moyenne s'écrit toute seule et rend un chiffre plausible. Elle est
//! fausse **dans une direction connue** : la queue de runs perdus tôt la tire
//! vers le bas, et l'ante de défaite « médian » sortirait de sa fourchette sans
//! qu'aucune valeur du jeu n'ait bougé. **Une seule fonction de médiane** dans
//! ce fichier, appelée par ses trois usages ; trois implémentations donneraient
//! trois conventions au premier remaniement.
//!
//! # Aucune date, et c'est une correction
//!
//! Le document d'étape fait dater la ligne d'indisponibilité des mises. **Une
//! date lue à l'horloge fait changer l'empreinte du rapport à chaque
//! exécution**, par construction — or c'est cette empreinte que la passe
//! nocturne compare, rapport archivé compris, et l'alerte de dérive se
//! déclencherait chaque nuit sur la seule ligne qui ne mesure rien. Ce que la
//! date veut dire est déjà porté par le **numéro d'étape** que la ligne nomme,
//! qui est plus précis et ne bouge pas. Le moment d'une campagne appartient au
//! journal de la passe nocturne, ou au **nom** du fichier archivé — un nom
//! n'entre pas dans l'empreinte.

use crate::config::{SimConfig, cup_nom};
use crate::outcome::{ANTES, RunAggregates, RunOutcome};
use core_engine::cups::CupId;
use core_engine::relics::{CATALOG, RelicId, RelicRarity, rarity_of};

/// L'unité des taux : **aucun flottant, jamais**.
pub const DIX_MILLIEMES: u32 = 10_000;

/// Les quatre cibles du document d'étape, **déclarées une fois chacune**. Un
/// seuil recopié à trois endroits se désaligne au premier calibrage — et c'est
/// justement le calibrage que cette étape existe pour produire.
pub const VICTOIRE_MIN: u32 = 2_500;
pub const VICTOIRE_MAX: u32 = 4_000;
/// Quinze points de pourcentage, en dix-millièmes.
pub const ECART_GOBELETS_MAX: u32 = 1_500;
pub const CONSERVATION_MIN: u32 = 1_000;
pub const CONSERVATION_MAX: u32 = 8_000;
pub const ANTE_DEFAITE_MIN: u8 = 6;
pub const ANTE_DEFAITE_MAX: u8 = 7;
/// La fourchette de contribution d'une relique autour de la médiane de sa
/// rareté, en millièmes : de la moitié au double.
pub const CONTRIBUTION_MIN_MILLIEMES: u64 = 500;
pub const CONTRIBUTION_MAX_MILLIEMES: u64 = 2_000;

/// Un signe court et fixe, jamais une couleur ni un pictogramme : un rapport
/// doit se diffuser par copier-coller et se comparer d'une nuit à l'autre.
const MARQUEUR: &str = "<<";
/// Ce qu'affiche une cible dont le **dénominateur est nul**. Une cible verte
/// par absence de mesure est pire qu'une cible rouge.
const SANS_MESURE: &str = "non mesurable";

/// Une cellule de la matrice.
pub type Cellule = (CupId, u8);

/// Les trois indicateurs d'une relique, et sa contribution moyenne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RelicStats {
    pub offered: u32,
    pub bought: u32,
    pub kept: u32,
    pub mean_contribution: u64,
}

impl RelicStats {
    /// À quelle fréquence le joueur la voit.
    #[must_use]
    pub fn taux_apparition(&self, runs: u32) -> u32 {
        taux(self.offered, runs)
    }

    /// Quand il la voit, la prend-il.
    #[must_use]
    pub fn taux_achat(&self) -> u32 {
        taux(self.bought, self.offered)
    }

    /// Parmi les runs **qui l'ont vue**, la possède-t-il encore à la fin.
    ///
    /// **Le dénominateur est le nombre de runs qui l'ont vue, pas achetée.**
    /// Rapporté aux achats, une relique proposée cent fois et achetée trois
    /// fois afficherait un taux flatteur sur trois observations.
    #[must_use]
    pub fn taux_conservation(&self) -> u32 {
        taux(self.kept, self.offered)
    }

    /// Vrai quand la relique a été achetée sans jamais produire un seul pas de
    /// score.
    ///
    /// **Ce n'est pas « sans valeur », c'est « hors de portée de cet
    /// instrument ».** La contribution se somme sur le journal de score ; une
    /// relique qui agit sur l'**or de fin de manche** ou sur le **lancer** n'y
    /// paraît jamais. Mesuré : deux des douze reliques livrées sont dans ce
    /// cas, dont une achetée près de neuf cents fois sur deux mille runs. Sur
    /// autant d'achats, un zéro n'est pas un accident de mesure, il est
    /// structurel — et le lire comme une mesure de puissance ferait corriger
    /// deux reliques qui fonctionnent.
    #[must_use]
    pub fn hors_du_pipeline(&self) -> bool {
        self.bought > 0 && self.mean_contribution == 0
    }
}

/// Ce qu'une campagne rend, agrégé.
///
/// **Les cinq premiers champs portent les noms normatifs du document
/// d'étape.** Le type de la table des reliques est amendé — l'identifiant de
/// relique ne dérive pas l'ordre, et une table ordonnée exigerait une clé
/// ordonnable ; le manque est inscrit au fichier des manques d'API,
/// propriétaire Étape 2, et **le harnais n'ajoute pas le dérivé** : ce serait
/// une ligne du moteur, donc la règle n°1 violée, et l'ordre de déclaration
/// deviendrait normatif sans que personne l'ait décidé.
///
/// Les cinq suivants sont ce que le rendu exige et que les cinq premiers ne
/// portent pas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceReport {
    pub win_rate_by_cup: Vec<(CupId, u32)>,
    pub win_rate_by_stake: Vec<(u8, u32)>,
    pub median_defeat_ante: u8,
    pub score_vs_target_by_ante: [u32; ANTES],
    pub relics: Vec<(RelicId, RelicStats)>,
    /// Le taux par **cellule** : « à mise fixée » n'a pas de sens sur une
    /// matrice à plusieurs mises, et agréger les mises mélangerait des
    /// conditions que le seuil d'écart entre gobelets veut séparer.
    pub win_rate_by_cell: Vec<(Cellule, u32)>,
    /// Le nombre de runs **ayant atteint** chaque ante.
    ///
    /// Sans lui, une médiane sur vingt-quatre runs se lit comme une médiane sur
    /// deux mille.
    pub observations_by_ante: [u32; ANTES],
    pub runs: u32,
    /// Runs marqués abandonnés : ils n'entrent dans **aucun** taux, et leur
    /// compte s'affiche en tête. Un compte non nul invalide la campagne ; il ne
    /// se commente pas, il s'affiche.
    pub abandoned: u32,
    /// Défauts de politique rencontrés, **sommés sur tous les runs**.
    ///
    /// Ce n'est **pas** le compte d'abandons : un run peut porter une anomalie
    /// sans être abandonné, et les deux se lisent séparément. Mesuré, ils
    /// coïncident au calibrage actuel — le repli d'une sonde sans figure
    /// jouable termine le run —, et les confondre ferait croire à une identité
    /// qui n'est pas garantie.
    pub anomalies: u32,
    /// La mise sur laquelle porte le taux par gobelet, nommée dans l'intitulé.
    pub reference_stake: u8,
}

/// Un taux en dix-millièmes, **la multiplication avant la division**.
///
/// Diviser d'abord rendrait zéro dans presque tous les cas en arithmétique
/// entière — un taux d'achat de quarante pour cent affiché à zéro, sans erreur
/// et parfaitement crédible pour une relique impopulaire.
#[must_use]
pub fn taux(numerateur: u32, denominateur: u32) -> u32 {
    numerateur
        .saturating_mul(DIX_MILLIEMES)
        .checked_div(denominateur)
        .unwrap_or(0)
}

/// La médiane **basse** d'un vecteur, convention unique du harnais.
///
/// Vecteur trié, élément d'indice `(n - 1) / 2`. La moyenne des deux valeurs
/// centrales sur une longueur paire est écartée : elle réintroduit une division
/// qui n'est plus une statistique d'ordre, et n'apporte rien à la lecture d'un
/// ante. Vecteur vide : zéro, signalé comme absence d'observation, jamais une
/// panique.
///
/// **La signature prend une tranche, et c'est un amendement déclaré.** Le
/// document d'étape écrit un vecteur mutable ; clippy le refuse sous
/// `-D warnings` — un vecteur mutable là où une tranche suffit impose un objet
/// à l'appelant sans rien lui rendre. Les appels ne changent pas d'un
/// caractère, la conversion étant automatique.
#[must_use]
pub fn median(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    values
        .get(values.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(0)
}

/// Agrège une campagne. Les runs marqués abandonnés sont **écartés de tous les
/// taux** et comptés à part.
#[must_use]
pub fn agreger(config: &SimConfig, resultats: &[(RunOutcome, RunAggregates)]) -> BalanceReport {
    let retenus: Vec<&(RunOutcome, RunAggregates)> = resultats
        .iter()
        .filter(|(ligne, _)| !ligne.abandoned)
        .collect();
    let reference_stake = config.stakes.first().copied().unwrap_or(1);

    // Les cellules, dans l'ordre du produit cartésien — le même que celui du
    // tableau de sortie. Le regroupement se fait par le nom du gobelet et la
    // mise portés par la ligne, jamais par un découpage d'indices.
    let mut win_rate_by_cell: Vec<(Cellule, u32)> = Vec::new();
    for cup in &config.cups {
        for stake in &config.stakes {
            win_rate_by_cell.push((
                (*cup, *stake),
                taux_sur(&retenus, |ligne| {
                    ligne.cup == cup_nom(*cup) && ligne.stake == *stake
                }),
            ));
        }
    }

    let win_rate_by_cup: Vec<(CupId, u32)> = config
        .cups
        .iter()
        .map(|cup| {
            (
                *cup,
                taux_sur(&retenus, |ligne| {
                    ligne.cup == cup_nom(*cup) && ligne.stake == reference_stake
                }),
            )
        })
        .collect();

    let win_rate_by_stake: Vec<(u8, u32)> = config
        .stakes
        .iter()
        .map(|stake| (*stake, taux_sur(&retenus, |ligne| ligne.stake == *stake)))
        .collect();

    let mut antes: Vec<u64> = retenus
        .iter()
        .filter(|(ligne, _)| !ligne.victory)
        .filter_map(|(ligne, _)| ligne.defeat_ante.map(u64::from))
        .collect();
    let median_defeat_ante = u8::try_from(median(&mut antes)).unwrap_or(u8::MAX);

    // **La médiane porte sur les runs qui ont atteint l'ante.** Sur tous les
    // runs, les zéros des antes jamais entrés dominent dès le second : la
    // mesure montrerait une falaise qui n'est qu'une absence d'entrée.
    let mut score_vs_target_by_ante = [0u32; ANTES];
    let mut observations_by_ante = [0u32; ANTES];
    for (index, case) in score_vs_target_by_ante.iter_mut().enumerate() {
        let ante = u8::try_from(index.saturating_add(1)).unwrap_or(u8::MAX);
        let mut entres: Vec<u64> = retenus
            .iter()
            .filter(|(ligne, _)| ligne.ante_reached >= ante)
            .map(|(_, agregats)| u64::from(agregats.score_vs_target_by_ante[index]))
            .collect();
        observations_by_ante[index] = u32::try_from(entres.len()).unwrap_or(u32::MAX);
        *case = u32::try_from(median(&mut entres)).unwrap_or(u32::MAX);
    }

    // **Le tableau itère le catalogue**, jamais une liste écrite à la main :
    // douze reliques aujourd'hui, soixante à l'Étape 9, sans qu'une ligne
    // change. Une relique jamais proposée y figure à zéro — son absence serait
    // une information perdue, et c'est l'une des plus utiles.
    let relics: Vec<(RelicId, RelicStats)> = CATALOG
        .iter()
        .copied()
        .map(|def| (def, statistiques_de(&retenus, def)))
        .collect();

    BalanceReport {
        win_rate_by_cup,
        win_rate_by_stake,
        median_defeat_ante,
        score_vs_target_by_ante,
        relics,
        win_rate_by_cell,
        observations_by_ante,
        runs: u32::try_from(resultats.len()).unwrap_or(u32::MAX),
        abandoned: u32::try_from(resultats.len().saturating_sub(retenus.len())).unwrap_or(u32::MAX),
        anomalies: resultats.iter().fold(0u32, |total, (ligne, _)| {
            total.saturating_add(ligne.anomalies)
        }),
        reference_stake,
    }
}

/// Le taux de victoire d'un sous-ensemble, en dix-millièmes.
fn taux_sur(retenus: &[&(RunOutcome, RunAggregates)], dans: impl Fn(&RunOutcome) -> bool) -> u32 {
    let (mut total, mut victoires) = (0u32, 0u32);
    for (ligne, _) in retenus.iter().filter(|(ligne, _)| dans(ligne)) {
        total = total.saturating_add(1);
        if ligne.victory {
            victoires = victoires.saturating_add(1);
        }
    }
    taux(victoires, total)
}

/// Les trois indicateurs d'une relique, additionnés sur les runs.
///
/// Ils sont des **booléens de run** à la source : une relique proposée deux
/// fois dans le même run compte pour une, et cette addition ne peut donc pas
/// compter deux fois le même run.
fn statistiques_de(retenus: &[&(RunOutcome, RunAggregates)], def: RelicId) -> RelicStats {
    let mut stats = RelicStats::default();
    let mut contribution = 0u64;
    for (_, agregats) in retenus {
        let Some(rencontre) = agregats.relics_seen.iter().find(|e| e.def == def) else {
            continue;
        };
        if rencontre.offered {
            stats.offered = stats.offered.saturating_add(1);
        }
        if rencontre.bought {
            stats.bought = stats.bought.saturating_add(1);
            contribution = contribution.saturating_add(rencontre.contribution);
        }
        if rencontre.kept {
            stats.kept = stats.kept.saturating_add(1);
        }
    }
    // **La seule moyenne du rapport**, et son dénominateur s'affiche à côté du
    // chiffre : une contribution moyenne sur trois achats et une sur neuf cents
    // ne se lisent pas de la même façon.
    stats.mean_contribution = contribution
        .checked_div(u64::from(stats.bought))
        .unwrap_or(0);
    stats
}

/// Le premier ante dont la médiane passe sous l'unité — **le mur réel de la
/// run**. Un ante sans observation n'en est pas un : il n'a pas été joué.
#[must_use]
pub fn premier_ante_sous_un(rapport: &BalanceReport) -> Option<u8> {
    rapport
        .score_vs_target_by_ante
        .iter()
        .zip(rapport.observations_by_ante.iter())
        .position(|(ratio, observations)| *observations > 0 && *ratio < DIX_MILLIEMES)
        .and_then(|index| u8::try_from(index.saturating_add(1)).ok())
}

/// La médiane des contributions moyennes d'un palier de rareté.
#[must_use]
pub fn mediane_du_palier(rapport: &BalanceReport, rarity: RelicRarity) -> u64 {
    let mut valeurs: Vec<u64> = rapport
        .relics
        .iter()
        .filter(|(def, stats)| rarity_of(*def) == rarity && stats.bought > 0)
        .map(|(_, stats)| stats.mean_contribution)
        .collect();
    median(&mut valeurs)
}

/// Le rendu console : texte aligné, aucune couleur, aucun tableau à balises.
#[must_use]
pub fn rendu(rapport: &BalanceReport) -> String {
    let mut sortie = String::new();
    let mut ligne = |texte: &str| {
        sortie.push_str(texte);
        sortie.push('\n');
    };

    ligne("== rapport d'équilibrage ==");
    ligne(&format!(
        "  runs           : {}   retenus : {}   abandons : {}",
        rapport.runs,
        rapport.runs.saturating_sub(rapport.abandoned),
        rapport.abandoned
    ));
    // **Un compte non nul invalide la campagne** : il ne se commente pas, il
    // s'affiche. Les deux grandeurs sont distinctes — un run peut porter une
    // anomalie sans être abandonné.
    ligne(&format!(
        "  anomalies      : {}{}",
        rapport.anomalies,
        marque(rapport.anomalies > 0)
    ));

    ligne("");
    ligne("-- taux de victoire par cellule --");
    for ((cup, stake), taux) in &rapport.win_rate_by_cell {
        ligne(&format!(
            "  {:12} mise {stake}   {:>9}{}",
            cup_nom(*cup),
            pourcentage(*taux),
            marque(!(VICTOIRE_MIN..=VICTOIRE_MAX).contains(taux))
        ));
    }

    ligne("");
    ligne(&format!(
        "-- taux de victoire par gobelet, à mise {} --",
        rapport.reference_stake
    ));
    for (cup, taux) in &rapport.win_rate_by_cup {
        ligne(&format!("  {:12} {:>9}", cup_nom(*cup), pourcentage(*taux)));
    }
    let ecart = ecart_entre_gobelets(rapport);
    ligne(&match ecart {
        // **Une cible dont le dénominateur est nul ne se marque pas conforme.**
        // Cinq gobelets à zéro victoire rendent un écart de zéro, et la cible
        // passerait sans rien avoir mesuré.
        None => format!("  écart          : {SANS_MESURE} (aucune victoire)"),
        Some(points) => format!(
            "  écart          : {:>9}{}",
            pourcentage(points),
            marque(points > ECART_GOBELETS_MAX)
        ),
    });

    ligne("");
    ligne("-- taux de victoire par mise --");
    for (stake, taux) in &rapport.win_rate_by_stake {
        ligne(&format!("  mise {stake}         {:>9}", pourcentage(*taux)));
    }
    ligne("  progression des six mises : non mesurable avant l'Étape 10");

    ligne("");
    ligne(&format!(
        "-- ante médian de défaite : {}{} --",
        rapport.median_defeat_ante,
        marque(!(ANTE_DEFAITE_MIN..=ANTE_DEFAITE_MAX).contains(&rapport.median_defeat_ante))
    ));

    ligne("");
    ligne("-- rapport score / cible par ante --");
    for (index, ratio) in rapport.score_vs_target_by_ante.iter().enumerate() {
        let observations = rapport.observations_by_ante[index];
        let ante = index.saturating_add(1);
        // **Le compte d'observations, à côté du chiffre.** Sans lui, une
        // médiane sur vingt-quatre runs se lit comme une médiane sur deux mille.
        ligne(&if observations == 0 {
            format!("  ante {ante}         {SANS_MESURE} (aucun run n'y parvient)")
        } else {
            format!(
                "  ante {ante}         {:>9}   sur {observations} runs",
                pourcentage(*ratio)
            )
        });
    }
    ligne(&match premier_ante_sous_un(rapport) {
        None => "  premier ante sous 1,00 : aucun".to_owned(),
        Some(ante) => format!("  premier ante sous 1,00 : {ante}   (le mur de la run)"),
    });

    ligne("");
    ligne("-- reliques : apparition / achat / conservation --");
    ligne("  (la conservation se rapporte aux runs qui l'ont VUE, pas achetée)");
    for (def, stats) in &rapport.relics {
        let conservation = stats.taux_conservation();
        let contribution: String = if stats.hors_du_pipeline() {
            // Hors de portée de cet instrument, et non « sans valeur » : l'or
            // de fin de manche et le modificateur de lancer ne passent pas par
            // le journal de score.
            "—".to_owned()
        } else {
            stats.mean_contribution.to_string()
        };
        ligne(&format!(
            "  {:22} {:>8} {:>8} {:>8}{:2}  contrib {:>6} sur {} achats",
            format!("{def:?}"),
            pourcentage(stats.taux_apparition(rapport.runs.saturating_sub(rapport.abandoned))),
            pourcentage(stats.taux_achat()),
            pourcentage(conservation),
            marque(
                stats.offered > 0 && !(CONSERVATION_MIN..=CONSERVATION_MAX).contains(&conservation)
            ),
            contribution,
            stats.bought
        ));
    }

    ligne("");
    ligne("-- contribution médiane par palier --");
    ligne("  hypothèse du document d'étape : 1 : 2 : 3,75 — observé ci-dessous");
    for palier in PALIERS {
        let mediane = mediane_du_palier(rapport, palier);
        ligne(&if mediane == 0 {
            format!("  {:14} {SANS_MESURE} (aucune achetée)", palier_nom(palier))
        } else {
            format!("  {:14} {mediane}", palier_nom(palier))
        });
    }

    sortie
}

/// L'écart de taux de victoire entre gobelets, à mise de référence.
///
/// **Rend l'absence de mesure quand aucun gobelet ne gagne** : un écart de zéro
/// entre cinq zéros tiendrait la cible sans rien avoir mesuré.
#[must_use]
pub fn ecart_entre_gobelets(rapport: &BalanceReport) -> Option<u32> {
    let taux: Vec<u32> = rapport.win_rate_by_cup.iter().map(|(_, t)| *t).collect();
    if taux.iter().all(|t| *t == 0) {
        return None;
    }
    let haut = taux.iter().max().copied().unwrap_or(0);
    let bas = taux.iter().min().copied().unwrap_or(0);
    Some(haut.saturating_sub(bas))
}

/// Un taux en pourcentage, **par quotient et reste**.
#[must_use]
pub fn pourcentage(dix_milliemes: u32) -> String {
    format!("{},{:02} %", dix_milliemes / 100, dix_milliemes % 100)
}

/// Le marqueur de dépassement, appliqué par une fonction unique.
#[must_use]
pub fn marque(hors_cible: bool) -> &'static str {
    if hors_cible { MARQUEUR } else { "" }
}

/// Les paliers de rareté, dans l'ordre où le rapport les présente.
const PALIERS: [RelicRarity; 4] = [
    RelicRarity::Common,
    RelicRarity::Uncommon,
    RelicRarity::Rare,
    RelicRarity::Legendary,
];

#[must_use]
fn palier_nom(rarity: RelicRarity) -> &'static str {
    match rarity {
        RelicRarity::Common => "commune",
        RelicRarity::Uncommon => "peu commune",
        RelicRarity::Rare => "rare",
        RelicRarity::Legendary => "légendaire",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::campagne;
    use crate::config::{PolicyKind, ShopPolicyKind};
    use crate::outcome::RelicEncounter;

    fn cellule(cup: CupId, stake: u8, runs: u32) -> SimConfig {
        SimConfig {
            runs,
            seed_base: 1,
            cups: vec![cup],
            stakes: vec![stake],
            policy: PolicyKind::GridAware,
            shop_policy: ShopPolicyKind::Budget,
            threads: 8,
        }
    }

    fn un_lot(runs: u32) -> (SimConfig, Vec<(RunOutcome, RunAggregates)>) {
        let config = cellule(CupId::Standard, 1, runs);
        let resultats = campagne(&config);
        (config, resultats)
    }

    /// Une ligne fabriquée, pour éprouver ce que le calibrage masque.
    fn ligne(base: &RunOutcome, victory: bool, abandoned: bool, ante: u8) -> RunOutcome {
        RunOutcome {
            victory,
            abandoned,
            defeat_ante: (!victory).then_some(ante),
            ante_reached: ante,
            ..base.clone()
        }
    }

    #[test]
    fn test_median_is_an_order_statistic() {
        // **La seule raison d'être de ce test** : la moyenne rendrait 25, puis
        // 3, et le test échouerait. Une queue de runs perdus tôt tire la
        // moyenne vers le bas, et l'ante de défaite sortirait de sa fourchette
        // sans qu'aucune valeur du jeu n'ait bougé.
        assert_eq!(median(&mut [1, 1, 1, 100]), 1, "moyenne : 25");
        assert_eq!(median(&mut [2, 4]), 2, "médiane basse, moyenne : 3");
        assert_eq!(median(&mut []), 0, "une tranche vide ne panique pas");
        assert_eq!(median(&mut [7]), 7);
        // Et elle trie : l'ordre d'entrée ne change rien.
        assert_eq!(median(&mut [100, 1, 1, 1]), 1);
    }

    #[test]
    fn test_report_uses_integer_arithmetic() {
        // Une victoire sur trois vaut 3 333 dix-millièmes, et la valeur ne
        // dépend pas de l'ordre d'agrégation.
        assert_eq!(taux(1, 3), 3_333);
        assert_eq!(taux(40, 100), 4_000);
        assert_eq!(taux(0, 0), 0, "un dénominateur nul ne panique pas");
        assert_eq!(taux(1, 1), DIX_MILLIEMES);

        // **La multiplication avant la division.** L'inverse rendrait zéro dans
        // presque tous les cas : un taux d'achat de quarante pour cent affiché
        // à zéro, sans erreur, et parfaitement crédible.
        assert_eq!(taux(3, 8), 3_750);
        assert_ne!(taux(3, 8), 0);

        // Le formatage aussi est entier.
        assert_eq!(pourcentage(2_537), "25,37 %");
        assert_eq!(pourcentage(0), "0,00 %");
        assert_eq!(pourcentage(DIX_MILLIEMES), "100,00 %");

        let (config, resultats) = un_lot(200);
        let direct = agreger(&config, &resultats);
        let mut inverse = resultats.clone();
        inverse.reverse();
        assert_eq!(
            direct.win_rate_by_cup,
            agreger(&config, &inverse).win_rate_by_cup,
            "le taux dépend de l'ordre d'agrégation"
        );
    }

    #[test]
    fn test_three_relic_rates_are_distinct() {
        // Cent runs la voient, quarante l'achètent, dix la gardent.
        let stats = RelicStats {
            offered: 100,
            bought: 40,
            kept: 10,
            mean_contribution: 7,
        };
        assert_eq!(stats.taux_apparition(200), 5_000);
        assert_eq!(stats.taux_achat(), 4_000);
        // **Parmi les runs qui l'ont vue**, jamais parmi celles qui l'ont
        // achetée : rapporté aux achats, ce serait 2 500.
        assert_eq!(stats.taux_conservation(), 1_000);
        assert_ne!(stats.taux_conservation(), 2_500);

        // Trois dénominateurs différents, donc trois valeurs différentes.
        let trois = [
            stats.taux_apparition(200),
            stats.taux_achat(),
            stats.taux_conservation(),
        ];
        for (rang, valeur) in trois.iter().enumerate() {
            assert!(!trois[..rang].contains(valeur), "deux taux coïncident");
        }
    }

    #[test]
    fn test_targets_are_marked() {
        // **Les bornes sont incluses** : la marque tombe au-delà, pas dessus.
        for (valeur, attendu) in [
            (VICTOIRE_MIN - 1, true),
            (VICTOIRE_MIN, false),
            (VICTOIRE_MAX, false),
            (VICTOIRE_MAX + 1, true),
        ] {
            let hors = !(VICTOIRE_MIN..=VICTOIRE_MAX).contains(&valeur);
            assert_eq!(hors, attendu, "{valeur}");
            assert_eq!(marque(hors).is_empty(), !attendu);
        }
        for (valeur, attendu) in [
            (900u32, true),
            (1_000, false),
            (8_000, false),
            (8_100, true),
        ] {
            let hors = !(CONSERVATION_MIN..=CONSERVATION_MAX).contains(&valeur);
            assert_eq!(hors, attendu, "conservation {valeur}");
        }
        for (ante, attendu) in [(5u8, true), (6, false), (7, false), (8, true)] {
            let hors = !(ANTE_DEFAITE_MIN..=ANTE_DEFAITE_MAX).contains(&ante);
            assert_eq!(hors, attendu, "ante {ante}");
        }
        // Le marqueur est le même signe partout, et vient d'une seule fonction.
        assert_eq!(marque(true), marque(true));
        assert!(!marque(true).is_empty());
        assert!(marque(false).is_empty());
    }

    #[test]
    fn test_per_ante_median_uses_entered_runs() {
        // **La médiane porte sur les runs qui ont atteint l'ante.** Sur tous
        // les runs, les zéros des antes jamais entrés dominent : mesuré, la
        // médiane vaut zéro dès l'ante deux, ce qui montre une falaise qui
        // n'existe pas et rend « le premier ante sous 1,00 » muet.
        let (config, resultats) = un_lot(500);
        let rapport = agreger(&config, &resultats);

        assert_eq!(
            rapport.observations_by_ante[0], 500,
            "tous entrent à l'ante 1"
        );
        assert!(
            rapport.observations_by_ante[1] < 500,
            "le décor ne discrimine pas : tous les runs atteignent l'ante 2"
        );
        assert!(
            rapport.score_vs_target_by_ante[1] > 0,
            "l'ante 2 rend zéro : les runs non entrés ont été comptés"
        );
        // Un ante que personne n'atteint porte zéro observation, et sa médiane
        // ne se lit pas comme une mesure.
        assert_eq!(rapport.observations_by_ante[7], 0);
        assert_eq!(rapport.score_vs_target_by_ante[7], 0);
    }

    #[test]
    fn test_first_ante_below_one_is_named() {
        let mut rapport = agreger(&un_lot(50).0, &[]);
        rapport.observations_by_ante = [10; ANTES];
        rapport.score_vs_target_by_ante = [
            DIX_MILLIEMES + 1,
            DIX_MILLIEMES,
            DIX_MILLIEMES + 5,
            DIX_MILLIEMES,
            DIX_MILLIEMES,
            DIX_MILLIEMES - 1,
            0,
            0,
        ];
        assert_eq!(premier_ante_sous_un(&rapport), Some(6));

        rapport.score_vs_target_by_ante = [DIX_MILLIEMES; ANTES];
        assert_eq!(premier_ante_sous_un(&rapport), None, "les huit tiennent");

        // Un ante sans observation n'est pas un mur : il n'a pas été joué.
        rapport.observations_by_ante = [10, 10, 0, 0, 0, 0, 0, 0];
        rapport.score_vs_target_by_ante = [DIX_MILLIEMES, DIX_MILLIEMES, 0, 0, 0, 0, 0, 0];
        assert_eq!(premier_ante_sous_un(&rapport), None);
    }

    #[test]
    fn test_relic_table_iterates_the_catalog() {
        let (config, resultats) = un_lot(30);
        let rapport = agreger(&config, &resultats);
        assert_eq!(
            rapport.relics.len(),
            CATALOG.len(),
            "la table filtre le catalogue au lieu de l'itérer"
        );
        let ordre: Vec<RelicId> = rapport.relics.iter().map(|(def, _)| *def).collect();
        assert_eq!(
            ordre,
            CATALOG.to_vec(),
            "l'ordre n'est pas celui du catalogue"
        );
        // **Une relique jamais proposée figure quand même, à zéro.** Son
        // absence serait une information perdue, et c'est l'une des plus
        // utiles. Le décor se fabrique : trente runs suffisent déjà à proposer
        // les douze, et une campagne ne prouverait donc rien ici.
        let base = resultats[0].0.clone();
        let mut agg = RunAggregates::neuf();
        agg.relics_seen.push(RelicEncounter {
            def: CATALOG[0],
            offered: true,
            bought: false,
            kept: false,
            contribution: 0,
        });
        let maigre = agreger(&config, &[(ligne(&base, false, false, 1), agg)]);
        assert_eq!(maigre.relics.len(), CATALOG.len());
        assert_eq!(maigre.relics[0].1.offered, 1);
        assert!(
            maigre.relics[1..]
                .iter()
                .all(|(_, stats)| stats.offered == 0),
            "une relique jamais vue a disparu du tableau"
        );
    }

    #[test]
    fn test_abandoned_runs_are_excluded_and_counted() {
        let (config, resultats) = un_lot(20);
        let base = resultats[0].0.clone();
        let agg = resultats[0].1.clone();
        let lot: Vec<(RunOutcome, RunAggregates)> = vec![
            (ligne(&base, true, false, 8), agg.clone()),
            (ligne(&base, false, false, 3), agg.clone()),
            (ligne(&base, false, true, 2), agg.clone()),
            (ligne(&base, false, true, 2), agg.clone()),
        ];
        let rapport = agreger(&config, &lot);
        assert_eq!(rapport.abandoned, 2);
        assert_eq!(rapport.runs, 4);
        // Les deux grandeurs sont distinctes : les lignes fabriquées ne portent
        // aucune anomalie, bien que deux soient abandonnées.
        assert_eq!(
            rapport.anomalies, 0,
            "abandon et anomalie ont été confondus"
        );
        // Une victoire sur **deux** runs retenus, jamais sur quatre.
        assert_eq!(rapport.win_rate_by_cup[0].1, 5_000);
        // Et l'ante médian ne compte pas les abandons.
        assert_eq!(rapport.median_defeat_ante, 3);
    }

    #[test]
    fn test_unmeasurable_targets_are_not_marked_as_conforming() {
        // Mesuré : les cinq gobelets rendent zéro victoire, donc l'écart entre
        // eux vaut zéro et la cible « moins de quinze points » passe **sans
        // rien mesurer**. Une cible verte par absence de mesure est pire qu'une
        // cible rouge.
        let (config, resultats) = un_lot(40);
        let rapport = agreger(&config, &resultats);
        // **La ligne d'écart, spécifiquement.** Mesuré au banc : une assertion
        // sur la seule présence du libellé passe, « non mesurable » figurant
        // aussi sur les antes jamais atteints et les paliers jamais achetés.
        assert_eq!(
            ecart_entre_gobelets(&rapport),
            None,
            "un écart est calculé entre cinq taux nuls"
        );
        let sortie = rendu(&rapport);
        let ligne_ecart = sortie
            .lines()
            .find(|l| l.contains("écart"))
            .unwrap_or_default();
        assert!(
            ligne_ecart.contains(SANS_MESURE),
            "l'écart passe pour conforme : {ligne_ecart}"
        );
        assert!(!ligne_ecart.contains(MARQUEUR));

        // Et la moitié qui discrimine : dès qu'un gobelet gagne, l'écart existe.
        let mut avec_victoire = rapport.clone();
        avec_victoire.win_rate_by_cup = vec![(CupId::Standard, 3_000), (CupId::Fortune, 1_000)];
        assert_eq!(ecart_entre_gobelets(&avec_victoire), Some(2_000));
    }

    #[test]
    fn test_contribution_out_of_the_pipeline_is_not_zero() {
        // Une relique achetée sans jamais produire un pas de score : elle agit
        // sur l'or de fin de manche ou sur le lancer, et ce n'est pas « sans
        // valeur ».
        let muette = RelicStats {
            offered: 900,
            bought: 898,
            kept: 898,
            mean_contribution: 0,
        };
        assert!(muette.hors_du_pipeline());
        let parlante = RelicStats {
            mean_contribution: 1,
            ..muette
        };
        assert!(!parlante.hors_du_pipeline());
        // Jamais achetée : ce n'est pas la même chose, et la mesure n'a pas eu
        // lieu.
        let jamais = RelicStats {
            offered: 300,
            bought: 0,
            kept: 0,
            mean_contribution: 0,
        };
        assert!(!jamais.hors_du_pipeline());

        let (config, resultats) = un_lot(200);
        let sortie = rendu(&agreger(&config, &resultats));
        assert!(
            sortie.contains("—"),
            "aucune relique hors pipeline n'est distinguée : {sortie}"
        );
    }

    #[test]
    fn test_win_rate_is_reported_per_cell() {
        let config = SimConfig {
            cups: vec![CupId::Standard, CupId::Fortune],
            stakes: vec![1, 3],
            ..cellule(CupId::Standard, 1, 10)
        };
        let rapport = agreger(&config, &campagne(&config));
        assert_eq!(rapport.win_rate_by_cell.len(), 4, "quatre cellules");
        assert_eq!(
            rapport.win_rate_by_cell[0].0,
            (CupId::Standard, 1),
            "l'ordre n'est pas celui du produit cartésien"
        );
        assert_eq!(rapport.win_rate_by_cell[1].0, (CupId::Standard, 3));
        assert_eq!(rapport.win_rate_by_cell[3].0, (CupId::Fortune, 3));
        // Le taux par gobelet porte sur **une** mise, et le rapport la nomme.
        assert_eq!(rapport.reference_stake, 1);
        assert_eq!(rapport.win_rate_by_cup.len(), 2);
        assert_eq!(rapport.win_rate_by_stake.len(), 2);
        assert!(rendu(&rapport).contains("mise 1"));
    }

    #[test]
    fn test_stake_progression_is_reported_as_unmeasurable() {
        let config = SimConfig {
            stakes: vec![1, 3, 4],
            ..cellule(CupId::Standard, 1, 10)
        };
        let sortie = rendu(&agreger(&config, &campagne(&config)));
        assert!(
            sortie.contains("non mesurable avant l'Étape 10"),
            "la progression des mises est présentée comme mesurée : {sortie}"
        );
        // Aucune colonne pour les mises que le corpus ne distingue pas.
        for absente in [" 2 ", " 5 ", " 6 "] {
            assert!(
                !sortie.contains(&format!("mise{absente}")),
                "une mise non couverte est affichée : {absente}"
            );
        }
        // **Aucune date** : elle ferait changer l'empreinte à chaque exécution,
        // et c'est cette empreinte que la passe nocturne compare.
        for chiffre in ["2026", "2025", "2027"] {
            assert!(
                !sortie.contains(chiffre),
                "une date s'est glissée : {chiffre}"
            );
        }
    }

    #[test]
    fn test_mean_contribution_divides_by_purchases() {
        // **Mesuré au banc : diviser par les propositions au lieu des achats
        // survit** tant que les deux coïncident. Il faut un lot où ils
        // diffèrent.
        let (config, resultats) = un_lot(20);
        let base = resultats[0].0.clone();
        let rencontre = |offered, bought, contribution| {
            let mut agg = RunAggregates::neuf();
            agg.relics_seen.push(RelicEncounter {
                def: CATALOG[0],
                offered,
                bought,
                kept: bought,
                contribution,
            });
            (ligne(&base, false, false, 2), agg)
        };
        // Quatre runs la voient, deux l'achètent, pour 10 et 30 points.
        let lot = vec![
            rencontre(true, true, 10),
            rencontre(true, true, 30),
            rencontre(true, false, 0),
            rencontre(true, false, 0),
        ];
        let rapport = agreger(&config, &lot);
        let (_, stats) = rapport.relics[0];
        assert_eq!((stats.offered, stats.bought), (4, 2));
        assert_eq!(stats.mean_contribution, 20, "divisé par les propositions");
        assert_ne!(stats.mean_contribution, 10);
    }

    #[test]
    fn test_win_rate_by_cup_is_taken_at_the_reference_stake() {
        // **Mesuré au banc : ignorer la mise survit** quand tous les taux sont
        // nuls. Un lot fabriqué où le gobelet gagne à une mise et pas à l'autre.
        let config = SimConfig {
            cups: vec![CupId::Standard],
            stakes: vec![1, 3],
            ..cellule(CupId::Standard, 1, 1)
        };
        let base = campagne(&cellule(CupId::Standard, 1, 1))[0].0.clone();
        let agg = RunAggregates::neuf();
        let avec = |victory, stake| {
            (
                RunOutcome {
                    stake,
                    ..ligne(&base, victory, false, 8)
                },
                agg.clone(),
            )
        };
        let lot = vec![
            avec(true, 1),
            avec(false, 3),
            avec(false, 3),
            avec(false, 3),
        ];
        let rapport = agreger(&config, &lot);

        assert_eq!(rapport.reference_stake, 1);
        assert_eq!(
            rapport.win_rate_by_cup[0].1, DIX_MILLIEMES,
            "le taux par gobelet mélange les mises"
        );
        assert_ne!(rapport.win_rate_by_cup[0].1, 2_500);
        // Et le taux par mise, lui, sépare bien les deux.
        assert_eq!(rapport.win_rate_by_stake[0].1, DIX_MILLIEMES);
        assert_eq!(rapport.win_rate_by_stake[1].1, 0);
    }

    #[test]
    fn test_report_order_is_stable() {
        // Le rendu ne dépend pas de l'ordre de parcours des runs : une table de
        // hachage réordonnerait ses clés d'une exécution à l'autre, sans erreur
        // et sans qu'aucun test unitaire ne le voie.
        let (config, resultats) = un_lot(100);
        let direct = rendu(&agreger(&config, &resultats));
        let mut inverse = resultats.clone();
        inverse.reverse();
        assert_eq!(direct, rendu(&agreger(&config, &inverse)));
        assert!(!direct.is_empty());
    }

    #[test]
    fn test_report_reads_both_channels() {
        // Les quatre grandeurs du second canal sont nulles quand il est vide,
        // et non nulles quand il est plein : elles ne peuvent pas venir des
        // colonnes du tableau, qui ne portent rien par ante ni par relique.
        let (config, resultats) = un_lot(200);
        let plein = agreger(&config, &resultats);
        assert!(plein.score_vs_target_by_ante[0] > 0);
        assert!(plein.relics.iter().any(|(_, s)| s.offered > 0));
        assert!(plein.relics.iter().any(|(_, s)| s.bought > 0));

        let sans_canal: Vec<(RunOutcome, RunAggregates)> = resultats
            .iter()
            .map(|(ligne, _)| (ligne.clone(), RunAggregates::neuf()))
            .collect();
        let vide = agreger(&config, &sans_canal);
        assert_eq!(vide.score_vs_target_by_ante, [0; ANTES]);
        assert!(vide.relics.iter().all(|(_, s)| s.offered == 0));
        assert!(vide.relics.iter().all(|(_, s)| s.mean_contribution == 0));
        // Mais les grandeurs du premier canal, elles, restent produites.
        assert_eq!(vide.runs, plein.runs);
        assert_eq!(vide.median_defeat_ante, plein.median_defeat_ante);
    }

    #[test]
    fn test_relics_field_needs_no_new_derive() {
        // La table est un vecteur de paires, et elle compile **sans** dérivé
        // d'ordre sur l'identifiant de relique. Le manque est inscrit au
        // fichier des manques d'API, propriétaire Étape 2 ; le harnais ne
        // l'ajoute pas — ce serait une ligne du moteur.
        let (config, resultats) = un_lot(10);
        let rapport = agreger(&config, &resultats);
        let _: &Vec<(RelicId, RelicStats)> = &rapport.relics;
        let chemin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("MISSING_API.md");
        let contenu = std::fs::read_to_string(&chemin).unwrap_or_default();
        assert!(
            contenu.contains("ne s'ordonne pas"),
            "l'entrée du manque a disparu du fichier des manques"
        );
        assert!(contenu.contains("Étape propriétaire"));
    }

    #[test]
    fn test_rarity_ratio_is_an_hypothesis() {
        let (config, resultats) = un_lot(100);
        let sortie = rendu(&agreger(&config, &resultats));
        assert!(sortie.contains("hypothèse"), "{sortie}");
        assert!(
            !sortie.contains("conforme"),
            "un ratio de rareté est présenté comme conforme"
        );
    }

    #[test]
    fn test_encounter_counts_runs_not_appearances() {
        // Les trois indicateurs sont des booléens de run à la source : une
        // relique proposée deux fois dans le même run compte pour une.
        let (config, resultats) = un_lot(20);
        let base = resultats[0].0.clone();
        let mut agg = RunAggregates::neuf();
        agg.relics_seen.push(RelicEncounter {
            def: CATALOG[0],
            offered: true,
            bought: true,
            kept: true,
            contribution: 10,
        });
        let lot = vec![(ligne(&base, false, false, 2), agg)];
        let rapport = agreger(&config, &lot);
        let (_, stats) = rapport
            .relics
            .iter()
            .find(|(def, _)| *def == CATALOG[0])
            .expect("la relique figure au tableau");
        assert_eq!((stats.offered, stats.bought, stats.kept), (1, 1, 1));
        assert_eq!(stats.mean_contribution, 10);
    }
}
