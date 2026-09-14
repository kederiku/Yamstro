//! Le témoin : la borne inférieure.
//!
//! **Il ne sert pas à jouer.** Il répond à une question que rien d'autre ne
//! pose : *l'instrument mesure-t-il quelque chose ?* Si la sonde gloutonne ne
//! dominait pas nettement ce témoin, le harnais ne distinguerait pas un joueur
//! d'un tirage au sort — et **tous** les chiffres qu'il produira ensuite
//! seraient sans valeur : l'écart que mesure la sonde attentive à la grille,
//! les taux de victoire par gobelet, la contribution des reliques. L'instrument
//! serait faux avant le jeu, et rien dans la campagne ne le dirait.
//!
//! # Un tirage par main, et rien d'autre
//!
//! **Il ne relance jamais.** Lui donner un relanceur aléatoire ajouterait une
//! seconde source de variance dont la contribution serait inséparable de la
//! première, et l'écart mesuré porterait sur deux choses à la fois. La borne
//! inférieure cesserait d'être une borne. Le nombre de tirages consommés est
//! donc **constant par main**, ce qui est une propriété vérifiable et non une
//! intention.
//!
//! # Il ne touche aucun flux de la run
//!
//! C'est facile ici, puisqu'il ne spécule pas. Le danger n'est **ni** le
//! pipeline de score **ni** les effets de relique — les deux sont purs et ne
//! prennent aucun générateur : c'est une politique qui **simulerait une
//! relance** pour estimer un candidat. Le test qui l'atteste reste néanmoins le
//! garde-fou, run après run, y compris quand l'Étape 9 aura versé soixante
//! reliques.

use crate::policy::{HandDecision, Policy};
use crate::run::figure_jouable;
use crate::view::HandView;
use core_engine::evaluator::HandMatch;
use core_engine::hands::YahtzeeHand;
use rand::RngExt;
use rand_chacha::ChaCha8Rng;

/// Le témoin. Sans état : un tirage par main, et rien à mémoriser.
#[derive(Debug, Default)]
pub struct RandomPolicy;

impl RandomPolicy {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Policy for RandomPolicy {
    fn name(&self) -> &'static str {
        "random"
    }

    fn decide(&mut self, view: &HandView<'_>, rng: &mut ChaCha8Rng) -> HandDecision {
        // Le même filtre que les deux autres sondes : une figure disponible
        // n'est pas seulement une figure non consommée.
        // **Une liste de figures, et non de noms de figures.** Une garde
        // d'Étape 6 proscrit tout second stockage de noms de figures hors du
        // prédicat du jeu : collecter les correspondances elles-mêmes, comme le
        // fait la sonde attentive à la grille, garde la garde entière et rend
        // au passage la figure choisie avec son aperçu.
        let disponibles: Vec<&HandMatch> = view
            .matches
            .iter()
            .filter(|figure| figure_jouable(view.blind, figure.hand))
            .collect();

        // **Jamais un dépaquetage sur le résultat d'un tirage**, même « puisque
        // la case de dump est toujours là » : une contrainte de manche de
        // l'Étape 9 peut la retirer de l'évaluation, et une panique au milieu de
        // dix mille runs ne dirait pas quelle graine l'a produite. Le repli
        // soumet le meilleur candidat formé, que la boucle comptera en anomalie.
        let Some(premier) = disponibles.first().map(|figure| figure.hand) else {
            return HandDecision::Submit(
                view.matches
                    .first()
                    .map_or(YahtzeeHand::Chance, |figure| figure.hand),
            );
        };

        let index = rng.random_range(0..disponibles.len());
        HandDecision::Submit(disponibles.get(index).map_or(premier, |figure| figure.hand))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::policy::greedy::GreedyPolicy;
    use crate::policy::{ShopAction, ShopPolicy};
    use crate::rng::SimRng;
    use crate::run::simulate_with;
    use crate::view::ShopView;
    use core_engine::blinds::{BlindContext, BlindDefinition, BlindType};
    use core_engine::config::RunConfig;
    use core_engine::cups::CupId;
    use core_engine::cups::definitions::cup;
    use core_engine::dice::{Die, DieId};
    use core_engine::evaluator::{HandEvaluator, rescore_with_levels};
    use core_engine::hands::{HandGrid, HandLevels};
    use core_engine::pool::DicePool;
    use core_engine::relics::RelicInventory;
    use core_engine::rng::RunRng;
    use rand::Rng;
    use smallvec::{SmallVec, smallvec};

    fn empreintes(rng: &RunRng) -> [Vec<u64>; 4] {
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
    }

    fn figure(hand: YahtzeeHand, apercu: u64) -> HandMatch {
        HandMatch {
            hand,
            scoring_dice: vec![DieId(0)],
            discarded_dice: Vec::new(),
            potential_score: apercu,
        }
    }

    fn manche(grille: HandGrid) -> BlindContext {
        BlindContext {
            blind: BlindDefinition {
                kind: BlindType::Small,
                target_score: 300,
                reward: 3,
                modifier: None,
            },
            target_score: 300,
            current_score: 0,
            hands_remaining: 4,
            used_hands: grille,
        }
    }

    struct Decor {
        des: Vec<Die>,
        niveaux: HandLevels,
        stock: RelicInventory,
    }

    fn decor() -> Decor {
        let config = RunConfig::from_cup(&cup(CupId::Standard));
        Decor {
            des: (0..5).map(|r| Die::new(DieId(r), 6)).collect(),
            niveaux: HandLevels::default(),
            stock: RelicInventory::new(config.relic_capacity),
        }
    }

    fn vue<'a>(d: &'a Decor, figures: &'a [HandMatch], blind: &'a BlindContext) -> HandView<'a> {
        HandView {
            dice: &d.des,
            matches: figures,
            blind,
            rerolls_left: 2,
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
            policy: crate::config::PolicyKind::Random,
            shop_policy: crate::config::ShopPolicyKind::Budget,
            threads: 1,
        }
    }

    #[test]
    fn test_random_policy_uses_sim_rng_only() {
        let d = decor();
        let figures = vec![
            figure(YahtzeeHand::Yahtzee, 500),
            figure(YahtzeeHand::Chance, 120),
        ];
        let m = manche(HandGrid::default());
        let temoin = RunRng::from_seed(4);
        let avant = empreintes(&temoin);

        let mut flux = SimRng::from_seed(4).policy;
        for _ in 0..50 {
            RandomPolicy::new().decide(&vue(&d, &figures, &m), &mut flux);
        }
        assert_eq!(empreintes(&temoin), avant, "un flux de la run a avancé");
    }

    #[test]
    fn test_random_draws_exactly_once_per_hand() {
        // **Un tirage par main, exactement**, quel que soit le nombre de
        // candidats : le témoin n'a qu'une source de variance, et c'est une
        // propriété vérifiable et non une intention.
        let d = decor();
        let m = manche(HandGrid::default());
        let deux = vec![
            figure(YahtzeeHand::Yahtzee, 500),
            figure(YahtzeeHand::Chance, 120),
        ];
        let cinq: Vec<HandMatch> = [
            YahtzeeHand::Yahtzee,
            YahtzeeHand::FullHouse,
            YahtzeeHand::Sixes,
            YahtzeeHand::Fours,
            YahtzeeHand::Chance,
        ]
        .into_iter()
        .enumerate()
        .map(|(rang, hand)| figure(hand, 500 - rang as u64 * 50))
        .collect();

        // **La mesure se fait en mots de trente-deux bits, et c'est mesuré et
        // non supposé** : un tirage dans un intervalle de cette taille avance le
        // flux d'**un seul mot**, là où une lecture de soixante-quatre bits en
        // consomme deux. Mesurer dans la mauvaise unité désaligne la comparaison
        // et fait échouer un test dont la propriété est pourtant vraie.
        let apres_une_decision = |figures: &[HandMatch]| {
            let mut flux = SimRng::from_seed(11).policy;
            RandomPolicy::new().decide(&vue(&d, figures, &m), &mut flux);
            flux.next_u32()
        };
        // Le second mot d'un flux vierge : la décision en a consommé un seul.
        let attendu = {
            let mut vierge = SimRng::from_seed(11).policy;
            vierge.next_u32();
            vierge.next_u32()
        };
        assert_eq!(apres_une_decision(&deux), attendu, "deux candidats");
        assert_eq!(apres_une_decision(&cinq), attendu, "cinq candidats");

        // Et jamais de relance.
        let mut flux = SimRng::from_seed(11).policy;
        for _ in 0..100 {
            assert!(matches!(
                RandomPolicy::new().decide(&vue(&d, &cinq, &m), &mut flux),
                HandDecision::Submit(_)
            ));
        }
    }

    #[test]
    fn test_random_is_uniform_over_available_hands() {
        // Dix mille tirages sur cinq figures : l'espérance est deux mille par
        // figure et l'écart-type quarante. La bande à ±10 % vaut **cinq
        // écarts-types** — jamais franchie par hasard, et elle attrape toute
        // vraie dérive. À ±5 % elle serait à deux écarts-types et demi, donc
        // instable une fois sur cent ; à ±20 % elle ne verrait pas un biais de
        // quinze pour cent.
        let d = decor();
        let candidats = [
            YahtzeeHand::Yahtzee,
            YahtzeeHand::FullHouse,
            YahtzeeHand::Sixes,
            YahtzeeHand::Fours,
            YahtzeeHand::Chance,
        ];
        let figures: Vec<HandMatch> = candidats
            .into_iter()
            .enumerate()
            .map(|(rang, hand)| figure(hand, 500 - rang as u64 * 50))
            .collect();

        // **Deux figures consommées en plus des cinq candidats.** Sans elles,
        // la liste des disponibles se confond avec celle des figures, et un
        // tirage fait sur la mauvaise longueur passe inaperçu : les index hors
        // bornes retombent sur le repli, qui est l'un des cinq. Mesuré au banc.
        let mut grille = HandGrid::default();
        grille.mark(YahtzeeHand::Aces);
        grille.mark(YahtzeeHand::Twos);
        let m = manche(grille);
        let mut figures = figures;
        figures.push(figure(YahtzeeHand::Aces, 40));
        figures.push(figure(YahtzeeHand::Twos, 30));

        let mut comptes = [0u32; 5];
        let mut flux = SimRng::from_seed(2).policy;
        let mut sonde = RandomPolicy::new();
        for _ in 0..10_000 {
            if let HandDecision::Submit(hand) = sonde.decide(&vue(&d, &figures, &m), &mut flux)
                && let Some(rang) = candidats.iter().position(|c| *c == hand)
            {
                comptes[rang] += 1;
            }
        }
        for (rang, compte) in comptes.iter().enumerate() {
            assert!(
                (1_800..=2_200).contains(compte),
                "{:?} sort {compte} fois sur dix mille",
                candidats[rang]
            );
        }
    }

    #[test]
    fn test_random_never_picks_a_used_hand() {
        let mut rng = RunRng::from_seed(41);
        let deck = cup(CupId::Standard);
        let config = RunConfig::from_cup(&deck);
        let niveaux = HandLevels::default();
        let stock = RelicInventory::new(config.relic_capacity);
        let mut flux = SimRng::from_seed(5).policy;
        let mut sonde = RandomPolicy::new();
        let mut un_seul = 0u32;

        for tour in 0..1_000u32 {
            let mut pool = DicePool::new(&config, &deck.sides);
            pool.roll_all(&mut rng.dice, true);
            let lances = pool.dice().to_vec();
            let mut figures = HandEvaluator::evaluate(&lances);
            rescore_with_levels(&mut figures, &lances, &niveaux);

            let consommees = figures.len().saturating_sub(1);
            un_seul += u32::from(consommees + 1 == figures.len() && figures.len() > 1);
            let mut grille = HandGrid::default();
            for f in figures.iter().take(consommees) {
                grille.mark(f.hand);
            }
            let m = manche(grille);
            let d = Decor {
                des: lances,
                niveaux: niveaux.clone(),
                stock: stock.clone(),
            };
            if let HandDecision::Submit(hand) = sonde.decide(&vue(&d, &figures, &m), &mut flux) {
                assert!(
                    figure_jouable(&m, hand),
                    "tour {tour} : {hand:?} indisponible"
                );
            }
        }
        // **Aucune panique quand il ne reste qu'un seul candidat** : c'est le
        // cas de tous les tours ci-dessus, et il est atteint mille fois.
        assert_eq!(
            un_seul, 1_000,
            "le scénario à un seul candidat n'est pas atteint"
        );
    }

    #[test]
    fn test_random_avoids_debuffed_hands() {
        use core_engine::blinds::BlindModifier;
        let d = decor();
        let figures = vec![
            figure(YahtzeeHand::Chance, 500),
            figure(YahtzeeHand::FullHouse, 300),
        ];
        let mut m = manche(HandGrid::default());
        m.blind.modifier = Some(BlindModifier::DebuffHands(smallvec![YahtzeeHand::Chance]));

        let mut flux = SimRng::from_seed(3).policy;
        let mut sonde = RandomPolicy::new();
        for _ in 0..200 {
            assert_eq!(
                sonde.decide(&vue(&d, &figures, &m), &mut flux),
                HandDecision::Submit(YahtzeeHand::FullHouse),
                "la figure interdite a été tirée"
            );
        }
    }

    #[test]
    fn test_loop_passes_the_sim_stream() {
        // **Rien d'autre ne vérifie le câblage.** Un clone d'un flux de la run
        // passé aux politiques n'avancerait aucun des quatre — le contrôle des
        // empreintes le laisserait passer — et corrélerait pourtant le témoin
        // aux dés qu'il évalue : il tirerait la séquence même qui les fabrique.
        struct Mouchard(Option<u32>);
        impl Policy for Mouchard {
            fn name(&self) -> &'static str {
                "mouchard"
            }
            fn decide(&mut self, view: &HandView<'_>, rng: &mut ChaCha8Rng) -> HandDecision {
                if self.0.is_none() {
                    self.0 = Some(rng.next_u32());
                }
                HandDecision::Submit(
                    view.matches
                        .iter()
                        .map(|f| f.hand)
                        .find(|h| figure_jouable(view.blind, *h))
                        .unwrap_or(YahtzeeHand::Chance),
                )
            }
        }

        let graine = 77;
        let mut mouchard = Mouchard(None);
        simulate_with(&campagne(), graine, &mut mouchard, &mut Passive);
        assert_eq!(
            mouchard.0,
            Some(SimRng::from_seed(graine).policy.next_u32()),
            "la boucle ne passe pas le cinquième flux"
        );
    }

    #[test]
    fn test_greedy_beats_random() {
        // **Contrôle de l'appareil, pas de la sonde.** Il répond à la seule
        // question que rien d'autre ne pose : l'instrument mesure-t-il quelque
        // chose ? Si la gloutonne ne dominait pas nettement le témoin, le
        // harnais ne distinguerait pas un joueur d'un tirage au sort, et **tous**
        // les chiffres de la campagne seraient sans valeur. Son échec invalide
        // la campagne entière, jamais le seuil. **Ne le marque pas ignoré** : il
        // coûte quatre cents millisecondes en debug, mesuré.
        //
        // **La mesure porte sur les manches, pas sur les antes.** Un ante en
        // compte trois, et tout se joue dans le premier : mesuré sur mille runs
        // par sonde, l'ante médian vaut **un des deux côtés**, écart nul. À la
        // résolution des manches, l'écart est net — médianes **2 contre 0**,
        // totaux **1702 contre 85**. Les deux seuils ci-dessous sont posés loin
        // de ces valeurs.
        let config = campagne();
        let mesure = |glouton: bool| {
            let mut manches: Vec<u8> = Vec::new();
            for graine in 1..=1_000u64 {
                let issue = if glouton {
                    simulate_with(&config, graine, &mut GreedyPolicy::new(), &mut Passive)
                } else {
                    simulate_with(&config, graine, &mut RandomPolicy::new(), &mut Passive)
                };
                manches.push(issue.blinds_cleared);
            }
            manches.sort_unstable();
            let total: u32 = manches.iter().map(|m| u32::from(*m)).sum();
            (u32::from(manches[manches.len() / 2]), total)
        };

        let (mediane_gloutonne, total_glouton) = mesure(true);
        let (mediane_temoin, total_temoin) = mesure(false);

        assert!(
            mediane_gloutonne > mediane_temoin,
            "médianes {mediane_gloutonne} contre {mediane_temoin} : l'instrument ne distingue pas un joueur d'un tirage au sort"
        );
        assert!(
            total_glouton >= total_temoin.saturating_mul(5),
            "totaux {total_glouton} contre {total_temoin} : moins de cinq fois plus de manches battues"
        );
    }
}
