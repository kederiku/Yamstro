//! La sonde attentive à la grille consommable.
//!
//! **Elle produit le résultat le plus important de l'étape.** L'ADR-001 pose
//! que la grille du Yams est consommable, et que c'est cette contrainte qui
//! transforme « quelle est ma meilleure main ? » en une décision. Personne ne
//! sait si elle en crée réellement une : l'écart entre cette sonde et la
//! gloutonne, en points de taux de victoire, le mesure directement. S'il est
//! nul, la grille est décorative et il faut la revoir **avant** que l'Étape 9
//! ne construise soixante reliques par-dessus.
//!
//! # Même évaluation, arbitrage différent
//!
//! Elle chiffre les candidats **exactement comme la gloutonne** — et relance
//! exactement comme elle. Seule la **figure soumise** diffère. C'est ce qui
//! rend l'écart interprétable : **une seule différence à la fois**. Si les deux
//! sondes ne relançaient pas de la même façon, l'écart mesuré serait dominé par
//! « l'une relance, l'autre pas » plutôt que par la profondeur de la grille.
//!
//! La gloutonne prend le plus gros score immédiat ; celle-ci prend **le plus
//! petit score suffisant**, et n'ouvre une case chère que si la manche l'exige.
//!
//! # Ce qu'elle ne fait pas
//!
//! Elle **n'anticipe pas au-delà de la manche courante** : le besoin porte sur
//! la manche, pas sur la run. Elle ne modélise pas les reliques — l'aperçu est
//! le même pour les deux sondes. Elle n'écrit **aucun seuil de jouabilité** :
//! tout arrive par la vue, sinon l'instrument mesurerait ses propres
//! constantes. Et elle ne consomme aucun flux de la run : toute simulation de
//! jet passerait par un clone, et c'est elle qui en sera le plus tentée.

use crate::policy::{HandDecision, Policy};
use crate::run::figure_jouable;
use crate::view::{HandView, LockMask};
use core_engine::blinds::BlindContext;
use core_engine::evaluator::HandMatch;
use core_engine::hands::YahtzeeHand;
use rand_chacha::ChaCha8Rng;

#[derive(Debug, Default)]
pub struct GridAwarePolicy {
    dernier: Option<(u8, u64)>,
}

impl GridAwarePolicy {
    #[must_use]
    pub fn new() -> Self {
        Self { dernier: None }
    }

    fn apercu_precedent(&self, relances: u8) -> Option<u64> {
        match self.dernier {
            Some((vues, apercu)) if relances < vues => Some(apercu),
            _ => None,
        }
    }
}

/// Ce qu'il reste à produire, réparti sur les mains restantes.
///
/// **Arithmétique entière saturante, aucun flottant.** Une répartition en
/// flottant ferait diverger la décision entre plateformes, et deux d'entre
/// elles ne rendraient pas le même tableau pour la même graine.
///
/// La soustraction sature : une manche déjà dépassée rend **zéro**, jamais un
/// entier immense — la sonde se croirait désespérée, ouvrirait ses cases les
/// plus chères, et l'écart mesuré deviendrait faux dans le sens qui fait
/// paraître l'ADR-001 inutile. Le diviseur est plancher à un : une division par
/// zéro est une panique franche au milieu de dix mille runs, et le message ne
/// dirait pas quelle graine l'a produite.
///
/// **La troncature est volontaire et sans correction** : elle sous-estime le
/// besoin, donc rend la sonde légèrement plus prudente. Le biais est connu,
/// constant et identique partout — les trois propriétés qu'on demande à un
/// instrument. L'arrondir introduirait un second comportement à justifier.
pub(crate) fn besoin(blind: &BlindContext) -> u64 {
    let restant = blind.target_score.saturating_sub(blind.current_score);
    restant / u64::from(blind.hands_remaining.max(1))
}

/// L'arbitrage sur la grille, en quatre règles dans cet ordre.
///
/// La quatrième — **le plus petit candidat suffisant** — est celle qui porte
/// toute la différence avec la gloutonne, et le document source ne l'énonce que
/// comme une formule. Elle remplace naturellement le sacrifice de la case de
/// dump quand celle-ci est déjà consommée : le plus petit suffisant est alors
/// le sacrifice le moins cher.
fn figure_a_soumettre(
    view: &HandView<'_>,
    disponibles: &[&HandMatch],
    meilleur: &HandMatch,
) -> YahtzeeHand {
    let besoin = besoin(view.blind);
    let suffisants: Vec<&&HandMatch> = disponibles
        .iter()
        .filter(|figure| figure.potential_score >= besoin)
        .collect();

    // **Rien n'atteint le besoin : la sonde décide comme la gloutonne.** C'est
    // voulu — sous pression maximale la grille ne laisse aucune latitude —, et
    // c'est aussi ce qui explique qu'une part des décisions coïncide.
    let Some(premier) = suffisants.first() else {
        return meilleur.hand;
    };

    // **La case de dump se sacrifie en priorité** quand la manche est acquise :
    // tout tirage la produit, elle n'exige aucune combinaison, et la consommer
    // tôt ne ferme aucune porte.
    if suffisants
        .iter()
        .any(|figure| figure.hand == YahtzeeHand::Chance)
    {
        return YahtzeeHand::Chance;
    }

    // **La case la plus chère se conserve tant qu'un autre candidat suffit.**
    // L'ouvrir par réflexe est précisément ce que la gloutonne incarne.
    disponibles
        .iter()
        .filter(|figure| figure.potential_score >= besoin && figure.hand != YahtzeeHand::Yahtzee)
        .min_by_key(|figure| figure.potential_score)
        .map_or(premier.hand, |figure| figure.hand)
}

impl Policy for GridAwarePolicy {
    fn name(&self) -> &'static str {
        "grid-aware"
    }

    fn decide(&mut self, view: &HandView<'_>, _rng: &mut ChaCha8Rng) -> HandDecision {
        let disponibles: Vec<&HandMatch> = view
            .matches
            .iter()
            .filter(|figure| figure_jouable(view.blind, figure.hand))
            .collect();
        let Some(meilleur) = disponibles.first().copied() else {
            return HandDecision::Submit(
                view.matches
                    .first()
                    .map_or(YahtzeeHand::Chance, |figure| figure.hand),
            );
        };

        let precedent = self.apercu_precedent(view.rerolls_left);
        self.dernier = Some((view.rerolls_left, meilleur.potential_score));
        let complete = meilleur.scoring_dice.len() >= view.dice.len();
        if view.rerolls_left > 0
            && !complete
            && precedent.is_none_or(|avant| meilleur.potential_score > avant)
        {
            return HandDecision::Reroll(LockMask::new(meilleur.scoring_dice.iter().copied()));
        }

        HandDecision::Submit(figure_a_soumettre(view, &disponibles, meilleur))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::policy::ShopPolicy;
    use crate::policy::greedy::GreedyPolicy;
    use crate::run::simulate_with;
    use crate::view::{ShopAction, ShopView};
    use core_engine::blinds::{BlindDefinition, BlindModifier, BlindType};
    use core_engine::config::RunConfig;
    use core_engine::cups::CupId;
    use core_engine::cups::definitions::cup;
    use core_engine::dice::{Die, DieId};
    use core_engine::evaluator::{HandEvaluator, rescore_with_levels};
    use core_engine::hands::{HandGrid, HandLevels};
    use core_engine::pool::DicePool;
    use core_engine::relics::RelicInventory;
    use core_engine::rng::RunRng;
    use rand_chacha::rand_core::{Rng, SeedableRng};
    use smallvec::{SmallVec, smallvec};

    fn des(n: usize) -> Vec<Die> {
        (0..n).map(|rang| Die::new(DieId(rang as u32), 6)).collect()
    }

    /// Une figure synthétique : l'arbitrage ne lit que la figure, son aperçu et
    /// ses dés marquants. Les construire à la main rend chaque scénario exact.
    fn figure(hand: YahtzeeHand, apercu: u64, marquants: usize) -> HandMatch {
        HandMatch {
            hand,
            scoring_dice: (0..marquants).map(|r| DieId(r as u32)).collect(),
            discarded_dice: Vec::new(),
            potential_score: apercu,
        }
    }

    /// Les figures **triées par aperçu décroissant**, comme l'évaluateur les rend.
    fn triees(mut liste: Vec<HandMatch>) -> Vec<HandMatch> {
        liste.sort_by_key(|figure| core::cmp::Reverse(figure.potential_score));
        liste
    }

    fn manche(cible: u64, score: u64, mains: u8, grille: HandGrid) -> BlindContext {
        BlindContext {
            blind: BlindDefinition {
                kind: BlindType::Small,
                target_score: cible,
                reward: 3,
                modifier: None,
            },
            target_score: cible,
            current_score: score,
            hands_remaining: mains,
            used_hands: grille,
        }
    }

    struct Decor {
        des: Vec<Die>,
        niveaux: HandLevels,
        stock: RelicInventory,
    }

    fn decor(n: usize) -> Decor {
        let config = RunConfig::from_cup(&cup(CupId::Standard));
        Decor {
            des: des(n),
            niveaux: HandLevels::default(),
            stock: RelicInventory::new(config.relic_capacity),
        }
    }

    fn vue<'a>(
        d: &'a Decor,
        figures: &'a [HandMatch],
        blind: &'a BlindContext,
        relances: u8,
    ) -> HandView<'a> {
        HandView {
            dice: &d.des,
            matches: figures,
            blind,
            rerolls_left: relances,
            hand_levels: &d.niveaux,
            relics: &d.stock,
        }
    }

    struct Passive;
    impl ShopPolicy for Passive {
        fn name(&self) -> &'static str {
            "passive"
        }
        fn decide(
            &mut self,
            _view: &ShopView<'_>,
            _rng: &mut ChaCha8Rng,
        ) -> SmallVec<[ShopAction; 4]> {
            smallvec![ShopAction::Leave]
        }
    }

    fn campagne() -> SimConfig {
        SimConfig {
            runs: 1,
            seed_base: 1,
            cups: vec![CupId::Standard],
            stakes: vec![1],
            policy: crate::config::PolicyKind::GridAware,
            shop_policy: crate::config::ShopPolicyKind::Budget,
            threads: 1,
        }
    }

    #[test]
    fn test_grid_aware_sacrifices_chance_first() {
        // Manche acquise : plusieurs candidats dépassent le besoin.
        //
        // **La case de dump n'est pas le plus petit suffisant, et c'est le
        // point.** Avec un scénario où elle l'est, la règle du plus petit
        // suffisant rendrait la même réponse et la priorité de la case de dump
        // ne serait gardée par rien — mesuré au banc.
        let d = decor(5);
        let figures = triees(vec![
            figure(YahtzeeHand::Yahtzee, 400, 5),
            figure(YahtzeeHand::Chance, 300, 5),
            figure(YahtzeeHand::Sixes, 200, 5),
            figure(YahtzeeHand::Fours, 100, 5),
        ]);
        let m = manche(400, 300, 4, HandGrid::default());
        assert_eq!(besoin(&m), 25, "scénario : tous les candidats suffisent");

        let mut sonde = GridAwarePolicy::new();
        let mut flux = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(
            sonde.decide(&vue(&d, &figures, &m, 0), &mut flux),
            HandDecision::Submit(YahtzeeHand::Chance),
            "la case de dump n'a pas été sacrifiée en premier"
        );

        // La gloutonne, elle, prend le plus gros immédiat.
        let mut gloutonne = GreedyPolicy::new();
        assert_eq!(
            gloutonne.decide(&vue(&d, &figures, &m, 0), &mut flux),
            HandDecision::Submit(YahtzeeHand::Yahtzee)
        );
    }

    #[test]
    fn test_grid_aware_takes_the_smallest_sufficient() {
        // **Deux suffisants hors case chère, d'aperçus distincts** : sans cela,
        // le plus petit et le plus grand se confondent et la règle n'est gardée
        // par rien.
        let d = decor(5);
        let figures = triees(vec![
            figure(YahtzeeHand::Yahtzee, 400, 5),
            figure(YahtzeeHand::Sixes, 200, 5),
            figure(YahtzeeHand::Fours, 100, 5),
        ]);
        let m = manche(400, 300, 4, HandGrid::default());
        assert_eq!(besoin(&m), 25);

        let mut flux = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(
            GridAwarePolicy::new().decide(&vue(&d, &figures, &m, 0), &mut flux),
            HandDecision::Submit(YahtzeeHand::Fours),
            "la sonde n'a pas pris le plus petit suffisant"
        );
    }

    #[test]
    fn test_grid_aware_keeps_yahtzee_while_reachable() {
        let d = decor(5);
        let mut flux = ChaCha8Rng::seed_from_u64(1);

        // **La case chère porte ici le plus petit aperçu suffisant**, et elle
        // n'est pourtant pas ouverte : la règle tient sur la **case**, pas sur
        // la grandeur de l'aperçu. Avec un scénario où elle porte le plus gros,
        // la règle du plus petit suffisant l'écarterait déjà et celle-ci ne
        // serait gardée par rien — mesuré au banc.
        let renversees = triees(vec![
            figure(YahtzeeHand::Sixes, 300, 5),
            figure(YahtzeeHand::Yahtzee, 100, 5),
        ]);
        let confortable = manche(400, 300, 4, HandGrid::default());
        assert!(besoin(&confortable) <= 100, "les deux suffisent");
        assert_eq!(
            GridAwarePolicy::new().decide(&vue(&d, &renversees, &confortable, 0), &mut flux),
            HandDecision::Submit(YahtzeeHand::Sixes),
            "la case la plus chère a été ouverte sans nécessité"
        );

        // Plus atteignable sans elle : seule la case chère couvre le besoin.
        let figures = triees(vec![
            figure(YahtzeeHand::Yahtzee, 500, 5),
            figure(YahtzeeHand::FullHouse, 300, 5),
        ]);
        let serre = manche(1_800, 0, 4, HandGrid::default());
        assert_eq!(besoin(&serre), 450);
        assert_eq!(
            GridAwarePolicy::new().decide(&vue(&d, &figures, &serre, 0), &mut flux),
            HandDecision::Submit(YahtzeeHand::Yahtzee),
            "la manche exigeait la case chère"
        );
    }

    #[test]
    fn test_grid_aware_opens_expensive_hand_only_when_needed() {
        // **Même main, deux manches ne différant que par le score courant.**
        let d = decor(5);
        let figures = triees(vec![
            figure(YahtzeeHand::Yahtzee, 500, 5),
            figure(YahtzeeHand::Sixes, 200, 5),
        ]);
        let mut flux = ChaCha8Rng::seed_from_u64(1);

        let confortable = manche(1_000, 400, 3, HandGrid::default());
        let serre = manche(1_000, 0, 3, HandGrid::default());
        assert_eq!(besoin(&confortable), 200);
        assert_eq!(besoin(&serre), 333);

        assert_eq!(
            GridAwarePolicy::new().decide(&vue(&d, &figures, &confortable, 0), &mut flux),
            HandDecision::Submit(YahtzeeHand::Sixes)
        );
        assert_eq!(
            GridAwarePolicy::new().decide(&vue(&d, &figures, &serre, 0), &mut flux),
            HandDecision::Submit(YahtzeeHand::Yahtzee)
        );
    }

    #[test]
    fn test_grid_aware_matches_greedy_under_pressure() {
        // **Quand rien n'atteint le besoin, les deux sondes décident pareil.**
        // C'est voulu : sous pression maximale la grille ne laisse aucune
        // latitude, et c'est aussi ce qui explique qu'une part des décisions
        // coïncide.
        let d = decor(5);
        let figures = triees(vec![
            figure(YahtzeeHand::Yahtzee, 500, 5),
            figure(YahtzeeHand::Chance, 120, 5),
        ]);
        let desespere = manche(100_000, 0, 4, HandGrid::default());
        assert!(besoin(&desespere) > 500);

        let mut flux = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(
            GridAwarePolicy::new().decide(&vue(&d, &figures, &desespere, 0), &mut flux),
            GreedyPolicy::new().decide(&vue(&d, &figures, &desespere, 0), &mut flux)
        );
    }

    #[test]
    fn test_grid_aware_rerolls_like_greedy() {
        // **Une seule différence à la fois.** Si les deux sondes ne relançaient
        // pas de la même façon, l'écart mesuré serait dominé par « l'une
        // relance, l'autre pas » plutôt que par la profondeur de la grille.
        let d = decor(5);
        let partielle = triees(vec![
            figure(YahtzeeHand::ThreeOfAKind, 200, 3),
            figure(YahtzeeHand::Chance, 120, 5),
        ]);
        let m = manche(1_000, 0, 4, HandGrid::default());
        let mut flux = ChaCha8Rng::seed_from_u64(1);

        let mut sonde = GridAwarePolicy::new();
        let mut gloutonne = GreedyPolicy::new();
        assert_eq!(
            sonde.decide(&vue(&d, &partielle, &m, 2), &mut flux),
            gloutonne.decide(&vue(&d, &partielle, &m, 2), &mut flux),
            "les deux sondes ne relancent pas de la même façon"
        );
        // Et le second cycle sans progrès arrête les deux.
        assert_eq!(
            sonde.decide(&vue(&d, &partielle, &m, 1), &mut flux),
            gloutonne.decide(&vue(&d, &partielle, &m, 1), &mut flux)
        );
    }

    #[test]
    fn test_needed_uses_saturating_integer_arithmetic() {
        // Manche déjà dépassée : zéro, jamais un entier immense.
        let depassee = manche(300, 900, 4, HandGrid::default());
        assert_eq!(besoin(&depassee), 0);
        // Aucune main restante : aucune division par zéro.
        let epuisee = manche(300, 0, 0, HandGrid::default());
        assert_eq!(besoin(&epuisee), 300);
    }

    #[test]
    fn test_grid_aware_never_picks_a_used_hand() {
        let mut rng = RunRng::from_seed(31);
        let deck = cup(CupId::Standard);
        let config = RunConfig::from_cup(&deck);
        let niveaux = HandLevels::default();
        let stock = RelicInventory::new(config.relic_capacity);
        let mut flux = ChaCha8Rng::seed_from_u64(3);

        for tour in 0..1_000u32 {
            let mut pool = DicePool::new(&config, &deck.sides);
            pool.roll_all(&mut rng.dice, true);
            let lances = pool.dice().to_vec();
            let mut figures = HandEvaluator::evaluate(&lances);
            rescore_with_levels(&mut figures, &lances, &niveaux);

            let consommees = figures.len().saturating_sub(1).min(5);
            let mut grille = HandGrid::default();
            for f in figures.iter().take(consommees) {
                grille.mark(f.hand);
            }
            let m = manche(1_000, 0, 4, grille);
            let d = Decor {
                des: lances,
                niveaux: niveaux.clone(),
                stock: stock.clone(),
            };

            if let HandDecision::Submit(hand) =
                GridAwarePolicy::new().decide(&vue(&d, &figures, &m, 0), &mut flux)
            {
                assert!(
                    figure_jouable(&m, hand),
                    "tour {tour} : {hand:?} indisponible"
                );
            }
        }
    }

    #[test]
    fn test_grid_aware_does_not_advance_run_rng() {
        let d = decor(5);
        let figures = triees(vec![figure(YahtzeeHand::Chance, 120, 5)]);
        let m = manche(400, 0, 4, HandGrid::default());
        let temoin = RunRng::from_seed(4);
        let empreintes = |rng: &RunRng| {
            let tirer = |flux: &ChaCha8Rng| {
                let mut clone = flux.clone();
                (0..4).map(|_| clone.next_u64()).collect::<Vec<u64>>()
            };
            [
                tirer(&rng.dice),
                tirer(&rng.shop),
                tirer(&rng.boss),
                tirer(&rng.relic_effects),
            ]
        };
        let avant = empreintes(&temoin);

        let mut flux = ChaCha8Rng::seed_from_u64(9);
        GridAwarePolicy::new().decide(&vue(&d, &figures, &m, 2), &mut flux);
        assert_eq!(empreintes(&temoin), avant, "un flux de la run a avancé");
    }

    #[test]
    fn test_grid_aware_is_deterministic() {
        let d = decor(5);
        let figures = triees(vec![
            figure(YahtzeeHand::Yahtzee, 500, 4),
            figure(YahtzeeHand::Chance, 120, 5),
        ]);
        let m = manche(400, 300, 4, HandGrid::default());
        let mut reference = None;
        for _ in 0..100 {
            let mut flux = ChaCha8Rng::seed_from_u64(3);
            let decision = GridAwarePolicy::new().decide(&vue(&d, &figures, &m, 2), &mut flux);
            match &reference {
                None => reference = Some(decision),
                Some(attendu) => assert_eq!(&decision, attendu),
            }
        }
    }

    #[test]
    fn test_grid_aware_differs_from_greedy_on_at_least_one_seed() {
        // **Contrôle de bon sens.** Si les deux sondes rendaient toujours le
        // même résultat, la mesure de l'ADR-001 serait vide avant même la
        // campagne, et le défaut serait ici — pas dans les chiffres.
        let config = campagne();
        let divergentes = (1..=50u64)
            .filter(|graine| {
                simulate_with(&config, *graine, &mut GridAwarePolicy::new(), &mut Passive)
                    != simulate_with(&config, *graine, &mut GreedyPolicy::new(), &mut Passive)
            })
            .count();
        assert!(
            divergentes > 0,
            "les deux sondes rendent le même résultat sur cinquante graines"
        );
    }

    #[test]
    fn test_grid_aware_avoids_debuffed_hands() {
        let d = decor(5);
        let figures = triees(vec![
            figure(YahtzeeHand::Chance, 500, 5),
            figure(YahtzeeHand::FullHouse, 300, 5),
        ]);
        let mut m = manche(400, 300, 4, HandGrid::default());
        m.blind.modifier = Some(BlindModifier::DebuffHands(smallvec![YahtzeeHand::Chance]));

        let mut flux = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(
            GridAwarePolicy::new().decide(&vue(&d, &figures, &m, 0), &mut flux),
            HandDecision::Submit(YahtzeeHand::FullHouse),
            "la figure interdite a été sacrifiée"
        );
    }
}
