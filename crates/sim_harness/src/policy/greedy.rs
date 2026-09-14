//! La sonde gloutonne : elle retient le meilleur aperçu disponible, garde ses
//! dés et relance le reste tant que cela progresse.
//!
//! **Elle ne consomme aucun flux de la run, et le type l'en empêche.** Le
//! générateur qu'elle reçoit est celui des politiques ; la vue n'en porte
//! aucun. Le danger réel n'est pas le pipeline de score — il est pur et ne
//! prend aucun générateur — mais la politique qui **simulerait un jet** pour
//! estimer un candidat : le nombre de simulations dépendant de la politique,
//! deux sondes verraient des dés différents pour la même graine, et la
//! comparaison qui est le résultat central de l'étape mesurerait du bruit.
//! Toute spéculation se fait donc sur un **clone**. Celle-ci ne spécule pas du
//! tout.
//!
//! # Deux sondes divergent sur une même graine, et c'est correct
//!
//! Verrouiller d'autres dés ne consomme pas le même nombre de tirages : deux
//! politiques voient des dés différents dès leur premier choix différent. **Ce
//! n'est pas un défaut de déterminisme.** Resemer un flux entre deux décisions,
//! forcer un nombre fixe de tirages ou cloner le générateur de run à chaque
//! main casserait la reproductibilité qu'on croirait rétablir, et aucun test
//! ne le verrait. Le déterminisme exigé est celui-ci, et lui seul : la même
//! graine, avec la même politique, rend le même résultat. La comparaison des
//! sondes est **statistique sur N graines**, jamais run à run.
//!
//! # Il n'existe et il n'existera aucune API « meilleure figure » dans le moteur
//!
//! L'évaluateur trie par aperçu décroissant, mais **ce tri ne sélectionne
//! rien** : la grille est consommable et la figure est choisie par le joueur.
//! Réintroduire une sélection automatique dans le moteur est la régression
//! exacte que l'ADR-001 corrige. La sélection est le travail de la politique,
//! et le prédicat de disponibilité vit dans la boucle, qui l'applique aussi.

use crate::policy::{HandDecision, Policy};
use crate::run::figure_jouable;
use crate::view::{HandView, LockMask};
use core_engine::evaluator::HandMatch;
use core_engine::hands::YahtzeeHand;
use rand_chacha::ChaCha8Rng;

/// La sonde gloutonne.
///
/// Elle porte l'aperçu du cycle précédent, parce que le critère d'arrêt se
/// mesure sur un **progrès** : sans lui, elle relancerait jusqu'à épuisement
/// même sur une main déjà optimale, ce qui gonfle le coût d'un run sans rien
/// changer au résultat.
#[derive(Debug, Default)]
pub struct GreedyPolicy {
    /// `(relances vues, meilleur aperçu)` au cycle précédent.
    dernier: Option<(u8, u64)>,
}

impl GreedyPolicy {
    #[must_use]
    pub fn new() -> Self {
        Self { dernier: None }
    }

    /// L'aperçu du cycle précédent **de la même main**, s'il y en a un.
    ///
    /// **Une nouvelle main se reconnaît à ce que les relances n'ont pas
    /// décru.** Dans une main, le compteur décroît strictement à chaque
    /// relance ; à l'entrée de la suivante, il remonte — ou reste égal, si la
    /// précédente s'est soumise sans relancer. Le test couvre aussi la toute
    /// première main, où il n'y a rien de mémorisé.
    fn apercu_precedent(&self, relances: u8) -> Option<u64> {
        match self.dernier {
            Some((vues, apercu)) if relances < vues => Some(apercu),
            _ => None,
        }
    }
}

impl Policy for GreedyPolicy {
    /// Le libellé exact de la variante correspondante : c'est lui qui remplira
    /// la colonne de politique du tableau de sortie, et il doit être stable
    /// d'une version à l'autre.
    fn name(&self) -> &'static str {
        "greedy"
    }

    fn decide(&mut self, view: &HandView<'_>, _rng: &mut ChaCha8Rng) -> HandDecision {
        // **Le filtrage précède la sélection, pas l'évaluation.** Les figures
        // consommées restent évaluées — la sonde attentive à la grille en a
        // besoin — et c'est la politique qui les écarte de son choix.
        //
        // Les aperçus arrivent **déjà rescorés aux niveaux de figures** : la
        // boucle évalue puis rescore avant de bâtir la vue. Rappeler
        // l'évaluateur ici rendrait des aperçus de niveau un et classerait
        // autrement que la boucle qui l'alimente, sans qu'aucune erreur ne le
        // signale.
        let Some(candidat) = meilleur_disponible(view) else {
            // **Impossible en run réel** — quatre mains pour treize cases — et
            // l'issue fait remonter le cas plutôt que de l'absorber : la boucle
            // comptera l'anomalie et abandonnera. Choisir une figure arbitraire
            // cacherait un défaut de conception derrière un run plausible.
            return HandDecision::Submit(
                view.matches
                    .first()
                    .map_or(YahtzeeHand::Chance, |figure| figure.hand),
            );
        };

        let precedent = self.apercu_precedent(view.rerolls_left);
        self.dernier = Some((view.rerolls_left, candidat.potential_score));

        // Rien à relancer : la main est complète, et brûler une relance pour
        // reposer les mêmes dés ne changerait que le compteur.
        let complete = candidat.scoring_dice.len() >= view.dice.len();
        if view.rerolls_left == 0 || complete {
            return HandDecision::Submit(candidat.hand);
        }

        // **Le progrès se mesure sur l'aperçu du meilleur candidat**, comparé
        // au cycle précédent. Égalité vaut absence de progrès, donc
        // soumission. Au premier cycle il n'y a rien à comparer : la main est
        // fraîche et il reste des dés à relancer.
        match precedent {
            Some(avant) if candidat.potential_score <= avant => HandDecision::Submit(candidat.hand),
            _ => HandDecision::Reroll(LockMask::new(candidat.scoring_dice.iter().copied())),
        }
    }
}

/// Le meilleur aperçu **disponible**, l'évaluateur rendant sa liste déjà triée
/// par aperçu décroissant.
fn meilleur_disponible<'a>(view: &'a HandView<'a>) -> Option<&'a HandMatch> {
    view.matches
        .iter()
        .find(|figure| figure_jouable(view.blind, figure.hand))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::policy::ShopPolicy;
    use crate::run::simulate_with;
    use crate::view::{ShopAction, ShopView};
    use core_engine::blinds::{BlindContext, BlindDefinition, BlindModifier, BlindType};
    use core_engine::config::RunConfig;
    use core_engine::cups::CupId;
    use core_engine::cups::definitions::cup;
    use core_engine::dice::{Die, DieId};
    use core_engine::evaluator::{HandEvaluator, HandMatch, rescore_with_levels};
    use core_engine::hands::{HandGrid, HandLevels};
    use core_engine::pool::DicePool;
    use core_engine::relics::RelicInventory;
    use core_engine::rng::RunRng;
    use rand_chacha::rand_core::{Rng, SeedableRng};
    use smallvec::{SmallVec, smallvec};

    /// Observe un flux **sans le consommer** : le clone tire, l'original non.
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

    fn des_de(valeurs: &[u8]) -> Vec<Die> {
        valeurs
            .iter()
            .enumerate()
            .map(|(rang, valeur)| {
                let mut die = Die::new(DieId(rang as u32), 6);
                die.current_value = *valeur;
                die
            })
            .collect()
    }

    fn manche(modifier: Option<BlindModifier>, grille: HandGrid) -> BlindContext {
        BlindContext {
            blind: BlindDefinition {
                kind: BlindType::Small,
                target_score: 300,
                reward: 3,
                modifier,
            },
            target_score: 300,
            current_score: 0,
            hands_remaining: 4,
            used_hands: grille,
        }
    }

    struct Decor {
        des: Vec<Die>,
        figures: Vec<HandMatch>,
        niveaux: HandLevels,
        stock: RelicInventory,
    }

    fn decor(valeurs: &[u8]) -> Decor {
        let des = des_de(valeurs);
        let niveaux = HandLevels::default();
        let mut figures = HandEvaluator::evaluate(&des);
        rescore_with_levels(&mut figures, &des, &niveaux);
        let config = RunConfig::from_cup(&cup(CupId::Standard));
        Decor {
            des,
            figures,
            niveaux,
            stock: RelicInventory::new(config.relic_capacity),
        }
    }

    fn vue<'a>(d: &'a Decor, blind: &'a BlindContext, relances: u8) -> HandView<'a> {
        HandView {
            dice: &d.des,
            matches: &d.figures,
            blind,
            rerolls_left: relances,
            hand_levels: &d.niveaux,
            relics: &d.stock,
        }
    }

    /// Politique instrumentée qui **simule un jet** avant de décider. Elle
    /// détient son propre générateur de run, parce que c'est le seul moyen de
    /// montrer ce que la discipline du clone protège : le trait, lui, ne donne
    /// accès à aucun flux de la run.
    struct Speculative {
        rng: RunRng,
        simulations: usize,
        sur_le_clone: bool,
    }

    impl Policy for Speculative {
        fn name(&self) -> &'static str {
            "speculative"
        }
        fn decide(&mut self, view: &HandView<'_>, _rng: &mut ChaCha8Rng) -> HandDecision {
            for _ in 0..self.simulations {
                let mut copie = view.dice.to_vec();
                if self.sur_le_clone {
                    let mut clone = self.rng.dice.clone();
                    for die in &mut copie {
                        die.roll(&mut clone, true);
                    }
                } else {
                    for die in &mut copie {
                        die.roll(&mut self.rng.dice, true);
                    }
                }
            }
            HandDecision::Submit(view.matches[0].hand)
        }
    }

    struct Premiere;
    impl Policy for Premiere {
        fn name(&self) -> &'static str {
            "premiere"
        }
        fn decide(&mut self, view: &HandView<'_>, _rng: &mut ChaCha8Rng) -> HandDecision {
            let figure = view
                .matches
                .iter()
                .map(|f| f.hand)
                .find(|h| figure_jouable(view.blind, *h))
                .unwrap_or(YahtzeeHand::Chance);
            HandDecision::Submit(figure)
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
            policy: crate::config::PolicyKind::Greedy,
            shop_policy: crate::config::ShopPolicyKind::Budget,
            threads: 1,
        }
    }

    #[test]
    fn test_policy_does_not_advance_run_rng() {
        let d = decor(&[5, 5, 5, 2, 2]);
        let m = manche(None, HandGrid::default());
        let mut flux = ChaCha8Rng::seed_from_u64(9);

        let mut sonde = Speculative {
            rng: RunRng::from_seed(4),
            simulations: 3,
            sur_le_clone: true,
        };
        let avant = empreintes(&sonde.rng);
        sonde.decide(&vue(&d, &m, 2), &mut flux);
        assert_eq!(empreintes(&sonde.rng), avant, "un flux de la run a avancé");

        // Et la sonde gloutonne, qui ne spécule pas du tout.
        let mut gloutonne = GreedyPolicy::new();
        let temoin = RunRng::from_seed(4);
        let avant = empreintes(&temoin);
        gloutonne.decide(&vue(&d, &m, 2), &mut flux);
        assert_eq!(empreintes(&temoin), avant);
    }

    #[test]
    fn test_speculation_uses_a_clone() {
        let d = decor(&[5, 5, 5, 2, 2]);
        let m = manche(None, HandGrid::default());
        // Le nombre de simulations n'a **aucun** effet observable.
        let empreinte_apres = |simulations: usize, sur_le_clone: bool| {
            let mut flux = ChaCha8Rng::seed_from_u64(9);
            let mut sonde = Speculative {
                rng: RunRng::from_seed(4),
                simulations,
                sur_le_clone,
            };
            sonde.decide(&vue(&d, &m, 2), &mut flux);
            empreintes(&sonde.rng)
        };
        let reference = empreintes(&RunRng::from_seed(4));
        assert_eq!(empreinte_apres(0, true), reference);
        assert_eq!(empreinte_apres(7, true), reference);

        // **Contre-épreuve** : la même sonde qui spécule sur le flux vivant le
        // fait avancer. Sans elle, le test passerait sur une sonde inerte.
        assert_ne!(empreinte_apres(7, false), reference);
    }

    #[test]
    fn test_greedy_never_picks_a_used_hand() {
        let mut rng = RunRng::from_seed(21);
        let deck = cup(CupId::Standard);
        let config = RunConfig::from_cup(&deck);
        let niveaux = HandLevels::default();
        let stock = RelicInventory::new(config.relic_capacity);
        let mut flux = ChaCha8Rng::seed_from_u64(3);
        let mut eprouves = 0usize;

        for tour in 0..1_000u32 {
            let mut pool = DicePool::new(&config, &deck.sides);
            pool.roll_all(&mut rng.dice, true);
            let des = pool.dice().to_vec();
            let mut figures = HandEvaluator::evaluate(&des);
            rescore_with_levels(&mut figures, &des, &niveaux);

            // **Les cinq meilleures figures consommées, mais jamais toutes.**
            // Le scénario du ticket demande les cinq premières ; une main qui
            // n'en forme que cinq n'aurait alors plus aucun candidat, et le
            // repli du § 2.2 — soumettre le meilleur formé pour que la boucle
            // compte l'anomalie — rendrait le test rouge sur son propre
            // scénario. On en laisse toujours une.
            let consommees = figures.len().saturating_sub(1).min(5);
            eprouves += usize::from(consommees == 5);
            let mut grille = HandGrid::default();
            for figure in figures.iter().take(consommees) {
                grille.mark(figure.hand);
            }
            let m = manche(None, grille);
            let d = Decor {
                des,
                figures,
                niveaux: niveaux.clone(),
                stock: stock.clone(),
            };

            let mut gloutonne = GreedyPolicy::new();
            if let HandDecision::Submit(figure) = gloutonne.decide(&vue(&d, &m, 0), &mut flux) {
                assert!(
                    figure_jouable(&m, figure),
                    "tour {tour} : {figure:?} n'était pas disponible"
                );
            }
        }
        // **Mesuré, pas supposé** : environ une main sur cinq forme plus de
        // cinq figures et éprouve donc le scénario complet. Le seuil dit que
        // le cas existe en nombre, sans épingler un chiffre que le premier
        // changement d'évaluateur rendrait faux.
        assert!(
            eprouves > 100,
            "seules {eprouves} mains sur mille portaient cinq figures consommées"
        );
    }

    #[test]
    fn test_greedy_avoids_debuffed_hands() {
        let d = decor(&[6, 6, 6, 6, 6]);
        let interdite = d.figures[0].hand;
        let m = manche(
            Some(BlindModifier::DebuffHands(smallvec![interdite])),
            HandGrid::default(),
        );

        let mut flux = ChaCha8Rng::seed_from_u64(3);
        let mut gloutonne = GreedyPolicy::new();
        match gloutonne.decide(&vue(&d, &m, 0), &mut flux) {
            HandDecision::Submit(figure) => {
                assert_ne!(figure, interdite, "la figure interdite a été retenue");
                assert!(figure_jouable(&m, figure));
            }
            HandDecision::Reroll(_) => panic!("aucune relance restante"),
        }
    }

    #[test]
    fn test_greedy_locks_the_scoring_dice() {
        let d = decor(&[5, 5, 5, 5, 2]);
        let m = manche(None, HandGrid::default());
        let attendu = d
            .figures
            .iter()
            .find(|f| figure_jouable(&m, f.hand))
            .expect("un candidat");
        assert!(
            attendu.scoring_dice.len() < d.des.len(),
            "le scénario exige un candidat partiel"
        );

        let mut flux = ChaCha8Rng::seed_from_u64(3);
        let mut gloutonne = GreedyPolicy::new();
        match gloutonne.decide(&vue(&d, &m, 2), &mut flux) {
            HandDecision::Reroll(masque) => {
                assert_eq!(masque.iter().collect::<Vec<_>>(), attendu.scoring_dice);
            }
            HandDecision::Submit(_) => panic!("une main partielle se relance"),
        }
    }

    #[test]
    fn test_greedy_submits_a_complete_hand_without_burning_a_reroll() {
        // Cinq dés identiques : le meilleur candidat les retient tous, et il
        // n'y a rien à relancer. Relancer brûlerait une relance pour rien.
        let d = decor(&[6, 6, 6, 6, 6]);
        let m = manche(None, HandGrid::default());
        let meilleur = d
            .figures
            .iter()
            .find(|f| figure_jouable(&m, f.hand))
            .expect("un candidat");
        assert_eq!(meilleur.scoring_dice.len(), d.des.len(), "scénario");

        let mut flux = ChaCha8Rng::seed_from_u64(3);
        let mut gloutonne = GreedyPolicy::new();
        assert_eq!(
            gloutonne.decide(&vue(&d, &m, 2), &mut flux),
            HandDecision::Submit(meilleur.hand)
        );
    }

    #[test]
    fn test_greedy_stops_rerolling_when_no_progress() {
        let d = decor(&[5, 5, 5, 5, 2]);
        let m = manche(None, HandGrid::default());
        let mut flux = ChaCha8Rng::seed_from_u64(3);
        let mut gloutonne = GreedyPolicy::new();

        // Premier cycle : il reste un dé à relancer.
        assert!(matches!(
            gloutonne.decide(&vue(&d, &m, 2), &mut flux),
            HandDecision::Reroll(_)
        ));
        // Second cycle, même main : l'aperçu n'a pas progressé.
        assert!(
            matches!(
                gloutonne.decide(&vue(&d, &m, 1), &mut flux),
                HandDecision::Submit(_)
            ),
            "la sonde relance sans progrès"
        );
    }

    #[test]
    fn test_greedy_forgets_the_previous_hand() {
        // **Sans remise à zéro, le premier cycle d'une main se compare au
        // dernier cycle de la précédente** : une main faible qui suit une main
        // forte se soumettrait aussitôt, sans relancer une seule fois. Mesuré
        // au banc, le mutant survivait à toute la suite.
        let forte = decor(&[6, 6, 6, 6, 2]);
        let faible = decor(&[1, 1, 2, 3, 4]);
        let m = manche(None, HandGrid::default());
        let mut flux = ChaCha8Rng::seed_from_u64(3);
        let mut gloutonne = GreedyPolicy::new();

        let apercu = |d: &Decor| {
            d.figures
                .iter()
                .find(|f| figure_jouable(&m, f.hand))
                .map_or(0, |f| f.potential_score)
        };
        assert!(
            apercu(&forte) > apercu(&faible),
            "scénario : la seconde est faible"
        );

        // Première main : elle relance, puis se soumet faute de progrès.
        assert!(matches!(
            gloutonne.decide(&vue(&forte, &m, 2), &mut flux),
            HandDecision::Reroll(_)
        ));
        assert!(matches!(
            gloutonne.decide(&vue(&forte, &m, 1), &mut flux),
            HandDecision::Submit(_)
        ));

        // **Seconde main, relances de nouveau au maximum** : la sonde doit
        // avoir oublié l'aperçu de la précédente.
        assert!(
            matches!(
                gloutonne.decide(&vue(&faible, &m, 2), &mut flux),
                HandDecision::Reroll(_)
            ),
            "la sonde se souvient de la main précédente"
        );
    }

    #[test]
    fn test_greedy_is_deterministic() {
        let d = decor(&[5, 5, 5, 5, 2]);
        let m = manche(None, HandGrid::default());
        let mut reference = None;
        for _ in 0..100 {
            let mut flux = ChaCha8Rng::seed_from_u64(3);
            let mut gloutonne = GreedyPolicy::new();
            let decision = gloutonne.decide(&vue(&d, &m, 2), &mut flux);
            match &reference {
                None => reference = Some(decision),
                Some(attendu) => assert_eq!(&decision, attendu, "masque ou figure instable"),
            }
        }
    }

    #[test]
    fn test_two_policies_diverge_on_one_seed() {
        // **C'est l'attendu, pas un défaut.** Verrouiller d'autres dés ne
        // consomme pas le même nombre de tirages : deux politiques voient des
        // dés différents dès la main suivante. La comparaison des sondes est
        // statistique sur N graines, jamais run à run.
        let config = campagne();
        let (gloutonne, _) = simulate_with(&config, 5, &mut GreedyPolicy::new(), &mut Passive);
        let (premiere, _) = simulate_with(&config, 5, &mut Premiere, &mut Passive);
        assert_ne!(gloutonne, premiere, "les deux sondes n'ont pas divergé");

        // Et chacune est reproductible à graine égale.
        assert_eq!(
            gloutonne,
            simulate_with(&config, 5, &mut GreedyPolicy::new(), &mut Passive).0
        );
    }

    #[test]
    fn test_run_rng_still_has_four_streams() {
        // **Le compilateur tient l'invariant, pas un compte.** Un cinquième
        // flux ajouté au générateur de run donne `E0027` en nommant le champ
        // oublié — et ce serait une modification du moteur, donc la règle n°1.
        let RunRng {
            dice: _,
            shop: _,
            boss: _,
            relic_effects: _,
        } = RunRng::from_seed(1);
    }
}
