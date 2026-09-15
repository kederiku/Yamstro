//! Description d'une campagne, et la fourchette de la porte d'intégration.
//!
//! Ce module ne simule rien : il décrit ce qu'une campagne couvrirait, et il
//! rend le verdict d'une mesure déjà prise. La boucle arrive à TASK-144, la
//! campagne à TASK-151.

use core_engine::cups::CupId;

/// Le triplet de stakes par défaut. **Ce n'est pas `1..=6`, et c'est normatif**
/// (raccord F du backlog de l'étape).
///
/// À la fin de l'Étape 6, le niveau de stake n'a que deux effets : un bouchon
/// qui rend `1` au quatrième niveau et `0` partout ailleurs, et un facteur de
/// croissance binaire, l'un en dessous du troisième niveau et l'autre à partir
/// de là. Les niveaux 3, 5 et 6 sont donc **rigoureusement indiscernables**
/// tant que l'Étape 10 n'a pas livré leur table : une campagne par défaut sur
/// les six rendrait six colonnes dont quatre jumelles, et quatre colonnes
/// identiques se lisent comme un résultat — « la progression est plate » —
/// alors qu'elles ne mesurent qu'une absence d'implémentation.
///
/// Cette ligne se remplace par la matrice complète le jour où cette table
/// existe. Rien d'autre n'a à changer.
const STAKES_PAR_DEFAUT: [u8; 3] = [1, 3, 4];

/// Un taux se compte en **dix-millièmes**, jamais en flottant : la comparaison
/// d'un taux en flottant fait diverger la porte d'intégration d'une plateforme
/// à l'autre, et personne ne saurait dire laquelle des deux ment.
const UNITE: u32 = 10_000;

/// Ce qu'une campagne couvrirait. **Le corps de cette structure est normatif**
/// et se reprend au champ près ; les dérivés, eux, sont à nous.
///
/// Elle ne porte **ni** `out`, `report`, `trace`, `seed` **ni** la fourchette
/// d'assertion : ces cinq-là décrivent la sortie, pas la campagne, et vivent
/// dans la structure de ligne de commande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimConfig {
    pub runs: u32,
    pub seed_base: u64,
    pub cups: Vec<CupId>,
    pub stakes: Vec<u8>,
    pub policy: PolicyKind,
    pub shop_policy: ShopPolicyKind,
    pub threads: usize,
}

/// Les trois sondes de décision de main. **Aucune donnée portée, aucun
/// `Default`** : une campagne naît d'une invocation, jamais de zéros
/// implicites. Le défaut de la ligne de commande est un choix explicite, pas
/// une valeur nulle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum PolicyKind {
    Greedy,
    GridAware,
    Random,
}

/// Les deux politiques d'achat. Même règle qu'au-dessus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ShopPolicyKind {
    Budget,
    Synergy,
}

/// Une fourchette de taux de victoire, bornes comprises, en dix-millièmes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinRateRange {
    pub min: u32,
    pub max: u32,
}

/// Ce que la porte d'intégration rend : un code de sortie, et la ligne qui
/// l'explique quand il n'est pas nul.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub code: u8,
    pub stderr: Option<String>,
}

/// Les stakes couverts quand la ligne de commande n'en nomme aucun.
pub fn stakes_par_defaut() -> Vec<u8> {
    STAKES_PAR_DEFAUT.to_vec()
}

/// Le gobelet qui suit, dans l'ordre de déclaration.
///
/// **C'est le seul endroit du harnais qui nomme un gobelet**, et il cesse de
/// compiler le jour où le catalogue en gagne un : `E0004`, la variante nommée.
/// C'est la garantie mécanique qui remplace la liste ordonnée que le moteur
/// n'expose pas encore — voir `MISSING_API.md`.
///
/// Le bras `_ =>` est proscrit ici : il rendrait la non-exhaustivité muette,
/// et le harnais mesurerait cinq gobelets sur huit sans qu'un test le voie.
pub fn cup_suivant(cup: CupId) -> Option<CupId> {
    match cup {
        CupId::Standard => Some(CupId::Abandoned),
        CupId::Abandoned => Some(CupId::Polyhedron),
        CupId::Polyhedron => Some(CupId::Cheater),
        CupId::Cheater => Some(CupId::Fortune),
        CupId::Fortune => None,
    }
}

/// Les gobelets couverts quand la ligne de commande n'en nomme aucun : tous.
pub fn cups_par_defaut() -> Vec<CupId> {
    core::iter::successors(Some(CupId::Standard), |cup| cup_suivant(*cup)).collect()
}

/// Le nom d'un gobelet sur la ligne de commande.
///
/// Il vient du dérivé de formatage, donc **aucune seconde liste** ne le nomme,
/// et un gobelet ajouté est utilisable sans qu'une ligne change. La rançon est
/// qu'un renommage amont changerait le contrat de la ligne de commande sans
/// bruit : `test_cup_names_are_pinned` en fait un échec de test.
pub fn cup_nom(cup: CupId) -> String {
    format!("{cup:?}").to_lowercase()
}

/// Lit un gobelet sur la ligne de commande. La correspondance passe par la
/// chaîne ci-dessus, jamais par une seconde énumération.
pub fn parse_cup(texte: &str) -> Result<CupId, String> {
    cups_par_defaut()
        .into_iter()
        .find(|cup| cup_nom(*cup) == texte)
        .ok_or_else(|| {
            let connus = cups_par_defaut()
                .into_iter()
                .map(cup_nom)
                .collect::<Vec<_>>()
                .join(", ");
            format!("gobelet inconnu « {texte} » ; attendus : {connus}")
        })
}

/// Lit un taux décimal en dix-millièmes, **par découpage de la chaîne**.
///
/// `0.25` donne `2500`, `1` donne `10000`, `0.999` donne `9990`. Aucun
/// flottant n'intervient, pas même transitoirement.
fn en_dix_millemes(texte: &str) -> Result<u32, String> {
    let (entier, fraction) = texte.split_once('.').unwrap_or((texte, ""));
    if fraction.len() > 4 {
        return Err(format!("« {texte} » a plus de quatre décimales"));
    }
    let entier: u32 = entier
        .parse()
        .map_err(|_| format!("« {texte} » n'est pas un taux décimal"))?;
    let fraction: u32 = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<4}")
            .parse()
            .map_err(|_| format!("« {texte} » n'est pas un taux décimal"))?
    };
    let valeur = entier
        .checked_mul(UNITE)
        .and_then(|gros| gros.checked_add(fraction))
        .filter(|valeur| *valeur <= UNITE)
        .ok_or_else(|| format!("« {texte} » n'est pas un taux entre zéro et un"))?;
    Ok(valeur)
}

/// Rend un taux en décimal, pour l'affichage seul.
fn en_taux(valeur: u32) -> String {
    format!("{}.{:04}", valeur / UNITE, valeur % UNITE)
}

/// Lit la fourchette `MIN:MAX` de la porte d'intégration.
pub fn parse_win_rate(texte: &str) -> Result<WinRateRange, String> {
    let (min, max) = texte
        .split_once(':')
        .ok_or_else(|| format!("« {texte} » n'est pas une fourchette MIN:MAX"))?;
    let min = en_dix_millemes(min)?;
    let max = en_dix_millemes(max)?;
    if min > max {
        return Err(format!(
            "fourchette inversée : {} dépasse {}",
            en_taux(min),
            en_taux(max)
        ));
    }
    Ok(WinRateRange { min, max })
}

/// Le verdict de la porte d'intégration : une fonction **pure**, mesure et
/// intervalle en entrée, code de sortie et ligne d'erreur en sortie.
///
/// Son branchement sur une campagne réelle est l'affaire de TASK-151 et de
/// TASK-154 ; ici elle existe, elle est totale, et elle est éprouvée.
pub fn verdict(mesure: u32, fourchette: WinRateRange) -> Verdict {
    if (fourchette.min..=fourchette.max).contains(&mesure) {
        return Verdict {
            code: 0,
            stderr: None,
        };
    }
    Verdict {
        code: 1,
        stderr: Some(format!(
            "taux de victoire {} hors de la fourchette attendue {}:{}",
            en_taux(mesure),
            en_taux(fourchette.min),
            en_taux(fourchette.max)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_win_rate_parses_without_float() {
        for (texte, attendu) in [
            (
                "0.25:0.40",
                WinRateRange {
                    min: 2500,
                    max: 4000,
                },
            ),
            (
                "0:1",
                WinRateRange {
                    min: 0,
                    max: 10_000,
                },
            ),
            (
                "0.999:1",
                WinRateRange {
                    min: 9990,
                    max: 10_000,
                },
            ),
        ] {
            assert_eq!(parse_win_rate(texte), Ok(attendu), "{texte}");
        }

        // Une fourchette inversée, un séparateur manquant et un taux au-delà de
        // un sont refusés, pas silencieusement corrigés.
        for texte in ["0.40:0.25", "0.25", "1.5:2", "0.25:0.40:1", "a:b"] {
            assert!(parse_win_rate(texte).is_err(), "{texte} a été accepté");
        }
    }

    #[test]
    fn test_assert_win_rate_verdict_and_exit_code() {
        let fourchette = WinRateRange {
            min: 2500,
            max: 4000,
        };

        for mesure in [2500, 3200, 4000] {
            assert_eq!(
                verdict(mesure, fourchette),
                Verdict {
                    code: 0,
                    stderr: None
                },
                "{mesure} est dans la fourchette"
            );
        }

        // La seconde fourchette porte une borne à fraction courte : sans elle,
        // un taux affiché sans son remplissage passerait le test.
        for (mesure, fourchette) in [
            (2499, fourchette),
            (4001, fourchette),
            (100, WinRateRange { min: 250, max: 625 }),
        ] {
            let rendu = verdict(mesure, fourchette);
            assert_eq!(rendu.code, 1, "{mesure} est hors fourchette");
            let ligne = rendu.stderr.unwrap_or_default();
            // La valeur mesurée **et** l'intervalle attendu, sinon la ligne ne
            // dit pas de combien la porte a manqué.
            let taux = |valeur: u32| format!("{}.{:04}", valeur / 10_000, valeur % 10_000);
            for attendu in [taux(fourchette.min), taux(fourchette.max), taux(mesure)] {
                assert!(
                    ligne.contains(&attendu),
                    "« {ligne} » ne porte pas {attendu}"
                );
            }
        }
    }

    #[test]
    fn test_cup_names_are_pinned() {
        // Les noms de la ligne de commande viennent de `Debug`. Ce test les
        // épingle : un renommage amont devient un échec de test, et non un
        // contrat de CLI changé sans bruit.
        assert_eq!(
            cups_par_defaut()
                .into_iter()
                .map(cup_nom)
                .collect::<Vec<_>>(),
            ["standard", "abandoned", "polyhedron", "cheater", "fortune"]
        );
        for cup in cups_par_defaut() {
            assert_eq!(parse_cup(&cup_nom(cup)), Ok(cup));
        }
        assert!(parse_cup("inconnu").is_err());
    }

    #[test]
    fn test_default_matrix_is_the_stake_triplet() {
        assert_eq!(stakes_par_defaut(), vec![1, 3, 4]);
        assert_eq!(cups_par_defaut().len(), 5);
        assert_eq!(
            cup_suivant(CupId::Fortune),
            None,
            "la chaîne doit se terminer"
        );
    }
}
