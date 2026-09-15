//! Ce qu'un run produit : **la ligne du tableau, et le second canal**.
//!
//! # Deux livrables, deux types, et c'est tout l'intérêt
//!
//! Le tableau de sortie est un livrable de **tableur** : une ligne d'en-tête,
//! une ligne par run, rien d'autre — ni préambule, ni ligne de totaux, ni ligne
//! vide finale. Il se trie et se croise sans retouche.
//!
//! Les agrégats sont un livrable de **rapport**, et ils portent ce que les
//! colonnes ne peuvent pas porter : le rapport score sur cible par ante, et le
//! sort de chaque relique rencontrée. Les faire entrer dans le tableau
//! ajouterait huit colonnes par ante et quatre par relique — plus de soixante
//! cellules par ligne aujourd'hui, plusieurs centaines à l'Étape 9. Le fichier
//! ne se trierait plus, et le tableau croisé deviendrait illisible. Les
//! confondre coûte donc l'un ou l'autre.
//!
//! # Ni l'un ni l'autre n'est muté après construction
//!
//! Une ligne est **produite par un run et jamais touchée ensuite**. Aucune
//! méthode qui écrive, aucun mutateur : un agrégat qui repasserait sur les
//! résultats pour « normaliser » une colonne changerait ce que le tableau dit
//! d'un run **après** que le run l'a dit, et le fichier cesserait d'être
//! l'enregistrement de la campagne pour devenir sa réécriture.
//!
//! # Les deux grandeurs que ce type ne porte plus, et comment les retrouver
//!
//! Le tableau normatif ne liste ni le nombre de manches battues, ni le nombre
//! de mains jouées, et aucune colonne surnuméraire n'est permise. **Les deux se
//! reconstruisent**, exactement :
//!
//! - les **mains jouées** sont la somme des treize compteurs de figures ;
//! - les **manches battues** se lisent de l'ante et de la manche fatale, par
//!   `blinds_cleared` ci-dessous.
//!
//! La seconde compte : la colonne de victoire vaut `false` sur toute une
//! campagne au calibrage actuel, et l'ante atteint vaut un pour la majorité des
//! runs. **Les manches battues sont la grandeur trois fois plus fine** sur
//! laquelle les mesures de cette étape portent, et la reconstruction est écrite
//! ici une fois pour que le rapport et la porte de CI la lisent au même
//! endroit plutôt que de la réinventer chacun.

use core_engine::blinds::BlindType;
use core_engine::hands::YahtzeeHand;
use core_engine::relics::{RelicId, RelicInventory};
use core_engine::scoring::{ScoreStep, StepSource};

/// Le nombre d'antes d'une run complète.
pub const ANTES: usize = 8;
/// Les trois rangs de manche d'un ante.
pub const RANGS_PAR_ANTE: u8 = 3;
/// L'unité des taux du harnais : **aucun flottant, jamais**. Un taux comparé en
/// flottant fait diverger une porte d'intégration d'une plateforme à l'autre, et
/// personne ne saurait dire laquelle des deux ment.
pub const DIX_MILLIEMES: u32 = 10_000;

/// Le nom d'un rang de manche. **`match` total, sans bras attrape-tout** : un
/// quatrième rang à l'Étape 9 doit être une erreur de compilation, jamais une
/// cellule inconnue qui traverserait la campagne et le rapport sans que
/// personne ne la voie.
#[must_use]
pub fn blind_name(kind: BlindType) -> &'static str {
    match kind {
        BlindType::Small => "small",
        BlindType::Big => "big",
        BlindType::Boss => "boss",
    }
}

/// Le nom d'une figure, en minuscules séparées. **Même discipline**, et
/// **c'est aussi lui qui nomme les treize colonnes** : le nom de colonne est ce
/// nom préfixé. Une quatorzième figure fait échouer la compilation ici, et le
/// test d'en-tête refuse toute dérive d'ordre.
#[must_use]
pub fn hand_name(hand: YahtzeeHand) -> &'static str {
    match hand {
        YahtzeeHand::Aces => "aces",
        YahtzeeHand::Twos => "twos",
        YahtzeeHand::Threes => "threes",
        YahtzeeHand::Fours => "fours",
        YahtzeeHand::Fives => "fives",
        YahtzeeHand::Sixes => "sixes",
        YahtzeeHand::ThreeOfAKind => "three_of_a_kind",
        YahtzeeHand::FourOfAKind => "four_of_a_kind",
        YahtzeeHand::FullHouse => "full_house",
        YahtzeeHand::SmallStraight => "small_straight",
        YahtzeeHand::LargeStraight => "large_straight",
        YahtzeeHand::Yahtzee => "yahtzee",
        YahtzeeHand::Chance => "chance",
    }
}

/// Le nom d'une relique, **dérivé mécaniquement** et non par un `match`.
///
/// Les gobelets et les rangs de manche reçoivent un `match` total parce qu'une
/// variante ajoutée y demande une **décision** — celle du nom affiché. Une
/// relique n'en demande aucune : son nom se déduit de son identifiant, et
/// l'Étape 9 en versera soixante. Un `match` de soixante bras ne garderait rien
/// que cette dérivation ne garde déjà.
#[must_use]
pub fn relic_name(def: RelicId) -> String {
    let brut = format!("{def:?}");
    let mut nom = String::with_capacity(brut.len() + 4);
    for (rang, lettre) in brut.chars().enumerate() {
        if lettre.is_ascii_uppercase() && rang > 0 {
            nom.push('_');
        }
        nom.extend(lettre.to_lowercase());
    }
    nom
}

/// La cellule des reliques finales : les noms **dans l'ordre des slots**,
/// joints par un point-virgule.
///
/// **Elle ne trie pas.** L'ordre des slots **est** l'ordre d'application
/// (ADR-005) : trié — ou inversé —, le tableau décrirait un build que le run
/// n'a pas joué, et la contribution mesurée par relique ne correspondrait plus
/// à l'ordre qui l'a produite.
#[must_use]
pub fn relics_cell(inventory: &RelicInventory) -> String {
    let mut cellule = String::new();
    for (_, inst) in inventory.iter_slots() {
        if !cellule.is_empty() {
            cellule.push(';');
        }
        cellule.push_str(&relic_name(inst.def));
    }
    cellule
}

/// Ce qu'un run produit, **une ligne de tableau**.
///
/// **L'ordre de déclaration est l'ordre des colonnes** : c'est ainsi que la
/// sérialisation construit l'en-tête. Il ne se réordonne pas « pour la
/// lisibilité », et aucun renommage n'est permis — les noms de champs **sont**
/// les noms de colonnes.
///
/// Les deux champs de défaut de politique ne sont pas des colonnes : ils
/// portent l'attribut qui les retire de l'en-tête, et le rapport les lit **sur
/// la structure**. Ce sont les deux seuls, et il n'y en aura pas de troisième :
/// tout ce que le rapport demande en plus sort par le second canal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RunOutcome {
    pub seed: u64,
    pub cup: String,
    pub stake: u8,
    pub policy: &'static str,
    pub shop_policy: &'static str,
    pub ante_reached: u8,
    pub victory: bool,
    /// **Vide sur une victoire, jamais zéro.** Un zéro se trie et s'agrège
    /// comme une valeur : le tableau croisé compterait les victoires parmi les
    /// défaites d'un ante qui n'existe pas, et la moyenne de l'ante de défaite
    /// baisserait d'autant que la campagne est **bonne**. Plus le jeu
    /// s'équilibre, plus le chiffre ment — et personne ne cherche un défaut
    /// dans un chiffre qui s'améliore.
    pub defeat_ante: Option<u8>,
    /// Même traitement, pour la même raison.
    pub defeat_blind: Option<&'static str>,
    pub gold_earned: u32,
    pub gold_final: u32,
    pub interest_earned: u32,
    pub hands_aces: u16,
    pub hands_twos: u16,
    pub hands_threes: u16,
    pub hands_fours: u16,
    pub hands_fives: u16,
    pub hands_sixes: u16,
    pub hands_three_of_a_kind: u16,
    pub hands_four_of_a_kind: u16,
    pub hands_full_house: u16,
    pub hands_small_straight: u16,
    pub hands_large_straight: u16,
    pub hands_yahtzee: u16,
    pub hands_chance: u16,
    /// Les reliques possédées en fin de run, **dans l'ordre des slots**, en une
    /// seule cellule jointe par un point-virgule.
    ///
    /// Le point-virgule n'est pas arbitraire : le séparateur du fichier étant
    /// la virgule, une cellule jointe ainsi **ne demande aucun échappement** et
    /// se lit nue. Jointe par des virgules, elle serait citée, la ligne
    /// changerait de largeur selon le nombre de reliques, et certains lecteurs
    /// afficheraient les guillemets.
    ///
    /// **Elle ne se trie pas** : l'ordre des slots **est** l'ordre
    /// d'application (ADR-005), et un tri ferait décrire au tableau un build
    /// que le run n'a pas joué.
    pub relics_final: String,
    pub max_hand_score: u64,
    pub max_hand_figure: Option<&'static str>,
    /// Défauts de politique rencontrés. **Ce n'est pas un invariant mais une
    /// statistique** : le repli d'une sonde de main qui n'a plus aucune figure
    /// jouable se compte ici plutôt qu'il ne se rattrape en silence, et une
    /// campagne en porte quelques-uns.
    #[serde(skip)]
    pub anomalies: u32,
    /// Un run abandonné **n'entre pas** dans les agrégats de taux de victoire.
    #[serde(skip)]
    pub abandoned: bool,
}

impl RunOutcome {
    /// Les manches battues, **reconstruites** — le tableau normatif ne porte
    /// pas la colonne, et aucune colonne surnuméraire n'est permise.
    ///
    /// Mourir à l'ante `A` sur la manche de rang `B` (zéro pour la première),
    /// c'est avoir battu `(A - 1) × 3 + B` manches ; une victoire en vaut
    /// vingt-quatre. **Écrite ici une seule fois** : le rapport et la porte de
    /// CI la lisent, ils ne la réinventent pas.
    #[must_use]
    pub fn blinds_cleared(&self) -> u8 {
        if self.victory {
            return ANTES as u8 * RANGS_PAR_ANTE;
        }
        let Some(ante) = self.defeat_ante else {
            return 0;
        };
        let rang = self.defeat_blind.map_or(0, |nom| {
            u8::try_from(
                BLINDS_DANS_L_ORDRE
                    .iter()
                    .position(|kind| blind_name(*kind) == nom)
                    .unwrap_or_default(),
            )
            .unwrap_or_default()
        });
        ante.saturating_sub(1)
            .saturating_mul(RANGS_PAR_ANTE)
            .saturating_add(rang)
    }

    /// Les mains jouées, somme des treize compteurs.
    #[must_use]
    pub fn hands_played(&self) -> u32 {
        u32::from(self.hands_aces)
            + u32::from(self.hands_twos)
            + u32::from(self.hands_threes)
            + u32::from(self.hands_fours)
            + u32::from(self.hands_fives)
            + u32::from(self.hands_sixes)
            + u32::from(self.hands_three_of_a_kind)
            + u32::from(self.hands_four_of_a_kind)
            + u32::from(self.hands_full_house)
            + u32::from(self.hands_small_straight)
            + u32::from(self.hands_large_straight)
            + u32::from(self.hands_yahtzee)
            + u32::from(self.hands_chance)
    }

    /// Pose les treize compteurs **depuis un tableau indexé par l'ordre de la
    /// liste des figures**, en un seul endroit.
    ///
    /// C'est ce qui rend l'ordre des colonnes **l'ordre normatif de
    /// l'énumération** plutôt qu'une liste recopiée qui pourrait en diverger :
    /// une figure oubliée ou déplacée décalerait treize colonnes sans erreur de
    /// compilation, les Yams seraient comptés comme des Grandes Suites, et la
    /// question « quelles figures le joueur joue-t-il ? » recevrait une réponse
    /// fausse et cohérente. **Mesuré au banc : sans une assertion colonne par
    /// colonne, une permutation de deux d'entre elles survit à toute la suite.**
    ///
    /// **Par valeur, jamais par référence mutable** : la ligne se construit,
    /// elle ne se modifie pas.
    #[must_use]
    pub fn with_hand_counts(mut self, comptes: &[u16; 13]) -> Self {
        let par_figure = |hand: YahtzeeHand| comptes[hand as usize];
        self.hands_aces = par_figure(YahtzeeHand::Aces);
        self.hands_twos = par_figure(YahtzeeHand::Twos);
        self.hands_threes = par_figure(YahtzeeHand::Threes);
        self.hands_fours = par_figure(YahtzeeHand::Fours);
        self.hands_fives = par_figure(YahtzeeHand::Fives);
        self.hands_sixes = par_figure(YahtzeeHand::Sixes);
        self.hands_three_of_a_kind = par_figure(YahtzeeHand::ThreeOfAKind);
        self.hands_four_of_a_kind = par_figure(YahtzeeHand::FourOfAKind);
        self.hands_full_house = par_figure(YahtzeeHand::FullHouse);
        self.hands_small_straight = par_figure(YahtzeeHand::SmallStraight);
        self.hands_large_straight = par_figure(YahtzeeHand::LargeStraight);
        self.hands_yahtzee = par_figure(YahtzeeHand::Yahtzee);
        self.hands_chance = par_figure(YahtzeeHand::Chance);
        self
    }
}

/// Les trois rangs, dans l'ordre où la boucle les joue.
pub const BLINDS_DANS_L_ORDRE: [BlindType; 3] = [BlindType::Small, BlindType::Big, BlindType::Boss];

/// Le second canal : ce que le rapport exige et que les colonnes ne portent
/// pas. **Il n'entre jamais dans le tableau**, et ne dérive donc aucune
/// sérialisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunAggregates {
    /// Rapport score sur cible, en dix-millièmes entiers, indexé par
    /// `ante - 1`.
    ///
    /// **Il porte la dernière manche tentée de l'ante**, et la règle doit être
    /// écrite parce que trois implémentations en donneraient trois : un ante
    /// compte trois manches, avec trois cibles. La dernière tentée est la seule
    /// qui soit définie dès que l'ante est entré — mesuré, la majorité des
    /// antes commencés ne sont pas finis —, et c'est celle qui a arrêté le run.
    ///
    /// **Dense et total** : huit antes, huit cases, un ante jamais entré
    /// portant zéro.
    pub score_vs_target_by_ante: [u32; ANTES],
    /// Une entrée par relique **rencontrée** dans ce run, dans l'ordre du
    /// catalogue.
    pub relics_seen: Vec<RelicEncounter>,
}

/// Le sort d'une relique dans un run.
///
/// **Les trois indicateurs sont des booléens de run, jamais des compteurs** :
/// une relique proposée deux fois dans le même run compte pour une, et la règle
/// s'applique **ici, à la source**. C'est ce qui rend comparables les
/// dénominateurs du rapport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelicEncounter {
    pub def: RelicId,
    pub offered: bool,
    pub bought: bool,
    pub kept: bool,
    /// Somme, en points, des deltas de score des pas dont la source est une
    /// relique, sur toute la run.
    pub contribution: u64,
}

impl RunAggregates {
    #[must_use]
    pub fn neuf() -> Self {
        Self {
            score_vs_target_by_ante: [0; ANTES],
            relics_seen: Vec::new(),
        }
    }

    /// Note une rencontre, **sans jamais compter deux fois** : l'entrée existe
    /// au plus une fois par relique, et un indicateur déjà vrai le reste.
    pub fn noter(&mut self, def: RelicId, offered: bool, bought: bool) {
        if let Some(entree) = self.relics_seen.iter_mut().find(|e| e.def == def) {
            entree.offered |= offered;
            entree.bought |= bought;
            return;
        }
        self.relics_seen.push(RelicEncounter {
            def,
            offered,
            bought,
            kept: false,
            contribution: 0,
        });
    }

    /// Impute aux reliques les pas du journal de score dont elles sont la
    /// source, **par différence d'un pas au suivant**.
    ///
    /// Le journal porte un score **cumulé** : prendre la valeur absolue d'un pas
    /// attribuerait à la relique tout ce que la base et les dés avaient déjà
    /// produit. **Mesuré au banc, la confusion survit à une assertion de
    /// positivité** — il faut comparer des deltas connus.
    pub fn imputer(&mut self, steps: &[ScoreStep]) {
        let mut precedent = 0u64;
        for step in steps {
            let delta = step.score_after.saturating_sub(precedent);
            precedent = step.score_after;
            if let StepSource::Relic { def, .. } = step.source {
                self.contribuer(def, delta);
            }
        }
    }

    /// Ajoute la contribution d'un pas de relique, en créant l'entrée si la
    /// relique n'a jamais été vue en boutique — elle peut venir d'ailleurs.
    pub fn contribuer(&mut self, def: RelicId, points: u64) {
        if let Some(entree) = self.relics_seen.iter_mut().find(|e| e.def == def) {
            entree.contribution = entree.contribution.saturating_add(points);
            return;
        }
        self.relics_seen.push(RelicEncounter {
            def,
            offered: false,
            bought: false,
            kept: false,
            contribution: points,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PolicyKind, ShopPolicyKind, SimConfig, cup_nom};
    use crate::policy::grid_aware::GridAwarePolicy;
    use crate::policy::shop::BudgetShopPolicy;
    use crate::run::simulate_with;
    use core_engine::cups::CupId;
    use core_engine::dice::DieId;
    use core_engine::scoring::{ScoreAction, ScoreStep, StepSource};

    /// Les colonnes du tableau normatif, dans l'ordre, **écrites à la main** :
    /// c'est le contrat que le critère de fin d'étape déclare stable, et le
    /// comparer à une liste dérivée de la structure ne prouverait rien.
    const COLONNES: &str = "seed,cup,stake,policy,shop_policy,ante_reached,victory,\
defeat_ante,defeat_blind,gold_earned,gold_final,interest_earned,\
hands_aces,hands_twos,hands_threes,hands_fours,hands_fives,hands_sixes,\
hands_three_of_a_kind,hands_four_of_a_kind,hands_full_house,\
hands_small_straight,hands_large_straight,hands_yahtzee,hands_chance,\
relics_final,max_hand_score,max_hand_figure";

    fn temoin() -> RunOutcome {
        RunOutcome {
            seed: 7,
            cup: cup_nom(CupId::Standard),
            stake: 1,
            policy: "grid-aware",
            shop_policy: "budget",
            ante_reached: 8,
            victory: true,
            defeat_ante: None,
            defeat_blind: None,
            gold_earned: 120,
            gold_final: 42,
            interest_earned: 30,
            hands_aces: 1,
            hands_twos: 0,
            hands_threes: 0,
            hands_fours: 0,
            hands_fives: 2,
            hands_sixes: 0,
            hands_three_of_a_kind: 0,
            hands_four_of_a_kind: 0,
            hands_full_house: 3,
            hands_small_straight: 0,
            hands_large_straight: 0,
            hands_yahtzee: 0,
            hands_chance: 4,
            relics_final: String::new(),
            max_hand_score: 900,
            max_hand_figure: Some(hand_name(YahtzeeHand::FullHouse)),
            anomalies: 0,
            abandoned: false,
        }
    }

    /// Sérialise et rend le texte entier, terminateur compris.
    fn serialiser(lignes: &[RunOutcome]) -> String {
        let mut writer = csv::Writer::from_writer(Vec::new());
        for ligne in lignes {
            writer.serialize(ligne).expect("sérialisation");
        }
        String::from_utf8(writer.into_inner().expect("vidage")).expect("utf8")
    }

    fn entete_et_lignes(lignes: &[RunOutcome]) -> (String, Vec<String>) {
        let texte = serialiser(lignes);
        let mut iter = texte.lines();
        let entete = iter.next().unwrap_or_default().to_owned();
        (entete, iter.map(str::to_owned).collect())
    }

    fn campagne(cup: CupId) -> SimConfig {
        SimConfig {
            runs: 1,
            seed_base: 1,
            cups: vec![cup],
            stakes: vec![1],
            policy: PolicyKind::GridAware,
            shop_policy: ShopPolicyKind::Budget,
            threads: 1,
        }
    }

    fn un_run(seed: u64) -> (RunOutcome, RunAggregates) {
        let mut politique = GridAwarePolicy::default();
        let mut achat = BudgetShopPolicy::new();
        simulate_with(&campagne(CupId::Standard), seed, &mut politique, &mut achat)
    }

    #[test]
    fn test_csv_header_is_stable() {
        let (entete, lignes) = entete_et_lignes(&[temoin()]);
        assert_eq!(entete, COLONNES);
        assert_eq!(lignes.len(), 1, "une ligne par run, et rien d'autre");
        // Les deux champs de défaut de politique ne sont pas des colonnes.
        assert!(!entete.contains("anomalies"), "{entete}");
        assert!(!entete.contains("abandoned"), "{entete}");
        assert_eq!(entete.matches(',').count(), 27, "vingt-huit colonnes");
    }

    #[test]
    fn test_victory_leaves_defeat_ante_empty() {
        let victorieux = temoin();
        let perdu = RunOutcome {
            victory: false,
            ante_reached: 3,
            defeat_ante: Some(3),
            defeat_blind: Some(blind_name(BlindType::Boss)),
            ..temoin()
        };
        let (_, lignes) = entete_et_lignes(&[victorieux, perdu]);

        // Deux virgules consécutives : la cellule est vide, pas nulle.
        assert!(
            lignes[0].contains("true,,,"),
            "la victoire ne laisse pas les deux cellules vides : {}",
            lignes[0]
        );
        assert!(
            !lignes[0].contains("true,0,"),
            "un zéro s'est glissé dans l'ante de défaite"
        );
        assert!(lignes[1].contains("false,3,boss,"), "{}", lignes[1]);
    }

    #[test]
    fn test_relics_final_is_one_cell() {
        let trois = RunOutcome {
            relics_final: "polished_stone;pyramid_of_sixes;cracked_die".to_owned(),
            ..temoin()
        };
        let vide = temoin();
        let (_, lignes) = entete_et_lignes(&[trois, vide]);

        assert!(
            lignes[0].contains("polished_stone;pyramid_of_sixes;cracked_die"),
            "{}",
            lignes[0]
        );
        assert!(
            !lignes[0].contains('"'),
            "la cellule a été citée : {}",
            lignes[0]
        );
        assert_eq!(
            lignes[0].matches(',').count(),
            lignes[1].matches(',').count(),
            "la ligne change de largeur selon le nombre de reliques"
        );
    }

    #[test]
    fn test_hand_columns_follow_yahtzee_hand_all() {
        let (entete, _) = entete_et_lignes(&[temoin()]);
        let colonnes: Vec<&str> = entete.split(',').collect();
        let attendues: Vec<String> = YahtzeeHand::ALL
            .iter()
            .map(|hand| format!("hands_{}", hand_name(*hand)))
            .collect();
        assert_eq!(colonnes[12..25], attendues[..], "l'ordre a dérivé");
        // Et la tranche est bien contiguë : rien ne s'est glissé au milieu.
        assert_eq!(colonnes.len(), 28);

        // **La moitié qui discrimine, et elle manquait.** Comparer les noms de
        // colonnes ne dit rien de ce qu'on met dedans : mesuré au banc, une
        // permutation de deux compteurs survit à toute la suite si l'on ne
        // vérifie pas **colonne par colonne**. Treize valeurs distinctes, et
        // chacune doit atterrir dans la sienne.
        let comptes: [u16; 13] =
            core::array::from_fn(|rang| u16::try_from(rang).unwrap_or_default().saturating_add(1));
        let (_, lignes) = entete_et_lignes(&[temoin().with_hand_counts(&comptes)]);
        let cellules: Vec<&str> = lignes[0].split(',').collect();
        for (rang, hand) in YahtzeeHand::ALL.iter().enumerate() {
            assert_eq!(
                cellules[12 + rang],
                comptes[rang].to_string(),
                "la colonne de {hand:?} porte le compte d'une autre figure"
            );
        }
    }

    #[test]
    fn test_relics_cell_follows_slot_order() {
        // **Trois reliques dont l'ordre de slot n'est ni alphabétique ni son
        // inverse** : c'est la seule façon de séparer les trois. Mesuré au banc,
        // un tri comme une inversion survivent à une comparaison d'ensembles.
        let capacite = core_engine::config::RunConfig::from_cup(
            &core_engine::cups::definitions::cup(CupId::Standard),
        )
        .relic_capacity;
        let mut inventaire = RelicInventory::new(capacite);
        for def in [
            RelicId::PolishedStone,
            RelicId::CrackedDie,
            RelicId::TripletMaster,
        ] {
            inventaire.add_relic(def);
        }
        assert_eq!(
            relics_cell(&inventaire),
            "polished_stone;cracked_die;triplet_master",
            "l'ordre des slots a été perdu"
        );
        assert_eq!(relics_cell(&RelicInventory::new(capacite)), "");
    }

    #[test]
    fn test_relic_contribution_is_a_delta_not_a_running_total() {
        // Le journal porte un score **cumulé**. Prendre la valeur absolue d'un
        // pas attribuerait à la relique tout ce que la base avait déjà produit.
        let pas = |source, score_after| ScoreStep {
            source,
            action: ScoreAction::AddChips(0),
            chips_after: 0,
            mult_after: 1,
            score_after,
        };
        let steps = vec![
            pas(
                StepSource::HandBase {
                    hand: YahtzeeHand::Chance,
                },
                100,
            ),
            pas(
                StepSource::Relic {
                    uid: 1,
                    def: RelicId::PolishedStone,
                },
                130,
            ),
            pas(
                StepSource::Die {
                    die_id: DieId(0),
                    value: 4,
                },
                150,
            ),
            pas(
                StepSource::Relic {
                    uid: 2,
                    def: RelicId::CrackedDie,
                },
                156,
            ),
        ];
        let mut aggregats = RunAggregates::neuf();
        aggregats.imputer(&steps);

        let par = |def: RelicId| {
            aggregats
                .relics_seen
                .iter()
                .find(|entree| entree.def == def)
                .map_or(0, |entree| entree.contribution)
        };
        assert_eq!(par(RelicId::PolishedStone), 30, "la base a été imputée");
        assert_eq!(par(RelicId::CrackedDie), 6, "le dé a été imputé");
        assert_eq!(
            aggregats.relics_seen.len(),
            2,
            "une source non-relique a été imputée"
        );
    }

    #[test]
    fn test_outcome_is_never_mutated_after_construction() {
        // **La moitié qui mord** : lié sans mutabilité, il ne peut pas être
        // muté. Un mutateur appelé ici ne compilerait pas, et l'invariant
        // textuel — aucune méthode qui écrive — vit en CI, où il ne risque pas
        // d'interdire au test de nommer ce qu'il vérifie.
        let resultat = temoin();
        let (_, aggregats) = un_run(1);
        let _ = serialiser(core::slice::from_ref(&resultat));
        assert_eq!(resultat, temoin(), "la sérialisation a muté la ligne");
        assert_eq!(aggregats.score_vs_target_by_ante.len(), ANTES);
    }

    #[test]
    fn test_cup_and_blind_names_are_total() {
        let mut vus: Vec<String> = Vec::new();
        for cup in [
            CupId::Standard,
            CupId::Abandoned,
            CupId::Polyhedron,
            CupId::Cheater,
            CupId::Fortune,
        ] {
            let nom = cup_nom(cup);
            assert!(!nom.is_empty(), "{cup:?}");
            assert!(!vus.contains(&nom), "{cup:?} porte un nom déjà vu");
            vus.push(nom);
        }
        let mut rangs: Vec<&str> = Vec::new();
        for kind in BLINDS_DANS_L_ORDRE {
            let nom = blind_name(kind);
            assert!(!nom.is_empty());
            assert!(!rangs.contains(&nom), "{kind:?} porte un nom déjà vu");
            rangs.push(nom);
        }
        let mut figures: Vec<&str> = Vec::new();
        for hand in YahtzeeHand::ALL {
            let nom = hand_name(hand);
            assert!(!nom.is_empty());
            assert!(!figures.contains(&nom), "{hand:?} porte un nom déjà vu");
            figures.push(nom);
        }
    }

    #[test]
    fn test_csv_roundtrip_is_byte_stable() {
        assert_eq!(
            serialiser(&[temoin()]).as_bytes(),
            serialiser(&[temoin()]).as_bytes(),
            "deux sérialisations du même contenu diffèrent"
        );
        // Le terminateur est celui qu'on a fixé, et il est **unique** : une
        // campagne comparée octet pour octet entre deux plateformes ne peut pas
        // dépendre d'un défaut implicite.
        let texte = serialiser(&[temoin()]);
        assert!(!texte.contains('\r'), "un retour chariot s'est glissé");
        assert!(
            texte.ends_with('\n'),
            "la dernière ligne n'est pas terminée"
        );
    }

    #[test]
    fn test_aggregates_stay_out_of_the_csv() {
        let (entete, _) = entete_et_lignes(&[temoin()]);
        assert_eq!(entete, COLONNES, "un agrégat est entré dans le tableau");

        // Les trois indicateurs sont des booléens de run : une relique proposée
        // deux fois compte pour une.
        let mut aggregats = RunAggregates::neuf();
        aggregats.noter(RelicId::PolishedStone, true, false);
        aggregats.noter(RelicId::PolishedStone, true, true);
        assert_eq!(aggregats.relics_seen.len(), 1, "l'entrée a été dupliquée");
        assert!(aggregats.relics_seen[0].offered);
        assert!(aggregats.relics_seen[0].bought);

        aggregats.contribuer(RelicId::PolishedStone, 40);
        aggregats.contribuer(RelicId::PolishedStone, 2);
        assert_eq!(aggregats.relics_seen[0].contribution, 42);
        assert_eq!(aggregats.relics_seen.len(), 1);
    }

    #[test]
    fn test_blinds_cleared_is_reconstructed() {
        // La formule, éprouvée contre la boucle sur mille runs : c'est elle qui
        // remplace la colonne que le tableau normatif ne porte pas.
        assert_eq!(
            temoin().blinds_cleared(),
            24,
            "une victoire vaut vingt-quatre"
        );
        for (ante, rang, attendu) in [
            (1u8, BlindType::Small, 0u8),
            (1, BlindType::Big, 1),
            (1, BlindType::Boss, 2),
            (3, BlindType::Small, 6),
            (8, BlindType::Boss, 23),
        ] {
            let perdu = RunOutcome {
                victory: false,
                defeat_ante: Some(ante),
                defeat_blind: Some(blind_name(rang)),
                ..temoin()
            };
            assert_eq!(perdu.blinds_cleared(), attendu, "ante {ante} rang {rang:?}");
        }
    }

    #[test]
    fn test_run_fills_every_column() {
        // La boucle remplit ce que la structure déclare : sans quoi le tableau
        // est un en-tête suivi de zéros.
        let mut mains = 0u32;
        let mut avec_relique = 0u32;
        let mut avec_or = 0u32;
        let mut avec_figure_max = 0u32;
        for seed in 0..200u64 {
            let (resultat, aggregats) = un_run(1_000 + seed);
            assert_eq!(resultat.seed, 1_000 + seed);
            assert_eq!(resultat.cup, cup_nom(CupId::Standard));
            assert_eq!(resultat.policy, "grid-aware");
            assert_eq!(resultat.shop_policy, "budget");
            assert!(resultat.ante_reached >= 1);
            assert_eq!(
                resultat.victory,
                resultat.blinds_cleared() == ANTES as u8 * RANGS_PAR_ANTE
            );
            assert_eq!(resultat.victory, resultat.defeat_ante.is_none());
            assert_eq!(
                resultat.defeat_ante.is_none(),
                resultat.defeat_blind.is_none()
            );
            assert!(resultat.gold_final <= resultat.gold_earned.saturating_add(100));
            // **Une part, jamais le total.** Le gain d'une manche porte au
            // moins la récompense du rang, donc les intérêts lui sont
            // strictement inférieurs dès qu'une manche est battue. Mesuré au
            // banc : une inégalité large laisse survivre l'affectation du total.
            if resultat.gold_earned > 0 {
                assert!(
                    resultat.interest_earned < resultat.gold_earned,
                    "les intérêts valent tout le gain : {} sur {}",
                    resultat.interest_earned,
                    resultat.gold_earned
                );
            }
            mains += resultat.hands_played();
            if !resultat.relics_final.is_empty() {
                avec_relique += 1;
            }
            if resultat.gold_earned > 0 {
                avec_or += 1;
            }
            if resultat.max_hand_figure.is_some() {
                avec_figure_max += 1;
            }
            assert_eq!(
                resultat.max_hand_score > 0,
                resultat.max_hand_figure.is_some(),
                "le score maximum et sa figure se posent ensemble"
            );
            // **La colonne compte des slots, l'indicateur compte des
            // identifiants.** Un doublon de relique dans un même inventaire est
            // légal à cette étape : la colonne le liste deux fois, l'agrégat une
            // seule. Ce sont donc les **ensembles** qui doivent coïncider, pas
            // les cardinaux — et le mesurer autrement ferait échouer le test sur
            // un run parfaitement correct.
            let mut gardees: Vec<String> = aggregats
                .relics_seen
                .iter()
                .filter(|entree| entree.kept)
                .map(|entree| relic_name(entree.def))
                .collect();
            gardees.sort_unstable();
            let mut colonne: Vec<String> = resultat
                .relics_final
                .split(';')
                .filter(|nom| !nom.is_empty())
                .map(str::to_owned)
                .collect();
            colonne.sort_unstable();
            colonne.dedup();
            assert_eq!(
                gardees, colonne,
                "les reliques gardées ne correspondent pas à la colonne"
            );
        }
        assert!(mains > 200, "aucune main comptée : {mains}");
        assert!(avec_relique > 0, "aucun run ne finit avec une relique");
        assert!(avec_or > 0, "aucun run ne gagne d'or");
        assert!(avec_figure_max > 0, "aucune figure maximale");
    }

    #[test]
    fn test_aggregates_record_the_run() {
        let mut avec_ratio = 0u32;
        let mut proposees = 0u32;
        let mut achetees = 0u32;
        let mut contribution = 0u64;
        let mut franchis = 0u32;
        for seed in 0..200u64 {
            let (resultat, aggregats) = un_run(2_000 + seed);
            // Le rapport est posé pour chaque ante **entré**, et zéro ailleurs.
            for (index, ratio) in aggregats.score_vs_target_by_ante.iter().enumerate() {
                let entre = u8::try_from(index + 1).unwrap_or(u8::MAX) <= resultat.ante_reached;
                assert!(
                    entre || *ratio == 0,
                    "ante {} non entré et non nul",
                    index + 1
                );
                if *ratio > 0 {
                    avec_ratio += 1;
                }
            }
            // **La dernière manche tentée, et c'est ce qui discrimine.** L'ante
            // où le run meurt porte un rapport **inférieur à l'unité** — la
            // manche n'a pas été battue ; un ante entièrement franchi porte un
            // rapport **au moins égal**. Mesuré au banc : sans ces deux bornes,
            // poser le rapport de la première manche de l'ante survit.
            if let Some(ante) = resultat.defeat_ante {
                let case = usize::from(ante).saturating_sub(1);
                assert!(
                    aggregats.score_vs_target_by_ante[case] < DIX_MILLIEMES,
                    "l'ante fatal {ante} porte un rapport atteint : {}",
                    aggregats.score_vs_target_by_ante[case]
                );
                for franchi in 1..ante {
                    let case = usize::from(franchi).saturating_sub(1);
                    assert!(
                        aggregats.score_vs_target_by_ante[case] >= DIX_MILLIEMES,
                        "l'ante franchi {franchi} porte un rapport non atteint : {}",
                        aggregats.score_vs_target_by_ante[case]
                    );
                    franchis += 1;
                }
            }
            proposees += u32::try_from(aggregats.relics_seen.iter().filter(|e| e.offered).count())
                .unwrap_or(0);
            achetees += u32::try_from(aggregats.relics_seen.iter().filter(|e| e.bought).count())
                .unwrap_or(0);
            contribution += aggregats
                .relics_seen
                .iter()
                .map(|e| e.contribution)
                .sum::<u64>();
            // Une relique gardée a forcément été achetée.
            for entree in &aggregats.relics_seen {
                assert!(
                    !entree.kept || entree.bought,
                    "{:?} gardée sans achat",
                    entree.def
                );
            }
        }
        assert!(avec_ratio > 0, "aucun rapport score sur cible posé");
        assert!(proposees > 0, "aucune relique proposée");
        assert!(achetees > 0, "aucune relique achetée");
        assert!(contribution > 0, "aucune contribution de relique mesurée");
        assert!(
            franchis > 0,
            "aucun ante entièrement franchi : le test ne prouve rien"
        );
    }
}
