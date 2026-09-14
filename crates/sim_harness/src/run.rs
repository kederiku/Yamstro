//! La boucle d'un run.
//!
//! **Cette orchestration double celle de la crate d'états, et c'est assumé.**
//! Le document d'étape en fait la raison d'être du harnais : la boucle de
//! manche vit dans une crate qui dépend du moteur graphique, que le harnais ne
//! peut pas appeler. Le risque de divergence est couvert par le fixture
//! d'accord de TASK-155, qui rejoue une graine de référence et compare les
//! scores commis. **N'importe pas la crate d'états pour « supprimer le
//! doublon »** : elle tire le moteur graphique dans l'arbre normal, et le
//! harnais perd la seule propriété qui rend dix mille runs faisables en une
//! minute.

use crate::blind::{blind_context, blind_definition, draw_boss_for};
use crate::config::SimConfig;
use crate::policy::{HandDecision, Policy, ShopAction, ShopPolicy};
use crate::rng::SimRng;
use crate::state::{SimHand, SimSession};
use crate::view::{HandView, ShopView};
use core_engine::blinds::{BlindContext, BlindDefinition, BlindModifier, BlindType};
use core_engine::config::{RunConfig, effective_rerolls};
use core_engine::cups::CupId;
use core_engine::cups::definitions::cup;
use core_engine::dice::{Die, DieId};
use core_engine::economy::payout::calculate_payout;
use core_engine::economy::round_end_gold;
use core_engine::evaluator::{HandEvaluator, HandMatch, rescore_with_levels};
use core_engine::hands::{HandLevels, YahtzeeHand};
use core_engine::pool::DicePool;
use core_engine::relics::RelicInventory;
use core_engine::relics::effects::{advance_state, roll_modifier_for};
use core_engine::scoring::{Hook, ScoringPipeline, TriggerCtx};
use core_engine::shop::generator::generate_shop;
use core_engine::shop::pricing::{bump_reroll_cost, price_of, sell_value};
use core_engine::shop::{ShopInventory, ShopItem};
use rand_chacha::ChaCha8Rng;

const ANTE_FINAL: u8 = 8;
const RANGS: [BlindType; 3] = [BlindType::Small, BlindType::Big, BlindType::Boss];

/// Trous connus du contexte hors pipeline, nommés plutôt qu'écrits en zéros
/// nus : aucun bras de l'or ni de l'avancement ne les lit.
const BASE_CHIPS_HORS_PIPELINE: u64 = 0;
const BASE_MULT_HORS_PIPELINE: i64 = 0;
/// À zéro, la garde du *Dé Fantôme* serait vraie hors de tout lancer.
const ROLL_INDEX_HORS_LANCER: u8 = u8::MAX;

/// Ce qu'un run produit.
///
/// **Forme minimale : ce que la boucle produit, et rien de plus.** Les colonnes
/// du tableau de sortie, la sérialisation et le nom des politiques sont arrêtés
/// par TASK-150, qui **complète cette structure et la déplace** vers son propre
/// module — une migration, jamais un second type.
///
/// La victoire ne s'y stocke pas : elle se lit de `blinds_cleared`, et un champ
/// redondant divergerait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub seed: u64,
    pub cup: CupId,
    pub stake_level: u8,
    pub ante_reached: u8,
    pub blinds_cleared: u8,
    pub hands_played: u32,
    pub final_gold: u32,
    /// Défauts de politique rencontrés. **Il doit valoir zéro.**
    pub anomalies: u32,
    /// Un run abandonné **n'entre pas** dans les agrégats de taux de victoire.
    pub abandoned: bool,
}

impl RunOutcome {
    /// Nombre de manches d'une run complète : huit antes, trois rangs.
    #[must_use]
    pub fn victorieux(&self) -> bool {
        !self.abandoned && self.blinds_cleared == ANTE_FINAL * RANGS.len() as u8
    }
}

/// Ce que l'arbitre de fin de manche décide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IssueManche {
    Battue,
    RunPerdue,
    MainSuivante,
}

/// La dernière main commise, que l'encaissement de fin de manche relit.
struct DerniereMain {
    figure: HandMatch,
    des: Vec<Die>,
    rerolls_left: u8,
}

/// Bouchon de stake. **Les Stakes sont l'Étape 10** : ce bouchon rend `1` au
/// quatrième niveau et `0` partout ailleurs, et il se remplacera par la table
/// sans que la chaîne bouge.
// Étape 10
fn stake_reroll_malus(stake_level: u8) -> u8 {
    if stake_level == 4 { 1 } else { 0 }
}

/// Traduit le malus de stake, toujours positif, en delta signé.
fn as_negative_stake_delta(malus: u8) -> i8 {
    i8::try_from(malus).map_or(i8::MIN, i8::wrapping_neg)
}

/// Le plafond de relances de la manche, s'il y en a un.
fn plafond_de_manche(blind: &BlindDefinition) -> Option<u8> {
    if let Some(BlindModifier::MaxRerolls(cap)) = blind.modifier {
        Some(cap)
    } else {
        None
    }
}

/// Somme des deltas de relance des reliques productives.
///
/// **Signé de bout en bout.** Un delta positif doit pouvoir dépasser le plafond
/// de manche : l'écrire en soustraction perdrait la moitié du domaine sans
/// qu'aucun cas nominal ne le voie. L'agrégation passe par un entier plus large
/// puis sature.
///
/// **Le prédicat de neutralisation est celui du moteur**, et c'est le seul du
/// harnais : une seconde condition ici rendrait une relique neutralisée
/// porteuse de son malus de relance sans ses effets de score.
fn relic_reroll_malus(inventory: &RelicInventory, blind: &BlindDefinition) -> i8 {
    let somme: i16 = inventory
        .iter_slots()
        .filter(|(slot, inst)| inst.participe(*slot, blind))
        .map(|(_, inst)| {
            i16::from(roll_modifier_for(inst.def, &[], ROLL_INDEX_HORS_LANCER).reroll_delta)
        })
        .sum();
    i8::try_from(somme.clamp(i16::from(i8::MIN), i16::from(i8::MAX))).unwrap_or(0)
}

/// L'enveloppe de la chaîne de relances. **L'arithmétique appartient au
/// moteur** : l'enveloppe ne fait que lui fournir les deux deltas de sa
/// signature, de sorte qu'il n'existe toujours qu'une seule chaîne dans le
/// projet.
fn relances(
    config: &RunConfig,
    stake_level: u8,
    blind: &BlindDefinition,
    relics: &RelicInventory,
) -> u8 {
    effective_rerolls(
        config,                                                   // 1. base(gobelet)
        as_negative_stake_delta(stake_reroll_malus(stake_level)), // 2. stake
        plafond_de_manche(blind),                                 // 3. plafond de manche
        relic_reroll_malus(relics, blind),                        // 4. reliques, signé
    )
}

/// Les dés de la manche, **tous**, triés par identifiant.
///
/// Le pipeline résout chaque identifiant marquant dans le slice qu'il reçoit :
/// le filtrer sur les seuls dés participants casse la résolution. Et sans tri,
/// deux résolutions identiques peuvent rendre deux files différentes.
fn des_tries(pool: &DicePool) -> Vec<Die> {
    // Le tri est **défensif, et aucun test ne peut le distinguer** : le
    // compteur d'identifiants de la main est monotone et l'ajout se fait en
    // queue, donc la main est déjà croissante. Il rend la garantie **locale**
    // plutôt que dépendante d'une propriété d'un autre module — et le mutant
    // qui le retire est équivalent aujourd'hui, ce qui est consigné plutôt que
    // corrigé par un test qui ne mesurerait rien.
    let mut des = pool.dice().to_vec();
    des.sort_by_key(|die| die.id);
    des
}

/// Évalue puis **rescore**, dans cet ordre.
///
/// L'évaluateur chiffre tous les aperçus **au niveau un**. Sans le rescorage,
/// une politique classerait des aperçus de niveau un face à une cible réelle,
/// d'un biais croissant avec les niveaux — donc invisible au premier ante et
/// maximal au dernier, là où la campagne se décide.
fn evaluer(dice: &[Die], levels: &HandLevels) -> Vec<HandMatch> {
    let mut figures = HandEvaluator::evaluate(dice);
    rescore_with_levels(&mut figures, dice, levels);
    figures
}

/// Les figures que *L'Oubli* interdit, vide sous toute autre manche.
fn figures_interdites(blind: &BlindContext) -> &[YahtzeeHand] {
    match &blind.blind.modifier {
        Some(BlindModifier::DebuffHands(figures)) => figures,
        _ => &[],
    }
}

/// Une figure est-elle jouable ?
///
/// **Le nom diffère de celui du jeu, délibérément**, comme les noms de types de
/// l'état de run : trois gardes d'Étape 6 tiennent à ce qu'il n'existe qu'un
/// seul prédicat de figure, une seule liste de figures interdites et un seul
/// consommateur du plafond — **dans le jeu**. Le harnais les réimplémente,
/// nommément autorisé par le raccord C, et les nommer autrement garde les trois
/// gardes entières plutôt que d'y percer trois exclusions.
///
/// **Le prédicat unique du harnais, et il ne se réduit pas à la grille.** Une
/// figure interdite par la manche n'est pas neutralisée au score — le pipeline
/// ne connaît pas cette contrainte : elle est tenue par le **refus de la
/// soumission**, côté jeu comme ici. La boucle refuse, la politique évite, et
/// les deux lisent la même condition : deux conditions parallèles feraient
/// qu'une politique évite ce que la boucle accepte, ou l'inverse.
pub(crate) fn figure_jouable(blind: &BlindContext, hand: YahtzeeHand) -> bool {
    !blind.used_hands.contains(hand) && !figures_interdites(blind).contains(&hand)
}

/// L'arbitre de fin de manche. **Il ne recalcule rien et il est idempotent.**
///
/// L'ordre des trois branches est normatif : une cible atteinte à la dernière
/// main mène en boutique, pas en défaite. Tester les mains d'abord
/// transformerait chaque manche gagnée de justesse en défaite, et le taux de
/// victoire mesuré serait faux, à la baisse, de façon parfaitement crédible.
fn issue_de_manche(ctx: &BlindContext) -> IssueManche {
    if ctx.current_score >= ctx.target_score {
        return IssueManche::Battue;
    }
    if ctx.hands_remaining == 0 {
        return IssueManche::RunPerdue;
    }
    IssueManche::MainSuivante
}

/// Le contexte que l'or de fin de manche et l'avancement des états lisent.
fn contexte_de_base<'a>(
    hand_levels: &'a HandLevels,
    blind: &'a BlindContext,
    hand: &'a HandMatch,
    dice: &'a [Die],
    rerolls_left: u8,
) -> TriggerCtx<'a> {
    TriggerCtx {
        hand,
        dice,
        hand_levels,
        blind,
        uid: 0,
        slot: 0,
        state: core_engine::relics::RelicState::None,
        die: None,
        base_chips: BASE_CHIPS_HORS_PIPELINE,
        base_mult: BASE_MULT_HORS_PIPELINE,
        left_effects: &[],
        roll_index: ROLL_INDEX_HORS_LANCER,
        rerolls_left,
    }
}

/// Fait avancer l'état de chaque relique productive.
///
/// **Le document d'étape ne dit nulle part où le harnais l'appelle**, et il ne
/// l'appelait donc nulle part : toute relique à mémoire serait restée inerte,
/// et la table de contribution par relique l'aurait rapportée à zéro sans que
/// rien ne désigne la cause. Le jeu avance à deux moments, et le harnais aux
/// mêmes : après chaque figure commise, et à la fin d'une manche battue —
/// **après** l'encaissement, l'ordre inverse rendant zéro en silence.
///
/// Un slot neutralisé n'avance pas : c'est ce qui garde un compteur intact
/// après une manche passée en cage.
fn avancer(inventory: &mut RelicInventory, base: &TriggerCtx<'_>, hook: Hook) {
    for (index, slot) in inventory.slots.iter_mut().enumerate() {
        let Some(inst) = slot else { continue };
        let rang = u8::try_from(index).unwrap_or(u8::MAX);
        if !inst.participe(rang, &base.blind.blind) {
            continue;
        }
        let ctx = TriggerCtx {
            uid: inst.uid,
            slot: rang,
            state: inst.state,
            ..*base
        };
        inst.state = advance_state(inst.def, hook, &ctx, inst.state);
    }
}

/// Applique une liste d'actions d'achat. **La politique décide, la boucle
/// applique** : le solde vit dans la session, et rien d'autre n'y écrit.
///
/// Rend le nombre d'anomalies rencontrées.
fn appliquer_achats(
    session: &mut SimSession,
    inventory: &mut ShopInventory,
    actions: &[ShopAction],
) -> u32 {
    let mut anomalies: u32 = 0;
    for action in actions {
        match *action {
            // Termine la visite : le reste de la liste est **ignoré**.
            ShopAction::Leave => break,
            ShopAction::Buy(rang) => {
                // Accès **vérifié** : les achats précédents décalent la liste.
                let Some(item) = inventory.items.get(rang).cloned() else {
                    anomalies = anomalies.saturating_add(1);
                    continue;
                };
                // Ni modificateur de dé ni consommable n'a d'effet à la fin de
                // l'Étape 6 : les acheter dépenserait de l'or contre rien.
                // **Refus compté**, plutôt qu'un débit silencieux.
                if matches!(item, ShopItem::DieMod(_) | ShopItem::Consumable(_)) {
                    anomalies = anomalies.saturating_add(1);
                    continue;
                }
                let prix = price_of(&item);
                if session.gold < prix {
                    continue;
                }
                // La capacité vient de la configuration, jamais d'un littéral :
                // le Gobelet de Fortune en donne six.
                if matches!(item, ShopItem::RelicCard(_))
                    && session.relics.len() >= usize::from(session.config.relic_capacity)
                {
                    continue;
                }
                session.gold = session.gold.saturating_sub(prix);
                inventory.items.remove(rang);
                match item {
                    ShopItem::RelicCard(def) => {
                        session.relics.add_relic(def);
                    }
                    // `upgrade` est le seul mutateur des niveaux, et il sature.
                    ShopItem::GridUpgrade(hand) => session.hand_levels.upgrade(hand),
                    ShopItem::DieMod(_) | ShopItem::Consumable(_) => {}
                }
            }
            ShopAction::Sell(slot) => {
                if let Some(inst) = session.relics.remove_relic(slot) {
                    let prix = price_of(&ShopItem::RelicCard(inst.def));
                    session.gold = session.gold.saturating_add(sell_value(prix));
                }
            }
            ShopAction::RerollShop => {
                if session.gold < inventory.reroll_cost {
                    continue;
                }
                session.gold = session.gold.saturating_sub(inventory.reroll_cost);
                // **On ne réaffecte jamais l'inventaire entier** : le générateur
                // repose le coût initial à chaque appel, et les relances
                // seraient gratuites à perpétuité.
                inventory.items = generate_shop(&mut session.rng.shop).items;
                inventory.reroll_cost = bump_reroll_cost(inventory.reroll_cost);
            }
        }
    }
    anomalies
}

/// Encaisse une manche battue : l'or des reliques, puis le gain, puis
/// l'avancement des états.
///
/// **L'ordre est normatif et il ne se lit pas dans le résultat.** Une relique à
/// compteur est remise à zéro par l'avancement de fin de manche : avancer avant
/// d'encaisser rend **zéro**, en silence, et la contribution de toute relique à
/// mémoire disparaît du rapport sans qu'aucune erreur ne soit levée.
fn encaisser_fin_de_manche(
    session: &mut SimSession,
    manche: &BlindContext,
    derniere: &DerniereMain,
) {
    let base = contexte_de_base(
        &session.hand_levels,
        manche,
        &derniere.figure,
        &derniere.des,
        derniere.rerolls_left,
    );
    let or_des_reliques = round_end_gold(&session.relics, &base);
    avancer(&mut session.relics, &base, Hook::OnRoundEnd);
    let gain = calculate_payout(
        &manche.blind,
        manche.hands_remaining,
        session.gold,
        &session.config,
        or_des_reliques,
    );
    session.gold = session.gold.saturating_add(gain.total);
}

/// Joue une main jusqu'à la figure commise.
///
/// Rend `Err` sur un **défaut de politique** : figure déjà consommée, figure
/// injouable, relance demandée sans relance disponible. Corriger en silence
/// transformerait un bug de politique en biais de mesure invisible — le run
/// continuerait, la ligne de sortie paraîtrait normale, et la comparaison des
/// politiques mesurerait la qualité du rattrapage.
fn jouer_une_main<P: Policy>(
    session: &mut SimSession,
    manche: &mut BlindContext,
    sides: &[u8],
    policy: &mut P,
    policy_rng: &mut ChaCha8Rng,
    resultat: &mut RunOutcome,
) -> Result<DerniereMain, ()> {
    let mut pool = DicePool::new(&session.config, sides);
    let mut main = SimHand::new(relances(
        &session.config,
        session.stake_level,
        &manche.blind,
        &session.relics,
    ));
    pool.roll_all(&mut session.rng.dice, true);

    loop {
        let des = des_tries(&pool);
        let figures = evaluer(&des, &session.hand_levels);
        // La vue ne vit que le temps de l'appel : c'est le bloc qui le dit,
        // et c'est ce qui rend la manche de nouveau mutable en dessous.
        let decision = {
            let vue = HandView {
                dice: &des,
                matches: &figures,
                blind: manche,
                rerolls_left: main.rerolls_left,
                hand_levels: &session.hand_levels,
                relics: &session.relics,
            };
            policy.decide(&vue, policy_rng)
        };

        match decision {
            HandDecision::Submit(figure) => {
                if !figure_jouable(manche, figure) {
                    resultat.anomalies = resultat.anomalies.saturating_add(1);
                    return Err(());
                }
                let Some(choisie) = figures.iter().find(|f| f.hand == figure) else {
                    resultat.anomalies = resultat.anomalies.saturating_add(1);
                    return Err(());
                };
                let rapport = ScoringPipeline::resolve(
                    choisie,
                    &des,
                    &session.hand_levels,
                    &session.relics,
                    manche,
                );
                // **Le commit, à un seul endroit.** Le harnais ne recalcule
                // jamais un score, il le transporte.
                manche.current_score = manche.current_score.saturating_add(rapport.final_score);
                manche.hands_remaining = manche.hands_remaining.saturating_sub(1);
                manche.used_hands.mark(figure);
                resultat.hands_played = resultat.hands_played.saturating_add(1);

                let base = contexte_de_base(
                    &session.hand_levels,
                    manche,
                    choisie,
                    &des,
                    main.rerolls_left,
                );
                avancer(&mut session.relics, &base, Hook::OnHandScored);

                return Ok(DerniereMain {
                    figure: choisie.clone(),
                    des,
                    rerolls_left: main.rerolls_left,
                });
            }
            HandDecision::Reroll(masque) => {
                if main.rerolls_left == 0 {
                    resultat.anomalies = resultat.anomalies.saturating_add(1);
                    return Err(());
                }
                let identifiants: Vec<DieId> = pool.dice().iter().map(|die| die.id).collect();
                for id in identifiants {
                    let verrouille = pool
                        .dice()
                        .iter()
                        .find(|die| die.id == id)
                        .is_some_and(|die| die.locked);
                    if verrouille != masque.contains(id) {
                        pool.toggle_lock(id);
                    }
                }
                pool.roll_all(&mut session.rng.dice, false);
                main.rerolls_left = main.rerolls_left.saturating_sub(1);
            }
        }
    }
}

/// Joue un run complet.
///
/// **La forme à deux arguments et son aiguillage ne sont pas ici.** Les cinq
/// politiques n'existent pas encore : un aiguillage écrit maintenant n'aurait
/// que deux issues, un bras fourre-tout qui masque les variantes non câblées,
/// ou des bras qui rendent provisoirement une autre politique. Les deux
/// donnent un instrument qui rend un résultat **sous un mauvais nom**, sans le
/// moindre signal à l'exécution. TASK-151 la livre, quand les cinq existent.
pub fn simulate_with<P: Policy, S: ShopPolicy>(
    config: &SimConfig,
    seed: u64,
    policy: &mut P,
    shop_policy: &mut S,
) -> RunOutcome {
    // La ligne de commande remplit toujours les deux vecteurs ; une campagne
    // vide n'est pas un run jouable, et elle se signale plutôt qu'elle ne se
    // devine.
    let (Some(cup_id), Some(stake_level)) =
        (config.cups.first().copied(), config.stakes.first().copied())
    else {
        return RunOutcome {
            seed,
            cup: CupId::Standard,
            stake_level: 0,
            ante_reached: 0,
            blinds_cleared: 0,
            hands_played: 0,
            final_gold: 0,
            anomalies: 1,
            abandoned: true,
        };
    };

    let mut session = SimSession::new(cup_id, stake_level, seed);
    let deck = cup(cup_id);
    // **Le cinquième flux, celui des politiques.** Il vient du harnais et non
    // du moteur : une politique qui consommerait un flux de la run ferait
    // diverger la partie selon la stratégie employée, et deux campagnes de
    // même graine ne seraient plus comparables.
    let mut policy_rng = SimRng::from_seed(seed).policy;
    let mut resultat = RunOutcome {
        seed,
        cup: cup_id,
        stake_level,
        ante_reached: 1,
        blinds_cleared: 0,
        hands_played: 0,
        final_gold: session.gold,
        anomalies: 0,
        abandoned: false,
    };

    'run: for ante in 1..=ANTE_FINAL {
        session.ante = ante;
        resultat.ante_reached = ante;

        for kind in RANGS {
            let boss = draw_boss_for(kind, session.config.relic_capacity, &mut session.rng.boss);
            let definition = blind_definition(ante, kind, cup_id, stake_level, boss);
            // La grille naît vide avec la manche, et n'est jamais vidée entre
            // deux mains : sans elle, chaque main redevient indépendante et la
            // décision que l'étape mesure disparaît.
            let mut manche = blind_context(definition, &session.config);

            // L'arbitrage suit **chaque** main, jamais l'inverse : la manche
            // rend sa dernière main avec son issue, sans passer par un état
            // optionnel qu'il faudrait relire.
            let (battue, derniere) = loop {
                let main = match jouer_une_main(
                    &mut session,
                    &mut manche,
                    &deck.sides,
                    policy,
                    &mut policy_rng,
                    &mut resultat,
                ) {
                    Ok(main) => main,
                    Err(()) => {
                        resultat.abandoned = true;
                        break 'run;
                    }
                };
                match issue_de_manche(&manche) {
                    IssueManche::Battue => break (true, main),
                    IssueManche::RunPerdue => break (false, main),
                    IssueManche::MainSuivante => {}
                }
            };

            if !battue {
                break 'run;
            }
            resultat.blinds_cleared = resultat.blinds_cleared.saturating_add(1);
            encaisser_fin_de_manche(&mut session, &manche, &derniere);

            let mut etalage = generate_shop(&mut session.rng.shop);
            let actions = {
                let vue = ShopView {
                    inventory: &etalage,
                    gold: session.gold,
                    relics: &session.relics,
                    config: &session.config,
                };
                shop_policy.decide(&vue, &mut policy_rng)
            };
            resultat.anomalies = resultat.anomalies.saturating_add(appliquer_achats(
                &mut session,
                &mut etalage,
                &actions,
            ));
        }
    }

    resultat.final_gold = session.gold;
    resultat
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::LockMask;
    use core_engine::dice::DieId;
    use core_engine::hands::HandGrid;
    use core_engine::hands::YahtzeeHand;
    use core_engine::relics::{RelicId, RelicState};
    use core_engine::rng::RunRng;
    use rand_chacha::rand_core::SeedableRng;
    use smallvec::{SmallVec, smallvec};
    use std::collections::VecDeque;

    /// Ce que la politique a vu au moment de décider. **Elle est l'observateur
    /// du test** : la boucle ne rend qu'un résultat, et l'orchestration ne se
    /// vérifie que de l'intérieur.
    #[derive(Debug, Clone)]
    struct Instantane {
        cible: u64,
        score: u64,
        mains: u8,
        grille: HandGrid,
        relances: u8,
        des: Vec<Die>,
        figures: Vec<HandMatch>,
    }

    /// Politique à décisions figées. Sa file épuisée, elle **replie sur la
    /// première figure disponible** — jouable et non consommée — ce qui termine
    /// tout run en un nombre borné de mains.
    struct Scriptee {
        decisions: VecDeque<HandDecision>,
        vues: Vec<Instantane>,
    }

    impl Scriptee {
        fn neuve() -> Self {
            Self {
                decisions: VecDeque::new(),
                vues: Vec::new(),
            }
        }
        fn avec(decisions: Vec<HandDecision>) -> Self {
            Self {
                decisions: decisions.into(),
                vues: Vec::new(),
            }
        }
    }

    impl Policy for Scriptee {
        fn name(&self) -> &'static str {
            "scriptee"
        }
        fn decide(&mut self, view: &HandView<'_>, _rng: &mut ChaCha8Rng) -> HandDecision {
            self.vues.push(Instantane {
                cible: view.blind.target_score,
                score: view.blind.current_score,
                mains: view.blind.hands_remaining,
                grille: view.blind.used_hands,
                relances: view.rerolls_left,
                des: view.dice.to_vec(),
                figures: view.matches.to_vec(),
            });
            if let Some(decision) = self.decisions.pop_front() {
                return decision;
            }
            // **Le même prédicat que la boucle**, grille et figures interdites
            // comprises : un repli qui ne lirait que la grille soumettrait une
            // figure que *L'Oubli* interdit, et le run serait abandonné. Mesuré
            // sur mille graines avant que cette ligne n'existe.
            let repli = view
                .matches
                .iter()
                .map(|figure| figure.hand)
                .find(|hand| figure_jouable(view.blind, *hand));
            HandDecision::Submit(
                repli.unwrap_or_else(|| {
                    view.matches.first().map_or(YahtzeeHand::Chance, |f| f.hand)
                }),
            )
        }
    }

    struct BoutiqueScriptee {
        listes: VecDeque<SmallVec<[ShopAction; 4]>>,
        ors: Vec<u32>,
    }

    impl BoutiqueScriptee {
        fn passive() -> Self {
            Self {
                listes: VecDeque::new(),
                ors: Vec::new(),
            }
        }
    }

    impl ShopPolicy for BoutiqueScriptee {
        fn name(&self) -> &'static str {
            "boutique-scriptee"
        }
        fn decide(
            &mut self,
            view: &ShopView<'_>,
            _rng: &mut ChaCha8Rng,
        ) -> SmallVec<[ShopAction; 4]> {
            self.ors.push(view.gold);
            self.listes
                .pop_front()
                .unwrap_or_else(|| smallvec![ShopAction::Leave])
        }
    }

    fn campagne(cup: CupId, stake: u8) -> SimConfig {
        SimConfig {
            runs: 1,
            seed_base: 1,
            cups: vec![cup],
            stakes: vec![stake],
            policy: crate::config::PolicyKind::GridAware,
            shop_policy: crate::config::ShopPolicyKind::Budget,
            threads: 1,
        }
    }

    fn issue_vide() -> RunOutcome {
        RunOutcome {
            seed: 1,
            cup: CupId::Standard,
            stake_level: 1,
            ante_reached: 1,
            blinds_cleared: 0,
            hands_played: 0,
            final_gold: 0,
            anomalies: 0,
            abandoned: false,
        }
    }

    fn manche_avec(modifier: Option<BlindModifier>) -> BlindDefinition {
        BlindDefinition {
            kind: BlindType::Boss,
            target_score: 600,
            reward: 5,
            modifier,
        }
    }

    #[test]
    fn test_used_hand_is_never_submitted_twice() {
        let config = campagne(CupId::Standard, 1);
        for index in 0..1_000u64 {
            let mut politique = Scriptee::neuve();
            let mut boutique = BoutiqueScriptee::passive();
            let issue = simulate_with(&config, 1 + index, &mut politique, &mut boutique);

            assert_eq!(issue.anomalies, 0, "graine {index}");
            assert!(!issue.abandoned, "graine {index}");

            // Dans une manche, la grille ne perd jamais une figure et n'en
            // gagne jamais deux d'un coup.
            let mut precedente: Option<&Instantane> = None;
            for vue in &politique.vues {
                if let Some(avant) = precedente
                    && vue.mains < avant.mains
                {
                    for hand in YahtzeeHand::ALL {
                        if avant.grille.contains(hand) {
                            assert!(vue.grille.contains(hand), "{hand:?} démarquée");
                        }
                    }
                }
                precedente = Some(vue);
            }
        }
    }

    #[test]
    fn test_grid_resets_between_blinds() {
        let config = campagne(CupId::Standard, 1);
        let mut politique = Scriptee::neuve();
        let mut boutique = BoutiqueScriptee::passive();
        let issue = simulate_with(&config, 7, &mut politique, &mut boutique);
        let mains_par_manche = cup(CupId::Standard).hands_per_blind;

        assert!(!politique.vues.is_empty(), "la boucle n'a rien joué");
        assert!(issue.hands_played > 0);

        for vue in &politique.vues {
            if vue.mains == mains_par_manche {
                // Aucune main commise dans cette manche : la grille est vide.
                assert!(vue.grille.is_empty(), "grille non remise à zéro à l'entrée");
            } else {
                // Une main au moins a été commise : la grille la porte encore.
                assert!(!vue.grille.is_empty(), "grille vidée entre deux mains");
            }
        }
    }

    #[test]
    fn test_score_committed_once_per_hand() {
        let config = campagne(CupId::Standard, 1);
        let mut politique = Scriptee::neuve();
        let mut boutique = BoutiqueScriptee::passive();
        simulate_with(&config, 3, &mut politique, &mut boutique);

        let premiere = politique
            .vues
            .first()
            .cloned()
            .expect("au moins une décision");
        let suivante = politique
            .vues
            .iter()
            .find(|vue| vue.mains < premiere.mains)
            .cloned()
            .expect("une seconde main");

        // Le repli soumet la première figure jouable non consommée.
        let figure = premiere
            .figures
            .iter()
            .find(|f| !premiere.grille.contains(f.hand))
            .expect("une figure jouable");
        let session = SimSession::new(CupId::Standard, 1, 3);
        let contexte = BlindContext {
            blind: manche_avec(None),
            target_score: premiere.cible,
            current_score: premiere.score,
            hands_remaining: premiere.mains,
            used_hands: premiere.grille,
        };
        let attendu = ScoringPipeline::resolve(
            figure,
            &premiere.des,
            &session.hand_levels,
            &session.relics,
            &contexte,
        )
        .final_score;

        assert_eq!(
            suivante.score,
            premiere.score.saturating_add(attendu),
            "le score n'a pas été commis exactement une fois"
        );
        assert_eq!(
            suivante.mains,
            premiere.mains - 1,
            "la main n'a décru qu'une fois"
        );
        assert!(suivante.grille.contains(figure.hand));
    }

    #[test]
    fn test_rerolls_saturate() {
        // Gobelet Abandonné (zéro relance de base) + Stake 4 + L'Étau.
        let abandonne = RunConfig::from_cup(&cup(CupId::Abandoned));
        assert_eq!(abandonne.base_rerolls, 0, "le gobelet a changé");
        let stock = RelicInventory::new(abandonne.relic_capacity);
        let etau = manche_avec(Some(BlindModifier::MaxRerolls(1)));

        let n = relances(&abandonne, 4, &etau, &stock);
        assert_eq!(n, 0, "la chaîne a débordé");
        assert_ne!(n, 255);

        // **Cette moitié seule discrimine.** Sur le gobelet abandonné, chaque
        // maillon rend déjà zéro : un bouchon de stake inerte et un plafond
        // ignoré y passent tous deux, et le banc les a montrés survivants
        // jusqu'à ce que ces lignes existent.
        let neutre = RunConfig::from_cup(&cup(CupId::Standard));
        assert_eq!(neutre.base_rerolls, 2, "le gobelet a changé");
        let libre = manche_avec(None);
        assert_eq!(relances(&neutre, 1, &libre, &stock), 2, "chaîne inerte");
        assert_eq!(
            relances(&neutre, 4, &libre, &stock),
            1,
            "le stake ne mord pas"
        );
        assert_eq!(
            relances(&neutre, 1, &etau, &stock),
            1,
            "le plafond ne mord pas"
        );
        assert_eq!(relances(&neutre, 4, &etau, &stock), 1, "les deux ensemble");
    }

    #[test]
    fn test_reroll_chain_matches_effective_rerolls() {
        // **L'enveloppe est comparée au moteur, pas à elle-même.** Elle lui
        // fournit les deux deltas de sa signature ; l'arithmétique reste au
        // moteur, et il n'existe qu'une seule chaîne dans le projet.
        for base in [0u8, 1, 2] {
            for stake in [1u8, 4] {
                for cap in [None, Some(0), Some(1), Some(2), Some(3)] {
                    let mut config = RunConfig::from_cup(&cup(CupId::Standard));
                    config.base_rerolls = base;
                    let stock = RelicInventory::new(config.relic_capacity);
                    let manche = manche_avec(cap.map(BlindModifier::MaxRerolls));
                    let attendu = effective_rerolls(
                        &config,
                        as_negative_stake_delta(u8::from(stake == 4)),
                        cap,
                        0,
                    );
                    assert_eq!(
                        relances(&config, stake, &manche, &stock),
                        attendu,
                        "base {base} stake {stake} cap {cap:?}"
                    );
                }
            }
        }

        // Le maillon relique est **postérieur** au plafond, et il est signé.
        let mut config = RunConfig::from_cup(&cup(CupId::Standard));
        config.base_rerolls = 2;
        let mut stock = RelicInventory::new(config.relic_capacity);
        stock.add_relic(RelicId::UnstableObsidian);
        let etau = manche_avec(Some(BlindModifier::MaxRerolls(2)));
        assert_eq!(
            relic_reroll_malus(&stock, &etau),
            -1,
            "le delta n'est pas lu"
        );
        assert_eq!(
            relances(&config, 1, &etau, &stock),
            1,
            "le delta ne s'applique pas"
        );
        // Un delta **positif** dépasse légitimement le plafond : aucune relique
        // de cette étape n'en porte, et le moteur le déclare pourtant légitime.
        assert_eq!(effective_rerolls(&config, 0, Some(1), 2), 3);

        // Une relique neutralisée par la cage ne porte plus son delta.
        let cage = manche_avec(Some(BlindModifier::DisableRelicSlot(0)));
        assert_eq!(
            relic_reroll_malus(&stock, &cage),
            0,
            "la neutralisation est ignorée"
        );
    }

    #[test]
    fn test_blind_outcome_order_is_normative() {
        // Cible atteinte **à la dernière main** : boutique, pas défaite.
        let gagnee = BlindContext {
            blind: manche_avec(None),
            target_score: 600,
            current_score: 600,
            hands_remaining: 0,
            used_hands: HandGrid::default(),
        };
        assert_eq!(issue_de_manche(&gagnee), IssueManche::Battue);

        let perdue = BlindContext {
            current_score: 599,
            ..gagnee.clone()
        };
        assert_eq!(issue_de_manche(&perdue), IssueManche::RunPerdue);

        let encore = BlindContext {
            current_score: 599,
            hands_remaining: 1,
            ..gagnee
        };
        assert_eq!(issue_de_manche(&encore), IssueManche::MainSuivante);
    }

    #[test]
    fn test_resolve_receives_the_full_dice_slice() {
        let deck = cup(CupId::Standard);
        let config = RunConfig::from_cup(&deck);
        let mut pool = DicePool::new(&config, &deck.sides);
        let premier = pool.dice()[0].id;
        pool.remove(premier);
        pool.push(Die::new(DieId(0), 6));

        let tries = des_tries(&pool);
        assert_eq!(tries.len(), pool.len(), "le slice est amputé");
        assert!(
            tries.windows(2).all(|paire| paire[0].id <= paire[1].id),
            "le slice n'est pas trié par identifiant"
        );
    }

    #[test]
    fn test_evaluations_are_rescored_with_hand_levels() {
        let deck = cup(CupId::Standard);
        let config = RunConfig::from_cup(&deck);
        let mut pool = DicePool::new(&config, &deck.sides);
        let mut rng = RunRng::from_seed(5);
        pool.roll_all(&mut rng.dice, true);
        let des = des_tries(&pool);

        let niveau_un = HandLevels::default();
        let mut niveau_trois = HandLevels::default();
        let cible = HandEvaluator::evaluate(&des)
            .first()
            .map(|figure| figure.hand)
            .expect("au moins une figure");
        niveau_trois.upgrade(cible);
        niveau_trois.upgrade(cible);

        let bas = evaluer(&des, &niveau_un);
        let haut = evaluer(&des, &niveau_trois);
        let apercu = |liste: &[HandMatch]| {
            liste
                .iter()
                .find(|f| f.hand == cible)
                .map(|f| f.potential_score)
        };
        assert_ne!(
            apercu(&bas),
            apercu(&haut),
            "les aperçus ne sont pas rescorés"
        );
    }

    #[test]
    fn test_shop_actions_are_applied_by_the_loop() {
        let mut session = SimSession::new(CupId::Standard, 1, 1);
        session.gold = 100;
        let mut etalage = generate_shop(&mut session.rng.shop);
        let avant = etalage.items.len();
        assert!(avant >= 3, "étalage trop court pour le scénario");
        let prix: u32 = price_of(&etalage.items[1]) + price_of(&etalage.items[0]);

        let anomalies = appliquer_achats(
            &mut session,
            &mut etalage,
            &[
                ShopAction::Buy(1),
                ShopAction::Buy(0),
                ShopAction::Leave,
                ShopAction::Buy(0),
            ],
        );

        assert_eq!(anomalies, 0);
        assert_eq!(
            session.gold,
            100 - prix,
            "l'or n'a pas été débité sur la session"
        );
        assert_eq!(
            etalage.items.len(),
            avant - 2,
            "le Buy après Leave a été exécuté"
        );
    }

    #[test]
    fn test_buy_is_refused_on_gold_and_on_capacity() {
        let mut session = SimSession::new(CupId::Standard, 1, 1);
        session.gold = 0;
        let mut etalage = generate_shop(&mut session.rng.shop);
        let avant = etalage.items.len();
        assert_eq!(
            appliquer_achats(&mut session, &mut etalage, &[ShopAction::Buy(0)]),
            0
        );
        assert_eq!(session.gold, 0);
        assert_eq!(
            etalage.items.len(),
            avant,
            "un article est sorti sans être payé"
        );

        // Capacité atteinte : le gobelet neutre en porte cinq, celui de
        // Fortune six — et aucun littéral ne le sait.
        for (id, capacite) in [(CupId::Standard, 5u8), (CupId::Fortune, 6)] {
            let mut session = SimSession::new(id, 1, 1);
            assert_eq!(session.config.relic_capacity, capacite);
            session.gold = 1_000;
            for _ in 0..capacite {
                session.relics.add_relic(RelicId::PolishedStone);
            }
            let mut etalage = ShopInventory {
                items: vec![ShopItem::RelicCard(RelicId::PolishedStone)],
                reroll_cost: 5,
            };
            appliquer_achats(&mut session, &mut etalage, &[ShopAction::Buy(0)]);
            assert_eq!(session.relics.len(), usize::from(capacite), "{id:?}");
            assert_eq!(session.gold, 1_000, "{id:?} : débit sur un refus");
        }

        // **La moitié « acceptation », et elle seule discrimine.** Le refus
        // seul passe avec un littéral cinq : c'est le sixième slot du Gobelet
        // de Fortune qui dit que la capacité vient de la configuration. Mesuré
        // au banc, le mutant survivait sans ces lignes.
        let mut session = SimSession::new(CupId::Fortune, 1, 1);
        session.gold = 1_000;
        for _ in 0..5 {
            session.relics.add_relic(RelicId::PolishedStone);
        }
        let mut etalage = ShopInventory {
            items: vec![ShopItem::RelicCard(RelicId::PolishedStone)],
            reroll_cost: 5,
        };
        appliquer_achats(&mut session, &mut etalage, &[ShopAction::Buy(0)]);
        assert_eq!(session.relics.len(), 6, "le sixième slot a été refusé");
        assert!(session.gold < 1_000, "le sixième achat n'a pas été payé");
    }

    #[test]
    fn test_grid_upgrade_raises_the_hand_level() {
        let mut session = SimSession::new(CupId::Standard, 1, 1);
        session.gold = 100;
        let mut etalage = ShopInventory {
            items: vec![ShopItem::GridUpgrade(YahtzeeHand::FullHouse)],
            reroll_cost: 5,
        };
        appliquer_achats(&mut session, &mut etalage, &[ShopAction::Buy(0)]);

        assert_eq!(session.hand_levels.level(YahtzeeHand::FullHouse), 2);
        for hand in YahtzeeHand::ALL {
            if hand != YahtzeeHand::FullHouse {
                assert_eq!(session.hand_levels.level(hand), 1, "{hand:?}");
            }
        }
    }

    #[test]
    fn test_abandoned_run_is_marked_not_silently_fixed() {
        let config = campagne(CupId::Standard, 1);
        // Deux soumissions de la même figure dans la même manche.
        let mut politique = Scriptee::avec(vec![
            HandDecision::Submit(YahtzeeHand::Chance),
            HandDecision::Submit(YahtzeeHand::Chance),
        ]);
        let mut boutique = BoutiqueScriptee::passive();
        let issue = simulate_with(&config, 11, &mut politique, &mut boutique);

        assert!(issue.abandoned, "le run n'est pas marqué");
        assert_eq!(issue.anomalies, 1, "l'anomalie n'est pas comptée");
    }

    #[test]
    fn test_relic_states_advance_on_hand_scored() {
        // **Le document d'étape ne dit nulle part où la boucle fait avancer les
        // états.** Sans ces deux tests, toute relique à mémoire reste inerte et
        // la table de contribution la rapporte à zéro sans que rien ne désigne
        // la cause.
        let mut session = SimSession::new(CupId::Standard, 1, 1);
        session.relics.add_relic(RelicId::ClayPiggyBank);
        let deck = cup(CupId::Standard);
        let mut manche = blind_context(manche_avec(None), &session.config);
        let mut politique = Scriptee::neuve();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut resultat = issue_vide();

        let main = jouer_une_main(
            &mut session,
            &mut manche,
            &deck.sides,
            &mut politique,
            &mut rng,
            &mut resultat,
        )
        .expect("une main jouable");

        let etat = session.relics.slots[0].as_ref().map(|inst| inst.state);
        assert_ne!(etat, Some(RelicState::None), "l'état n'a pas avancé");
        assert_eq!(
            etat,
            Some(RelicState::Counter(u32::from(main.rerolls_left))),
            "la tirelire n'a pas cumulé les relances restantes"
        );
    }

    #[test]
    fn test_relic_gold_is_collected_before_states_advance() {
        let mut session = SimSession::new(CupId::Standard, 1, 1);
        session.relics.add_relic(RelicId::ClayPiggyBank);
        if let Some(inst) = session.relics.slots[0].as_mut() {
            inst.state = RelicState::Counter(3);
        }
        let avant = session.gold;
        let manche = blind_context(manche_avec(None), &session.config);
        let deck = cup(CupId::Standard);
        let des = DicePool::new(&session.config, &deck.sides).dice().to_vec();
        let figure = HandEvaluator::evaluate(&des)
            .into_iter()
            .next()
            .expect("une figure");
        let derniere = DerniereMain {
            figure,
            des,
            rerolls_left: 0,
        };

        encaisser_fin_de_manche(&mut session, &manche, &derniere);

        // Ce qui distingue les deux ordres, ce sont les trois pièces de la
        // tirelire. Avancer d'abord la remettrait à zéro, **en silence**.
        let sans_relique = calculate_payout(
            &manche.blind,
            manche.hands_remaining,
            avant,
            &session.config,
            0,
        )
        .total;
        assert_eq!(
            session.gold - avant,
            sans_relique + 3,
            "l'or de la relique n'a pas été encaissé avant l'avancement"
        );
        assert_eq!(
            session.relics.slots[0].as_ref().map(|inst| inst.state),
            Some(RelicState::Counter(0)),
            "l'état n'a pas été remis à zéro après l'encaissement"
        );
    }

    #[test]
    fn test_a_debuffed_hand_is_refused_like_a_used_one() {
        // **La contrainte n'est pas portée par le score.** Une figure interdite
        // par *L'Oubli* ne vaut pas zéro : elle ne se joue pas, et c'est le
        // refus de la soumission qui le tient — côté jeu comme ici.
        let config = campagne(CupId::Standard, 1);
        let oubli = manche_avec(Some(BlindModifier::DebuffHands(smallvec![
            YahtzeeHand::Chance,
            YahtzeeHand::Yahtzee
        ])));
        assert!(!figure_jouable(
            &blind_context(oubli.clone(), &RunConfig::from_cup(&cup(CupId::Standard))),
            YahtzeeHand::Chance
        ));
        assert!(figure_jouable(
            &blind_context(oubli, &RunConfig::from_cup(&cup(CupId::Standard))),
            YahtzeeHand::FullHouse
        ));

        // Et la boucle la refuse comme une figure consommée.
        let mut session = SimSession::new(CupId::Standard, 1, 1);
        let deck = cup(CupId::Standard);
        let mut manche = blind_context(
            manche_avec(Some(BlindModifier::DebuffHands(smallvec![
                YahtzeeHand::Chance
            ]))),
            &session.config,
        );
        let mut politique = Scriptee::avec(vec![HandDecision::Submit(YahtzeeHand::Chance)]);
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut resultat = issue_vide();
        let issue = jouer_une_main(
            &mut session,
            &mut manche,
            &deck.sides,
            &mut politique,
            &mut rng,
            &mut resultat,
        );
        assert!(issue.is_err(), "la figure interdite a été jouée");
        assert_eq!(resultat.anomalies, 1);
        let _ = config;
    }

    #[test]
    fn test_a_reroll_locks_the_masked_dice() {
        let config = campagne(CupId::Standard, 1);
        let mut politique = Scriptee::avec(vec![HandDecision::Reroll(LockMask::new([DieId(0)]))]);
        let mut boutique = BoutiqueScriptee::passive();
        simulate_with(&config, 13, &mut politique, &mut boutique);

        let avant = &politique.vues[0];
        let apres = &politique.vues[1];
        assert_eq!(
            apres.relances,
            avant.relances - 1,
            "la relance n'a pas été décomptée"
        );
        let garde = |vue: &Instantane| {
            vue.des
                .iter()
                .find(|die| die.id == DieId(0))
                .map(|die| die.current_value)
        };
        assert_eq!(garde(avant), garde(apres), "le dé conservé a été relancé");
    }
}
