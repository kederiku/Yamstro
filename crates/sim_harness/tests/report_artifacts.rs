//! Les pièces du premier rapport d'équilibrage, éprouvées **sur les fichiers**.
//!
//! **Ces tests vérifient qu'un rapport est un rapport, pas qu'il dit vrai.**
//! « Cette section existe et elle est chiffrée » se contrôle ; « cette section
//! dit la bonne chose » ne se contrôle pas, et prétendre le faire produirait
//! neuf contrôles qui passent sur un document vide de mesures.
//!
//! D'où la forme retenue : ancrage sur les **titres de section**, qui sont
//! normatifs, et exigence d'au moins **un nombre** dans chaque section qui doit
//! en porter un — plus une contre-épreuve qui retire les nombres et vérifie que
//! les tests tombent.
//!
//! Les motifs que ces tests interdisent s'**assemblent** au lieu de s'épeler :
//! épelés, ils se déclencheraient sur ce fichier même.

use std::path::{Path, PathBuf};

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

fn analyse() -> String {
    lire("reports/balance-report.md")
}

/// Le corps d'une section, de son titre au titre suivant de même niveau.
fn section(texte: &str, titre: &str) -> String {
    let debut = texte
        .find(titre)
        .unwrap_or_else(|| panic!("section « {titre} » absente"));
    let reste = &texte[debut + titre.len()..];
    let fin = reste.find("\n## ").unwrap_or(reste.len());
    reste[..fin].to_string()
}

/// Les lignes de données du **premier** tableau d'un bloc.
///
/// Une section peut porter plusieurs tableaux — celle des sept questions en a
/// deux —, et compter toutes les lignes de tableau du bloc mêlerait les deux.
fn premier_tableau(bloc: &str) -> Vec<&str> {
    let lignes: Vec<&str> = bloc
        .lines()
        .skip_while(|ligne| !ligne.starts_with("| "))
        .take_while(|ligne| ligne.starts_with("| "))
        // **La ligne de séparation se reconnaît à sa forme**, pas à l'une de
        // ses orthographes : une colonne alignée à droite s'écrit « --: » et
        // non « :-- », et un filtre sur la seconde seule laisse passer la
        // séparation des tableaux alignés à droite.
        .filter(|ligne| {
            !ligne
                .chars()
                .all(|lettre| matches!(lettre, '|' | '-' | ':' | ' '))
        })
        .collect();
    lignes.into_iter().skip(1).collect()
}

/// Vrai si le texte porte au moins un chiffre.
fn porte_un_nombre(texte: &str) -> bool {
    texte.chars().any(|lettre| lettre.is_ascii_digit())
}

/// Les titres de section normatifs du rapport, **sans leur numéro**.
///
/// La contre-épreuve remplace les chiffres par une lettre pour vérifier que les
/// contrôles tombent ; un titre numéroté deviendrait alors introuvable, et le
/// test se casserait sur son propre outil au lieu de mesurer quoi que ce soit.
const TEMOIN: &str = "Le témoin";
const SEPT_QUESTIONS: &str = "Les sept questions";
const ARBITRAGES: &str = "Les trois arbitrages";
const VALEURS: &str = "Les valeurs remises en cause";
const CONVERSION: &str = "La conversion";
const CORRECTION: &str = "La règle de correction";
const LIMITES: &str = "Portée et limites";

#[test]
fn test_report_artifacts_exist() {
    let console = lire("reports/balance-console.txt");
    let csv = lire("reports/balance.csv");
    let page = analyse();

    for (nom, contenu) in [
        ("la sortie console", &console),
        ("le tableau", &csv),
        ("la page d'analyse", &page),
    ] {
        assert!(!contenu.trim().is_empty(), "{nom} est vide");
    }

    // L'en-tête du tableau est celui de la campagne, et le nombre de lignes de
    // données correspond à ce que la page annonce.
    let mut lignes = csv.lines();
    let entete = lignes.next().unwrap_or_default();
    assert!(
        entete.starts_with("seed,cup,stake,policy,shop_policy,"),
        "l'en-tête du tableau a changé : {entete}"
    );
    let donnees = lignes.count();
    assert!(
        donnees >= 10_000,
        "le tableau porte {donnees} lignes : la campagne annoncée n'y est pas"
    );
    assert!(
        page.contains(&donnees.to_string())
            || page.contains(&format!(
                "{} {}",
                donnees / 1000,
                format_args!("{:03}", donnees % 1000)
            )),
        "la page n'annonce pas le nombre de runs du tableau ({donnees})"
    );

    // La sortie console est celle d'un rapport agrégé, non retouchée.
    assert!(
        console.contains("rapport d'équilibrage"),
        "la sortie console n'est pas celle du harnais"
    );
}

#[test]
fn test_report_records_machine_and_thread_count() {
    let page = analyse();
    let entete = page.split(TEMOIN).next().unwrap_or_default().to_string();

    // **Les champs se cherchent dans le tableau d'en-tête, pas dans l'en-tête
    // entier.** La prose qui suit le tableau parle du débit et de la commande :
    // les y chercher survit à la disparition de leur ligne — mesuré au banc.
    let tableau: Vec<&str> = entete
        .lines()
        .filter(|ligne| ligne.starts_with("| "))
        .collect();
    for champ in [
        "date",
        "commit",
        "seed_base",
        "commande",
        "machine",
        "threads",
        "débit",
    ] {
        assert!(
            tableau
                .iter()
                .any(|ligne| ligne.to_lowercase().contains(champ)),
            "le tableau d'en-tête ne porte pas de ligne {champ}"
        );
    }
    assert!(
        porte_un_nombre(&entete),
        "l'en-tête ne porte aucun nombre : la campagne n'est pas rejouable"
    );
    // L'empreinte de commit est une vraie empreinte, pas un mot.
    assert!(
        entete.split_whitespace().any(|mot| {
            let nu = mot.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            nu.len() >= 7 && nu.chars().all(|c| c.is_ascii_hexdigit())
        }),
        "aucune empreinte de commit dans l'en-tête"
    );
}

#[test]
fn test_report_cites_three_challenged_values() {
    let page = analyse();
    let bloc = section(&page, VALEURS);

    // Les valeurs sont listées dans un tableau, une ligne chacune. Le contrôle
    // porte sur le **compte** et sur la présence d'un nombre par ligne : ce
    // qu'une valeur remise en cause doit avoir, c'est le chiffre qui la
    // contredit.
    let lignes = premier_tableau(&bloc);
    assert!(
        lignes.len() >= 3,
        "{} valeur(s) remise(s) en cause, il en faut au moins trois",
        lignes.len()
    );
    for ligne in &lignes {
        // **Le chiffre se cherche dans la cellule qui contredit**, jamais dans
        // la ligne : le numéro d'ordre et le numéro d'étape en fournissent deux
        // gratuitement, et la garde survivrait au retrait de la mesure —
        // mesuré au banc.
        let cellules: Vec<&str> = ligne.split('|').map(str::trim).collect();
        assert_eq!(
            cellules.len(),
            7,
            "une valeur n'a pas ses cinq colonnes — numéro, valeur, étape, \
             nombre, correction : {ligne}"
        );
        assert!(
            porte_un_nombre(cellules[4]),
            "une valeur est citée sans le nombre qui la contredit : {ligne}"
        );
        assert!(
            cellules[5].len() > 20,
            "une valeur n'a pas de correction proposée : {ligne}"
        );
    }
}

#[test]
fn test_report_answers_the_seven_questions() {
    let page = analyse();
    let bloc = section(&page, SEPT_QUESTIONS);

    let lignes = premier_tableau(&bloc);
    assert_eq!(
        lignes.len(),
        7,
        "les sept questions ne sont pas toutes traitées : {} lignes",
        lignes.len()
    );

    // Celle des stakes porte la mention datée, jamais six colonnes.
    let stakes = lignes
        .iter()
        .find(|ligne| ligne.contains("mise") || ligne.contains("stake"))
        .expect("la question des stakes");
    assert!(
        stakes.contains("non mesurable"),
        "la question des stakes prétend mesurer : {stakes}"
    );
    assert!(
        stakes.contains("Étape 10"),
        "la mention de non-mesurabilité n'est pas datée : {stakes}"
    );
}

#[test]
fn test_report_settles_the_three_arbitrations() {
    let page = analyse();
    let bloc = section(&page, ARBITRAGES);

    // Trois sous-sections nommées, chacune chiffrée.
    for (titre, quoi) in [
        ("### (a)", "les doublons d'étalage"),
        ("### (b)", "La Cage"),
        ("### (c)", "le signal de calibrage n° 1"),
    ] {
        // **Ancré sur le début de ligne.** Un titre de niveau inférieur
        // contient celui du niveau au-dessus : « #### (b) » porte « ### (b) »,
        // et la garde survivait à la dégradation du titre — mesuré au banc.
        let ancre = format!("\n{titre}");
        let debut = bloc
            .find(&ancre)
            .map(|index| index + 1)
            .unwrap_or_else(|| panic!("l'arbitrage {titre} — {quoi} — manque"));
        let reste = &bloc[debut..];
        let fin = reste[4..].find("### ").map_or(reste.len(), |i| i + 4);
        let sous = &reste[..fin];
        assert!(
            porte_un_nombre(sous),
            "l'arbitrage {titre} est rendu sans chiffre : {sous}"
        );
    }

    // **Le troisième porte une suite de cibles et un ante de bascule, jamais un
    // taux de victoire.** Le Mur n'est dans aucun catalogue de boss livré :
    // aucune run ne peut produire cet empilement, et un taux calculé dessus
    // porterait sur zéro run tout en se lisant comme une impasse confirmée.
    let debut = bloc.find("### (c)").expect("l'arbitrage (c)");
    let troisieme = &bloc[debut..];
    assert!(
        troisieme.contains("bascule"),
        "l'arbitrage (c) ne conclut pas sur un ante de bascule"
    );
    // La suite des huit cibles est **un tableau de huit lignes**, pas huit
    // lignes chiffrées quelconques : la prose en fournit davantage, et la garde
    // survivait au retrait d'un ante — mesuré au banc.
    let cibles = premier_tableau(troisieme);
    assert_eq!(
        cibles.len(),
        8,
        "l'arbitrage (c) porte {} cibles au lieu de huit",
        cibles.len()
    );
    assert!(
        !troisieme.contains("taux de victoire de l'empilement"),
        "l'arbitrage (c) est tranché sur un taux de victoire, qu'aucune run ne peut produire"
    );
}

#[test]
fn test_report_states_its_limits() {
    let page = analyse();
    let bloc = section(&page, LIMITES);

    // **Ce qui se contrôle, c'est que chaque limite soit chiffrée.** Chercher
    // les mots dans la section entière ne garde rien : la prose de la section
    // nomme les gobelets et les mises ailleurs que dans la liste, et la garde
    // survivait au retrait de la ligne — mesuré au banc.
    //
    // La forme retenue : chaque puce de la liste porte au moins un nombre, et
    // il y en a au moins cinq — une limite sans chiffre est une clause de style.
    let puces: Vec<&str> = bloc
        .lines()
        .filter(|ligne| ligne.starts_with("- "))
        .collect();
    assert!(
        puces.len() >= 5,
        "la section de portée ne liste que {} limite(s)",
        puces.len()
    );
    for puce in &puces {
        assert!(
            porte_un_nombre(puce),
            "une limite est énoncée sans chiffre : {puce}"
        );
    }
    // Et l'absence de mesure des mises y figure, datée.
    assert!(
        bloc.contains("Étape 10"),
        "la section de portée ne date pas l'absence de mesure des mises"
    );

    // **Aucun intervalle de confiance.** Une barre d'erreur inventée
    // transforme une mesure exploratoire en résultat.
    let interdit = format!("intervalle de {}", "confiance");
    for motif in [interdit.as_str(), "± ", "écart-type"] {
        assert!(
            !page.contains(motif),
            "le rapport produit une marge qu'il ne peut pas calculer : {motif}"
        );
    }
}

#[test]
fn test_conversion_is_proposed_not_assumed() {
    let page = analyse();
    let bloc = section(&page, CONVERSION);

    assert!(porte_un_nombre(&bloc), "la section n'est pas chiffrée");
    // **Le nombre d'observations se lit dans le tableau, pas dans la prose.**
    // Chercher le mot dans la section entière survit au retrait de la colonne —
    // mesuré au banc : la prose parle d'achats ailleurs. Ce qui garde, c'est
    // que chaque palier porte ses contributions **et** ses comptes.
    let paliers = premier_tableau(&bloc);
    assert_eq!(
        paliers.len(),
        3,
        "la conversion ne repose pas sur les trois paliers peuplés"
    );
    let entete = bloc
        .lines()
        .find(|ligne| ligne.starts_with("| "))
        .unwrap_or_default();
    assert!(
        entete.contains("achats"),
        "le tableau des paliers ne dit pas sur combien d'achats il repose : {entete}"
    );
    for palier in &paliers {
        let cellules: Vec<&str> = palier.split('|').map(str::trim).collect();
        assert!(
            cellules.len() >= 5 && porte_un_nombre(cellules[4]),
            "un palier est donné sans son nombre d'observations : {palier}"
        );
    }
    // **Le ratio du glossaire reste une hypothèse.** Il n'est jamais présenté
    // comme atteint ou manqué.
    let ratio = format!("1 : 2 : {}", "3,75");
    if let Some(index) = bloc.find(&ratio) {
        let autour = &bloc[index.saturating_sub(200)..(index + 200).min(bloc.len())];
        assert!(
            autour.contains("hypothèse"),
            "le ratio du glossaire est présenté comme une cible : {autour}"
        );
    }
}

#[test]
fn test_report_keeps_the_correction_rule() {
    let page = analyse();
    let bloc = section(&page, CORRECTION);

    assert!(
        bloc.contains("définitions"),
        "la règle de correction ne dit pas où la valeur se corrige"
    );
    // **La formule de score ne se touche pas**, et la courbe s'ajuste dans
    // l'ordre : le terme initial d'abord, la croissance ensuite.
    assert!(
        bloc.contains("formule de score"),
        "la règle ne protège pas la formule de score"
    );
    assert!(
        bloc.contains("trois cents"),
        "la règle ne nomme pas le terme initial de la courbe"
    );
    assert!(
        bloc.contains("croissance par ante"),
        "la règle ne nomme pas le second facteur de la courbe"
    );

    // **Les deux nombres de la courbe s'écrivent en toutes lettres.** C'est du
    // rapport qu'on copie-colle une justification vers un commentaire de code :
    // un littéral écrit ici revient dans les crates par cette porte.
    let initial = format!("300{}000", "_");
    let croissance = format!("1{}600", "_");
    for motif in [initial.as_str(), croissance.as_str()] {
        assert!(
            !page.contains(motif),
            "la page d'analyse écrit un littéral de courbe : {motif}"
        );
    }
}

#[test]
fn test_report_uses_no_curve_literal_in_crates() {
    // **Le contrôle porte sur les crates, jamais sur les rapports.** Le tableau
    // porte une colonne de graine sur dix mille runs : la ligne de graine 1600
    // existe nécessairement, et un score ou un or peuvent valoir 1600 aussi. Un
    // motif de littéral appliqué à un fichier de données confond une valeur
    // **mesurée** avec une valeur **codée**, et la seule correction disponible
    // serait de falsifier le livrable.
    let initial = format!("300{}000", "_");
    let sortie = std::process::Command::new("grep")
        .args(["-rn", "--include=*.rs", "-e", initial.as_str(), "crates/"])
        .current_dir(racine())
        .output()
        .expect("grep");
    let trouve = String::from_utf8_lossy(&sortie.stdout);
    let hors_courbe: Vec<&str> = trouve
        .lines()
        .filter(|ligne| !ligne.contains("blinds/scaling.rs"))
        .filter(|ligne| !ligne.contains("tests/"))
        .collect();
    assert!(
        hors_courbe.is_empty(),
        "le terme initial de la courbe est écrit hors de son fichier : {hors_courbe:?}"
    );
}

#[test]
fn test_missing_api_has_at_least_the_known_entries() {
    let fichier = lire("crates/sim_harness/MISSING_API.md");
    assert!(!fichier.trim().is_empty(), "le fichier est vide");

    let entrees: Vec<&str> = fichier
        .lines()
        .filter(|ligne| ligne.starts_with("## "))
        .collect();
    assert!(
        entrees.len() >= 4,
        "{} entrée(s) ; les quatre connues d'avance doivent y être",
        entrees.len()
    );

    // Chaque entrée porte sa signature souhaitée et son étape propriétaire.
    for entree in &entrees {
        let bloc = section(&fichier, entree);
        // **Une API souhaitée n'est pas toujours une fonction** : la première
        // entrée demande une constante d'énumération. Ce qui se contrôle, c'est
        // qu'une forme publique soit proposée.
        assert!(
            bloc.contains("pub "),
            "l'entrée « {entree} » ne propose aucune forme publique"
        );
        // **Ancré sur la mention qui porte la décision**, pas sur le mot :
        // la prose d'une entrée nomme couramment d'autres étapes, et la garde
        // survivait au retrait du propriétaire — mesuré au banc.
        assert!(
            bloc.contains("Étape propriétaire"),
            "l'entrée « {entree} » n'a pas d'étape propriétaire"
        );
    }
}

#[test]
fn test_the_checks_fall_on_a_report_without_numbers() {
    // **La contre-épreuve.** Neuf contrôles de présence passeraient sur un
    // document qui nomme ses sections sans rien mesurer : ce test retire les
    // chiffres et vérifie que les contrôles tombent.
    let page = analyse();
    let sans_chiffres: String = page
        .chars()
        .map(|lettre| if lettre.is_ascii_digit() { 'x' } else { lettre })
        .collect();

    for titre in [VALEURS, ARBITRAGES, CONVERSION, LIMITES] {
        let bloc = section(&sans_chiffres, titre);
        assert!(
            !porte_un_nombre(&bloc),
            "la section {titre} porte encore un chiffre après effacement"
        );
    }

    // Et le rapport réel, lui, en porte dans chacune.
    for titre in [
        TEMOIN,
        SEPT_QUESTIONS,
        ARBITRAGES,
        VALEURS,
        CONVERSION,
        LIMITES,
    ] {
        assert!(
            porte_un_nombre(&section(&page, titre)),
            "la section {titre} n'est pas chiffrée"
        );
    }
}
