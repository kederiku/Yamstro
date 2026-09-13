//! Mise en place d'une manche : chaîne saturante des relances, construction du
//! contexte de blind, et arbitrage de fin de run.
//!
//! # Une seule chaîne de relances dans le projet
//!
//! `resolve_rerolls` **délègue** à `core_engine::config::effective_rerolls`
//! plutôt que de récrire l'arithmétique. L'ordre `base(cup) → stake →
//! blind_modifier → relic_modifier` est normatif (ADR-007) et il est déjà
//! implémenté une fois ; deux chaînes qui divergeraient au premier maillon
//! ajouté est précisément le défaut à rendre impossible. Les quatre
//! commentaires numérotés restent portés par les quatre arguments de l'appel.
//!
//! # L'ordre ne suffit pas, la garde non plus
//!
//! `check_run_completion` et `setup_blind` sont dans le même
//! `OnEnter(RunPhase::BlindSelect)`, chaînés dans cet ordre. Mais un changement
//! d'état ne prend effet qu'au `StateTransition` suivant : sans garde,
//! `setup_blind` monterait quand même une manche pour une run déjà gagnée.
//! `setup_blind` porte donc une condition qui lit `NextState<AppState>`. Il
//! faut **les deux** : la garde sans l'ordre ne verrait jamais la victoire, et
//! l'ordre sans la garde ne l'empêcherait de rien.
//!
//! `run_in`, `run_after` et `run_before` n'existent plus en 0.19 ; les
//! combinateurs sont `.chain()`, `.before()` et `.after()`.
//!
//! # Entrer dans une main : trois pièges d'ordonnancement
//!
//! Mesuré, et contraignant pour `setup_round` :
//!
//! 1. Une entité créée par `Commands` **n'est pas visible** de la `Query` du
//!    système qui l'a créée ; elle ne le devient qu'au passage suivant. Les
//!    dés manquants sont donc construits et **roulés en local**, puis spawnés
//!    déjà formés. Un spawn nu suivi d'un tour de `Query` les laisserait sur
//!    la face 1, et l'écart ne se verrait qu'à l'écran.
//! 2. L'ordre d'itération d'une `Query` suit l'archétype, pas le spawn : poser
//!    puis retirer `Locked` déplace l'entité. Le flux de dés se consomme donc
//!    **dans l'ordre des `DieId`**, triés explicitement, faute de quoi deux
//!    runs de même graine divergent.
//! 3. Une ré-entrée `Roll → Roll` déclenche bien les schedules de sortie et
//!    d'entrée. Les dés sont rattachés à `DespawnOnExit(AppState::InRun)`, et
//!    jamais à une phase : mesuré, ils survivent alors à la main suivante.
//!
//! # Les `DieId` ne sont jamais réutilisés
//!
//! `NextDieId` porte un compteur monotone à l'échelle de la run. Ce n'est pas
//! un ornement : la règle « `1 + max(DieId présents)` » ne tient pas la
//! propriété dès qu'un dé disparaît — cinq dés, retrait du cinquième, retour à
//! cinq, et l'identifiant retiré est réattribué. C'est exactement le cycle
//! qu'ouvre le boss *La Meule* à l'Étape 9, et c'est pourquoi `DicePool`
//! (TASK-09) porte un `next_id` privé que `remove` ne décrémente jamais. La
//! formule reste la règle d'**amorçage**, employée une seule fois.
//!
//! # Pourquoi des ressources optionnelles
//!
//! `RunPhase::BlindSelect` est l'état par défaut sous `AppState::InRun` : on y
//! entre **avant** toute run réelle, y compris dans une application montée
//! headless sans session. Mesuré : au premier `OnEnter(BlindSelect)`, le
//! `BlindContext` est absent du monde. Les deux systèmes lisent donc leurs
//! ressources de run en `Option` et ne font rien quand elles manquent — sans
//! session il n'y a pas de manche à monter, sans contexte il n'y a pas de blind
//! battue donc pas de victoire possible.

use bevy::prelude::*;
use core_engine::blinds::{BlindContext, BlindDefinition, BlindModifier, BlindType};
use core_engine::config::effective_rerolls;
use core_engine::cups::definitions::cup;
use core_engine::dice::{Die, DieId};
use core_engine::hands::HandGrid;
use core_engine::relics::RelicInventory;
use core_engine::relics::effects::roll_modifier_for;

use crate::components::{DieView, Hidden, Locked, Scoring};
use crate::resources::{HandContext, RunSession};
use crate::states::{AppState, RunPhase};

/// Ante dont la Mise Boss clôt la run.
// Étape 6 : la progression complète des antes.
const FINAL_ANTE: u8 = 8;

/// Étape 10 — Stakes. Seule valeur spécifiée par le corpus : le Stake 4 coûte
/// une relance.
/// À REMPLACER par la table des Stakes, jamais à contourner : un appelant qui
/// court-circuite cette fonction casse la chaîne au moment où l'Étape 10 la
/// remplit.
fn stake_reroll_malus(stake_level: u8) -> u8 {
    if stake_level == 4 { 1 } else { 0 }
}

/// Somme des deltas de relance des reliques équipées, dans l'ordre des slots.
///
/// **Signé de bout en bout, et c'est le point.** Le bouchon de TASK-32 rendait
/// un `u8` — un malus toujours positif — qu'une fonction d'appoint niait
/// ensuite. Aucune relique de l'Étape 5 n'a de delta positif, mais
/// `effective_rerolls` en déclare un légitime en toutes lettres, et
/// `test_relic_delta_may_exceed_blind_cap` le verrouille : un `u8` avalerait
/// le premier que l'Étape 9 apportera, sans erreur.
///
/// L'agrégation passe par un `i16` puis sature : deux *Obsidiennes* donnent
/// `-2`, et rien n'interdit à l'Étape 9 d'en aligner davantage.
///
/// Une relique qui ne participe pas ne contribue rien — même prédicat que le
/// parcours du score, l'or et l'avancement d'état.
fn relic_reroll_malus(inventory: &RelicInventory, blind: &BlindDefinition) -> i8 {
    agreger_deltas(
        inventory
            .iter_slots()
            .filter(|(slot, instance)| instance.participe(*slot, blind))
            .map(|(_, instance)| {
                roll_modifier_for(instance.def, &[], ROLL_INDEX_HORS_LANCER).reroll_delta
            }),
    )
}

/// Somme saturante de deltas signés.
///
/// **Extraite pour être éprouvable.** Aucune relique de l'Étape 5 n'a de delta
/// positif : à travers l'inventaire, un agrégat qui forcerait le signe rendrait
/// exactement les mêmes valeurs qu'un agrégat correct, et survivrait à tous les
/// tests. Mesuré. Cette fonction reçoit les deltas directement, donc un test
/// peut lui en donner un positif — le premier que l'Étape 9 apportera.
///
/// L'accumulateur est un `i16` : cinq *Obsidiennes* donnent `-5`, et rien
/// n'interdit à l'Étape 9 d'en aligner assez pour sortir d'un `i8`.
fn agreger_deltas(deltas: impl Iterator<Item = i8>) -> i8 {
    let somme = deltas.fold(0i16, |total, delta| total.saturating_add(i16::from(delta)));
    i8::try_from(somme).unwrap_or(if somme.is_negative() {
        i8::MIN
    } else {
        i8::MAX
    })
}

/// Rang de lancer employé pour la seule consultation du delta de relance.
///
/// **Le delta se calcule avant le lancer**, quand aucun dé n'est encore tombé :
/// aucune relique ne peut le conditionner à une face, et *Dé Fantôme* ne
/// contribue de toute façon rien à ce canal. La valeur est celle qui rend sa
/// garde **fausse**, pour qu'un jour où l'on confondrait les deux consultations
/// la relique reste muette plutôt que de forcer un dé qui n'existe pas.
const ROLL_INDEX_HORS_LANCER: u8 = u8::MAX;

/// Traduit le malus de stake, toujours positif, en delta signé.
///
/// **Conservée pour le seul maillon qui en a encore besoin.** Les Stakes sont
/// l'Étape 10 et leur bouchon rend un `u8` ; les reliques, elles, rendent
/// désormais un delta signé et n'ont plus à passer par ici.
fn as_negative_stake_delta(malus: u8) -> i8 {
    i8::try_from(malus).map_or(i8::MIN, i8::wrapping_neg)
}

/// Le plafond de relances de la manche, s'il y en a un.
fn blind_cap(blind: &BlindDefinition) -> Option<u8> {
    if let Some(BlindModifier::MaxRerolls(cap)) = blind.modifier {
        Some(cap)
    } else {
        None
    }
}

/// Nombre de relances de la manche, chaîne saturante complète.
///
/// Publique parce que la mise en place d'une **main** la consomme (TASK-33) :
/// ce ticket ne pose pas `HandContext.rerolls_left`, qui n'est pas une donnée
/// de manche.
pub fn resolve_rerolls(
    session: &RunSession,
    blind: &BlindDefinition,
    inventory: &RelicInventory,
) -> u8 {
    effective_rerolls(
        &session.config,                                                  // 1. base(cup)
        as_negative_stake_delta(stake_reroll_malus(session.stake_level)), // 2. stake
        blind_cap(blind),                                                 // 3. blind_modifier
        relic_reroll_malus(inventory, blind),                             // 4. relic_modifier
    )
}

/// Cible **effective** de la manche.
///
/// **Adaptateur, jamais une seconde courbe.** L'arithmétique vit dans
/// `core_engine::blinds::scaling`, seule à porter les constantes et les quatre
/// facteurs. L'Étape 3 en avait écrit une copie ici, avec ses trois bouchons et
/// un arrondi à chaque étage : elle rendait 5034 et 8054 aux antes 7 et 8, là où
/// la courbe normative rend 5033 et 8053. Deux courbes pour une difficulté, et
/// celle que le jeu employait était la fausse.
fn target_score(session: &RunSession, blind: &BlindDefinition) -> u64 {
    core_engine::blinds::target_score(
        session.ante,
        blind.kind,
        session.cup_id,
        session.stake_level,
        blind.modifier.as_ref(),
    )
}

fn current_blind_definition(session: &RunSession) -> BlindDefinition {
    BlindDefinition {
        kind: BlindType::Small,
        target_score: core_engine::blinds::target_score(
            session.ante,
            BlindType::Small,
            session.cup_id,
            session.stake_level,
            None,
        ),
        reward: 0,
        modifier: None,
    }
}

/// Vrai si une bascule vers `AppState::Victory` est déjà en attente.
///
/// Les deux variantes en attente de `NextState` sont couvertes : `Pending`,
/// posée par `set`, et `PendingIfNeq`, posée par `set_if_neq`. N'en couvrir
/// qu'une laisserait la garde muette selon l'appelant.
pub(crate) fn victory_is_pending(next: Res<NextState<AppState>>) -> bool {
    matches!(
        *next,
        NextState::Pending(AppState::Victory) | NextState::PendingIfNeq(AppState::Victory)
    )
}

/// `OnEnter(RunPhase::BlindSelect)` : construit le contexte de la manche.
///
/// N'écrit **que** le contexte de blind : ni score commis (ADR-010), ni
/// relances, ni transition.
fn setup_blind(mut commands: Commands, session: Option<Res<RunSession>>) {
    let Some(session) = session else {
        return;
    };

    let blind = current_blind_definition(&session);
    commands.insert_resource(BlindContext {
        target_score: target_score(&session, &blind),
        blind,
        current_score: 0,
        hands_remaining: session.config.hands_per_blind,
        // Grille neuve à chaque blind, jamais à chaque main : à l'entrée d'une
        // main la grille doit persister, sinon la décision de l'ADR-001
        // disparaît.
        used_hands: HandGrid::default(),
    });
}

/// `OnEnter(RunPhase::BlindSelect)` : bascule vers `AppState::Victory` si la
/// Mise Boss de l'ante final vient d'être battue.
///
/// Lit le contexte de la manche **précédente**, encore présent parce que ce
/// système est ordonné avant `setup_blind` et que les commandes de celui-ci ne
/// sont appliquées qu'en fin de schedule.
fn check_run_completion(
    session: Option<Res<RunSession>>,
    blind: Option<Res<BlindContext>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let (Some(session), Some(blind)) = (session, blind) else {
        return;
    };

    let gagnee = blind.blind.kind == BlindType::Boss
        && session.ante == FINAL_ANTE
        && blind.current_score >= blind.target_score;

    if gagnee {
        // Appel **qualifié** : `next.set_if_neq(..)` ne compile pas, la méthode
        // homonyme de `DetectChangesMut` capturant l'appel. Voir le `//!` de
        // `states.rs`. La règle est sans exception, même là où l'état cible ne
        // peut pas être l'état courant.
        NextState::set_if_neq(&mut next, AppState::Victory);
    }
}

/// Compteur monotone d'identifiants de dés, à l'échelle de la run.
///
/// C'est le `next_id` privé de `DicePool` (TASK-09) porté au monde : `Die` et
/// `DieView` remplacent le pool côté Bevy, mais la propriété qu'il garantissait
/// ne se garantit pas toute seule. Amorcé à `1 + max(DieId présents)`, `0` si
/// le monde n'en contient aucun ; il ne décroît ensuite jamais.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextDieId(pub u32);

/// Nombre de faces du dé de rang `index` : `sides.get(index)`, à défaut le
/// **dernier** élément. C'est la règle exacte du constructeur de `DicePool`
/// (TASK-09 § 2),
/// et jamais un nombre de faces écrit en dur — le Polyèdre porte un D8 en
/// dernière position, et un dé au-delà de la table en porte un aussi.
///
/// `sides` vide ne peut pas venir du catalogue, dont l'invariant est
/// `sides.len() == dice_count` ; le dé dégénéré rendu ici a une face, ce que
/// `Die::new` accepte, plutôt qu'une panique.
fn sides_for_rank(sides: &[u8], index: usize) -> u8 {
    sides
        .get(index)
        .or_else(|| sides.last())
        .copied()
        .unwrap_or(1)
}

/// Rang d'affichage. `dice_count` étant un `u8`, la saturation est
/// inatteignable ; elle évite une conversion faillible dans une boucle.
fn rank_of(index: usize) -> u8 {
    u8::try_from(index).unwrap_or(u8::MAX)
}

/// `OnEnter(RunPhase::Roll)` : entrée dans une main.
///
/// Calcule les relances, réinitialise le contexte de main, ajuste le nombre de
/// dés à la configuration puis les roule. N'écrit rien dans le contexte de
/// blind et ne décrémente aucune relance.
///
/// **Deux flux, et deux seulement.** `rng.dice` pour les lancers, et `rng.boss`
/// pour les dés que *La Fissure* cache : ceux-ci ne peuvent se tirer qu'ici,
/// les dés n'existant pas avant, et les tirer sur `rng.dice` décalerait la
/// séquence des lancers selon la présence du boss.
fn setup_round(
    mut commands: Commands,
    session: Option<ResMut<RunSession>>,
    blind: Option<Res<BlindContext>>,
    inventory: Option<Res<RelicInventory>>,
    next_id: Option<Res<NextDieId>>,
    mut dice: Query<(Entity, &mut Die, &mut DieView)>,
) {
    let (Some(mut session), Some(blind), Some(inventory)) = (session, blind, inventory) else {
        return;
    };

    // 1. Les relances, calculées avant tout emprunt mutable du flux.
    let rerolls_left = resolve_rerolls(&session, &blind.blind, &inventory);

    // 2. Le contexte de main est intégralement réinitialisé. Il est **posé**,
    //    et non muté : ses trois champs sont réécrits, et aucun autre système
    //    ne l'insère — sans cela il n'existerait nulle part.
    commands.insert_resource(HandContext {
        rerolls_left,
        active_evaluations: Vec::new(),
        selected_hand: None,
    });

    // 3. Les dés présents, triés par identifiant. Le tri n'est pas cosmétique :
    //    l'ordre d'itération suit l'archétype, que le verrouillage déplace.
    let mut present: Vec<(Entity, DieId)> = dice.iter().map(|(e, die, _)| (e, die.id)).collect();
    present.sort_unstable_by_key(|(_, id)| *id);

    let seed = present.last().map_or(0, |(_, id)| id.0.saturating_add(1));
    let mut next = next_id.map_or(seed, |counter| counter.0.max(seed));

    let wanted = usize::from(session.config.dice_count);
    let sides = cup(session.cup_id).sides;

    // Le surplus part par les identifiants les plus élevés.
    let kept = wanted.min(present.len());
    for (entity, _) in present.drain(kept..) {
        commands.entity(entity).despawn();
    }

    // *La Fissure* cache ses dés **au lancer**, et le tirage porte sur les
    // rangs : les dés manquants naissent plus bas, par `Commands`, et ne sont
    // donc pas encore visibles ici. Un tirage sur les entités présentes
    // laisserait la première manche d'une run sans aucun dé caché.
    //
    // Le marqueur est retiré de tous les dés avant d'être reposé : survivant à
    // une manche sans boss, il laisserait la liste des figures vide sans
    // raison visible.
    let masques =
        core_engine::blinds::hidden_ranks(blind.blind.hidden_dice(), wanted, &mut session.rng.boss);

    // 4 et 5 sur les dés conservés, dans l'ordre des identifiants.
    for (index, (entity, _)) in present.iter().enumerate() {
        // Le marqueur et le champ se retirent ensemble : c'est `Die.locked`
        // que `Die::roll` consulte, et une divergence rendrait le verrouillage
        // inopérant sans erreur de compilation.
        commands
            .entity(*entity)
            .remove::<(Locked, Scoring, Hidden)>();
        if masques.contains(&index) {
            commands.entity(*entity).insert(Hidden);
        }

        let Ok((_, mut die, mut view)) = dice.get_mut(*entity) else {
            continue;
        };
        die.locked = false;
        view.order = rank_of(index);
        die.roll(&mut session.rng.dice, false);
    }

    // Les dés manquants : construits, roulés, puis spawnés déjà formés.
    for index in present.len()..wanted {
        let mut die = Die::new(DieId(next), sides_for_rank(&sides, index));
        next = next.saturating_add(1);
        die.roll(&mut session.rng.dice, false);

        let mut neuf = commands.spawn((
            die,
            DieView {
                order: rank_of(index),
            },
            DespawnOnExit(AppState::InRun),
        ));
        if masques.contains(&index) {
            neuf.insert(Hidden);
        }
    }

    commands.insert_resource(NextDieId(next));
}

/// Applique les valeurs forcées par les reliques, **après** le lancer.
///
/// # Pourquoi un second système
///
/// Les dés manquants naissent par `Commands`, donc ils n'existent pas encore
/// dans la requête de `setup_round` : à la **première manche d'une run**, tous
/// les dés sont neufs et aucun n'y serait visible. Un système ordonné après
/// voit le monde une fois les commandes appliquées, `auto_insert_apply_deferred`
/// posant le point de synchronisation à l'arête d'ordonnancement.
///
/// C'est aussi la seconde des **deux** consultations de `roll_modifier_for` :
/// le delta de relance se calcule avant le lancer, quand aucun dé n'est tombé,
/// et les valeurs forcées après. Une seule consultation ne peut pas servir les
/// deux.
///
/// **Aucun aléatoire n'est consommé ici** : forcer une valeur est déterministe,
/// et un tirage décalerait le flux de dés, faisant diverger deux runs de même
/// graine (ADR-003).
pub fn apply_forced_values(
    inventory: Option<Res<RelicInventory>>,
    blind: Option<Res<BlindContext>>,
    mut dice: Query<&mut Die>,
) {
    let (Some(inventory), Some(blind)) = (inventory, blind) else {
        return;
    };

    let tombes: Vec<Die> = dice.iter().cloned().collect();
    let forcees: Vec<(DieId, u8)> = inventory
        .iter_slots()
        .filter(|(slot, instance)| instance.participe(*slot, &blind.blind))
        .flat_map(|(_, instance)| {
            roll_modifier_for(instance.def, &tombes, PREMIER_LANCER).force_values
        })
        .collect();
    if forcees.is_empty() {
        return;
    }

    let mut a_ecrire: Vec<Mut<Die>> = dice.iter_mut().collect();
    appliquer_valeurs_forcees(&mut a_ecrire, &forcees);
}

/// Rang du premier lancer d'une manche. **Zéro**, et c'est la convention que
/// *Dé Fantôme* lit : partir de un la rendrait silencieusement inerte.
const PREMIER_LANCER: u8 = 0;

/// Écrit les valeurs forcées sur les dés visés, **par identité**.
///
/// Un identifiant absent est ignoré sans panique : une relique peut viser un dé
/// que *La Meule* vient de retirer. L'ordre des slots est celui de la liste, si
/// bien qu'une relique de droite écrase une relique de gauche sur le même dé.
fn appliquer_valeurs_forcees(dice: &mut [Mut<'_, Die>], forcees: &[(DieId, u8)]) {
    for (cible, valeur) in forcees {
        if let Some(die) = dice.iter_mut().find(|die| die.id == *cible) {
            die.current_value = *valeur;
        }
    }
}

/// Branche les systèmes de mise en place. Pour la manche, l'ordre **et** la
/// garde ; pour la main, un seul système.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        OnEnter(RunPhase::BlindSelect),
        (
            check_run_completion,
            setup_blind.run_if(not(victory_is_pending)),
        )
            .chain(),
    );

    app.add_systems(
        OnEnter(RunPhase::Roll),
        (setup_round, apply_forced_values).chain(),
    );
}

#[cfg(test)]
mod tests {
    // `use super::*` apporte déjà le prélude de Bevy, ainsi que les types de
    // `core_engine` importés par l'implémentation.
    use super::*;
    use core_engine::config::RunConfig;
    use core_engine::cups::CupId;
    use core_engine::cups::definitions::cup;
    use core_engine::evaluator::HandMatch;
    use core_engine::hands::YahtzeeHand;
    use std::collections::BTreeSet;

    use crate::components::{Locked, Scoring};
    use crate::resources::HandContext;
    use crate::systems::fixtures::{
        app_a_la_graine, app_en_run, deck_de, des_tries, entites_des, entrer_dans_roll, inventaire,
        session,
    };

    /// Définition inerte, au plafond de relances près.
    fn blind(cap: Option<u8>) -> BlindDefinition {
        BlindDefinition {
            modifier: cap.map(BlindModifier::MaxRerolls),
            ..BlindDefinition::default()
        }
    }

    /// Arme le contexte courant sur les trois conditions de victoire, puis
    /// entre dans une nouvelle blind.
    fn arme_et_rentre(app: &mut App, kind: BlindType, ante: u8, atteint: bool) {
        app.world_mut().resource_mut::<RunSession>().ante = ante;
        {
            let mut manche = app.world_mut().resource_mut::<BlindContext>();
            let cible = manche.target_score;
            manche.blind.kind = kind;
            manche.current_score = if atteint {
                cible
            } else {
                cible.saturating_sub(1)
            };
        }
        rentrer_dans_une_blind(app);
    }

    /// Sort de `BlindSelect` puis y revient : la manche suivante du même ante.
    fn rentrer_dans_une_blind(app: &mut App) {
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::Shop);
        app.update();
        app.world_mut()
            .resource_mut::<NextState<RunPhase>>()
            .set(RunPhase::BlindSelect);
        app.update();
    }

    #[test]
    fn test_dice_count_follows_run_config() {
        // Monte **puis redescend** dans une seule application : trois
        // applications séparées n'exerceraient jamais le retrait du surplus.
        let mut app = app_en_run(CupId::Standard);

        for n in [4_u8, 5, 6, 4] {
            app.world_mut().resource_mut::<RunSession>().config = RunConfig::from_cup(&deck_de(n));
            entrer_dans_roll(&mut app);

            let des = des_tries(&mut app);
            assert_eq!(des.len(), usize::from(n), "{n} dés attendus");

            let rangs: Vec<u8> = des.iter().map(|(_, _, w)| w.order).collect();
            let attendus: Vec<u8> = (0..n).collect();
            assert_eq!(rangs, attendus, "rangs 0..{n}, sans trou ni doublon");
        }
    }

    #[test]
    fn test_second_entry_does_not_leak_dice() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);
        let avant = entites_des(&mut app);

        // `set` nu vers l'état courant : la transition a bien lieu, et les
        // entités rattachées à `DespawnOnExit(AppState::InRun)` y survivent.
        entrer_dans_roll(&mut app);
        let apres = entites_des(&mut app);

        assert_eq!(avant.len(), apres.len(), "le nombre de dés a bougé");
        assert_eq!(avant, apres, "des dés ont été détruits puis recréés");
    }

    #[test]
    fn test_setup_round_clears_locked_and_scoring() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        let entites = entites_des(&mut app);
        for entite in &entites {
            app.world_mut()
                .entity_mut(*entite)
                .insert((Locked, Scoring));
            app.world_mut().get_mut::<Die>(*entite).expect("dé").locked = true;
        }

        entrer_dans_roll(&mut app);

        for entite in &entites {
            assert!(
                app.world().get::<Locked>(*entite).is_none(),
                "marqueur Locked resté"
            );
            assert!(
                app.world().get::<Scoring>(*entite).is_none(),
                "marqueur Scoring resté"
            );
            // Le marqueur et le champ se retirent ensemble : c'est `Die.locked`
            // que `Die::roll` consulte, et une divergence rendrait le
            // verrouillage inopérant sans erreur de compilation.
            assert!(
                !app.world().get::<Die>(*entite).expect("dé").locked,
                "champ Die.locked resté vrai"
            );
        }
    }

    #[test]
    fn test_setup_round_resets_selected_hand() {
        let mut app = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut app);

        {
            let mut main = app.world_mut().resource_mut::<HandContext>();
            main.selected_hand = Some(YahtzeeHand::Chance);
            main.active_evaluations.push(HandMatch {
                hand: YahtzeeHand::Chance,
                scoring_dice: Vec::new(),
                discarded_dice: Vec::new(),
                potential_score: 0,
            });
        }

        entrer_dans_roll(&mut app);

        let main = app.world().resource::<HandContext>();
        assert_eq!(main.selected_hand, None);

        // `setup_round` vide la liste en `StateTransition` ; depuis TASK-35,
        // `update_hand_evaluations` la remplit dans la **même frame**, en
        // `Update`. Ce qui se vérifie ici est donc la disparition de l'entrée
        // témoin, et non une liste vide : une figure réelle retient toujours au
        // moins un dé, le témoin n'en retient aucun.
        assert!(
            !main
                .active_evaluations
                .iter()
                .any(|figure| figure.scoring_dice.is_empty()),
            "l'évaluation témoin a survécu à l'entrée dans la main"
        );
    }

    #[test]
    fn test_same_seed_same_first_roll() {
        // Deux applications de même graine, dont l'une a vu ses dés changer
        // d'archétype : poser `Locked` déplace l'entité, et l'ordre
        // d'itération d'une `Query` suit l'archétype. Sans le tri par `DieId`
        // avant de consommer le flux, les deux suites divergeraient alors que
        // le flux est identique.
        let mut a = app_a_la_graine(CupId::Standard, 42);
        let mut b = app_a_la_graine(CupId::Standard, 42);

        let suite = |app: &mut App| -> Vec<(DieId, u8)> {
            des_tries(app)
                .into_iter()
                .map(|(_, d, _)| (d.id, d.current_value))
                .collect()
        };

        entrer_dans_roll(&mut a);
        entrer_dans_roll(&mut b);
        let premiere = suite(&mut a);
        assert_eq!(premiere, suite(&mut b));

        // Les dés sont bel et bien roulés : un dé neuf **non** roulé reste sur
        // la face 1, et rien d'autre dans la suite ne le verrait.
        assert!(
            premiere.iter().any(|(_, valeur)| *valeur != 1),
            "aucun dé n'a bougé de la face 1"
        );

        for (rang, entite) in entites_des(&mut b).into_iter().enumerate() {
            if rang % 2 == 0 {
                b.world_mut().entity_mut(entite).insert(Locked);
            }
        }

        entrer_dans_roll(&mut a);
        entrer_dans_roll(&mut b);
        let seconde = suite(&mut a);

        assert_eq!(seconde, suite(&mut b));
        // Un dé **conservé** est roulé lui aussi. Sans cette assertion, une
        // entrée qui ne relancerait que les dés neufs passerait inaperçue.
        assert_ne!(premiere, seconde, "la seconde entrée n'a rien roulé");
    }

    #[test]
    fn test_rerolls_come_from_resolve_rerolls() {
        let mut app = app_en_run(CupId::Abandoned);
        app.world_mut().resource_mut::<RunSession>().stake_level = 4;
        app.world_mut()
            .resource_mut::<BlindContext>()
            .blind
            .modifier = Some(BlindModifier::MaxRerolls(1));
        entrer_dans_roll(&mut app);

        assert_eq!(app.world().resource::<HandContext>().rerolls_left, 0);

        // Contre-épreuve : sur un gobelet qui a des relances, la valeur suit la
        // chaîne. Sans elle, un compte constamment nul passerait l'assertion
        // ci-dessus, le gobelet Abandonné étant déjà à zéro.
        let mut standard = app_en_run(CupId::Standard);
        entrer_dans_roll(&mut standard);
        assert_eq!(
            standard.world().resource::<HandContext>().rerolls_left,
            cup(CupId::Standard).base_rerolls
        );
    }

    #[test]
    fn test_sides_follow_the_cup() {
        // Le Polyèdre porte [6,6,6,6,8] : le dernier dé est un D8, jamais un 6
        // écrit en dur.
        let mut app = app_en_run(CupId::Polyhedron);
        entrer_dans_roll(&mut app);

        let faces: Vec<u8> = des_tries(&mut app)
            .iter()
            .map(|(_, d, _)| d.sides)
            .collect();
        assert_eq!(faces, cup(CupId::Polyhedron).sides);

        // Au-delà de la table, la règle est le **dernier** élément : un sixième
        // dé sur le Polyèdre est encore un D8.
        app.world_mut().resource_mut::<RunSession>().config = RunConfig::from_cup(&deck_de(6));
        entrer_dans_roll(&mut app);

        let faces: Vec<u8> = des_tries(&mut app)
            .iter()
            .map(|(_, d, _)| d.sides)
            .collect();
        assert_eq!(faces.len(), 6);
        assert_eq!(faces[5], 8, "le dé hors table reprend le dernier élément");
    }

    #[test]
    fn test_die_id_counter_bootstraps_above_existing_dice() {
        // Le compteur n'existe pas encore au premier passage : il s'amorce à
        // « 1 + max(présents) ». Un monde qui porte déjà des dés — une partie
        // relue à l'Étape 10, un montage de test — ne doit pas les voir
        // réattribués. C'est la seule occasion où la formule du ticket sert.
        let mut app = app_en_run(CupId::Standard);
        app.world_mut().spawn((
            Die::new(DieId(7), 6),
            DieView { order: 0 },
            DespawnOnExit(AppState::InRun),
        ));

        entrer_dans_roll(&mut app);

        let ids: Vec<u32> = des_tries(&mut app).iter().map(|(_, d, _)| d.id.0).collect();
        assert_eq!(
            ids,
            vec![7, 8, 9, 10, 11],
            "les identifiants neufs partent au-dessus du plus haut présent"
        );
    }

    #[test]
    fn test_die_ids_are_never_reused() {
        // Rétrécir puis regrossir : la formule « 1 + max(présents) » du ticket
        // réattribuerait ici l'identifiant du dé retiré, et une relique qui
        // l'avait mémorisé pointerait sur un autre dé.
        let mut app = app_en_run(CupId::Standard);

        let mut ids_vus: BTreeSet<u32> = BTreeSet::new();
        let mut entites_vues: BTreeSet<u64> = BTreeSet::new();
        let mut precedents: Vec<u32> = Vec::new();

        for n in [5_u8, 4, 5, 3, 6] {
            app.world_mut().resource_mut::<RunSession>().config = RunConfig::from_cup(&deck_de(n));
            entrer_dans_roll(&mut app);

            for (entite, die, _) in des_tries(&mut app) {
                if entites_vues.insert(entite.to_bits()) {
                    assert!(
                        ids_vus.insert(die.id.0),
                        "DieId {} réattribué à une entité neuve",
                        die.id.0
                    );
                }
            }

            // Le surplus part par les identifiants les plus **élevés** : ce qui
            // survit d'un tour au suivant est toujours le début de la liste.
            let presents: Vec<u32> = des_tries(&mut app).iter().map(|(_, d, _)| d.id.0).collect();
            let survivants: Vec<u32> = presents
                .iter()
                .copied()
                .filter(|id| precedents.contains(id))
                .collect();
            assert_eq!(
                survivants,
                precedents[..survivants.len()],
                "le surplus n'est pas parti par le haut"
            );
            precedents = presents;
        }
    }

    #[test]
    fn test_rerolls_underflow_saturates() {
        // Gobelet Abandonné (aucune relance), Stake 4 (une relance retirée) et
        // le boss L'Étau (plafond à une). Sans saturation le compteur u8
        // repasserait à 255 en release et paniquerait en debug.
        let partie = session(CupId::Abandoned, 4);
        let stock = inventaire(&partie.config);

        assert_eq!(resolve_rerolls(&partie, &blind(Some(1)), &stock), 0);

        // Contre-épreuve : le maillon stake mord vraiment. Sans elle, un
        // `stake_reroll_malus` constamment nul passerait l'assertion ci-dessus,
        // le gobelet Abandonné étant déjà à zéro relance.
        let standard = session(CupId::Standard, 4);
        assert_eq!(
            resolve_rerolls(&standard, &blind(None), &inventaire(&standard.config)),
            cup(CupId::Standard).base_rerolls.saturating_sub(1)
        );
    }

    // ---- TASK-73 : *L'Étau*, depuis le catalogue ----

    /// Définition portant la contrainte d'un boss, telle que le catalogue la
    /// rend. **C'est l'angle neuf de ces deux tests** : les tests de TASK-32
    /// construisent leur plafond à la main et ne verraient pas *L'Étau* changer
    /// de contrainte dans `boss_definition`.
    fn manche_du_boss(id: core_engine::blinds::definitions::BossId) -> BlindDefinition {
        let mut rng = core_engine::rng::RunRng::from_seed(0);
        BlindDefinition {
            modifier: Some(
                core_engine::blinds::definitions::boss_definition(id, &mut rng.boss, 5).modifier,
            ),
            ..BlindDefinition::default()
        }
    }

    #[test]
    fn test_vise_forces_one_reroll() {
        use core_engine::blinds::definitions::BossId;

        // Gobelet standard : deux relances, plafonnées à une. Le nombre est
        // asséré en clair, là où `test_reroll_chain_matches_core_engine` se
        // déclare tautologique et ne peut rien ancrer.
        let partie = session(CupId::Standard, 0);
        let stock = inventaire(&partie.config);
        assert_eq!(cup(CupId::Standard).base_rerolls, 2, "le gobelet a changé");
        assert_eq!(
            resolve_rerolls(&partie, &manche_du_boss(BossId::Vise), &stock),
            1
        );

        // Gobelet Abandonné (0) + Stake 4 (−1) + L'Étau : zéro, jamais 255.
        // Sans saturation le compteur `u8` repasserait par le haut en release
        // et paniquerait en debug.
        let creux = session(CupId::Abandoned, 4);
        let stock = inventaire(&creux.config);
        assert_eq!(
            resolve_rerolls(&creux, &manche_du_boss(BossId::Vise), &stock),
            0
        );
    }

    #[test]
    fn test_cap_never_raises_rerolls() {
        // Le plafond est un `min`, jamais une affectation : un gobelet à deux
        // relances en garde **deux** sous `MaxRerolls(3)`. Le cas jumeau de
        // TASK-32 part d'un gobelet à zéro, où un plafond qui élève et un
        // plafond qui plafonne rendent la même chose : c'est la capacité
        // intermédiaire qui les sépare.
        let partie = session(CupId::Standard, 0);
        let stock = inventaire(&partie.config);

        assert_eq!(partie.config.base_rerolls, 2);
        assert_eq!(resolve_rerolls(&partie, &blind(Some(3)), &stock), 2);
        // Et il mord quand il est plus bas.
        assert_eq!(resolve_rerolls(&partie, &blind(Some(1)), &stock), 1);
    }

    #[test]
    fn test_blind_cap_never_raises_rerolls() {
        // Le plafond est un `min`, jamais une affectation : sur un gobelet sans
        // relance, `MaxRerolls(3)` en laisse zéro.
        let partie = session(CupId::Abandoned, 0);
        let stock = inventaire(&partie.config);

        assert_eq!(resolve_rerolls(&partie, &blind(Some(3)), &stock), 0);
    }

    #[test]
    fn test_reroll_chain_matches_core_engine() {
        // Tautologie assumée : `resolve_rerolls` délègue à `effective_rerolls`
        // (Décision de Lead du § 2.1), donc ce test ne peut pas constater une
        // divergence de calcul. Ce qu'il constate est que la délégation existe
        // encore : il tombe dès que quelqu'un réécrit la chaîne à la main dans
        // cette crate, ce qui est exactement le défaut que le ticket veut
        // rendre impossible.
        assert_eq!(
            [CupId::Abandoned, CupId::Cheater, CupId::Standard].map(|id| cup(id).base_rerolls),
            [0, 1, 2],
            "la matrice base ∈ {{0,1,2}} n'est plus couverte par ces gobelets"
        );

        for id in [CupId::Abandoned, CupId::Cheater, CupId::Standard] {
            for stake_level in [0, 4] {
                for cap in [None, Some(0), Some(1), Some(2), Some(3)] {
                    let partie = session(id, stake_level);
                    let stock = inventaire(&partie.config);

                    let attendu = effective_rerolls(
                        &partie.config,
                        as_negative_stake_delta(stake_reroll_malus(stake_level)),
                        cap,
                        relic_reroll_malus(&stock, &blind(cap)),
                    );

                    assert_eq!(
                        resolve_rerolls(&partie, &blind(cap), &stock),
                        attendu,
                        "{id:?}, stake {stake_level}, plafond {cap:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_used_hands_reset_on_each_blind() {
        let mut app = app_en_run(CupId::Standard);
        assert!(
            app.world().resource::<BlindContext>().used_hands.is_empty(),
            "première blind"
        );

        app.world_mut()
            .resource_mut::<BlindContext>()
            .used_hands
            .mark(YahtzeeHand::Yahtzee);
        assert!(!app.world().resource::<BlindContext>().used_hands.is_empty());

        rentrer_dans_une_blind(&mut app);

        assert!(
            app.world().resource::<BlindContext>().used_hands.is_empty(),
            "deuxième blind du même ante"
        );
    }

    #[test]
    fn test_hands_remaining_follows_run_config() {
        for id in [CupId::Standard, CupId::Abandoned, CupId::Fortune] {
            let mut app = app_en_run(id);
            let attendu = app.world().resource::<RunSession>().config.hands_per_blind;

            assert_eq!(
                app.world().resource::<BlindContext>().hands_remaining,
                attendu,
                "gobelet {id:?}"
            );

            // Les cinq gobelets portent aujourd'hui la même valeur : comparer à
            // la configuration ne distingue donc pas la lecture du champ d'un
            // littéral. On décale la configuration — valeur dérivée, jamais
            // écrite en dur — et on entre dans une seconde blind.
            let decale = attendu.saturating_add(1);
            app.world_mut()
                .resource_mut::<RunSession>()
                .config
                .hands_per_blind = decale;
            rentrer_dans_une_blind(&mut app);

            assert_eq!(
                app.world().resource::<BlindContext>().hands_remaining,
                decale,
                "gobelet {id:?}, configuration décalée"
            );
        }
    }

    #[test]
    fn test_current_score_starts_at_zero() {
        let app = app_en_run(CupId::Standard);
        let manche = app.world().resource::<BlindContext>();

        assert_eq!(manche.current_score, 0);
        assert!(manche.target_score > 0);
    }

    #[test]
    fn test_target_follows_the_ante_curve() {
        // **La courbe vit désormais dans le moteur.** Ce test ne la recalcule
        // plus : il vérifie que la manche montée en porte la valeur, ce qui est
        // la seule chose que cette crate décide encore.
        //
        // Le commentaire d'origine disait : « Arrondi au plus proche, jamais
        // troncature. Aucune valeur de la table actuelle ne le met en évidence
        // — 300 x 1,6 tombe juste à chaque étage. » **C'était faux** : à
        // l'ante 7, 5033,6 ne tombe pas juste, et c'est précisément là que la
        // courbe locale divergeait de la normative. La phrase avait été écrite
        // quand la table n'était vérifiée que jusqu'à l'ante 2.
        let mut app = app_en_run(CupId::Standard);
        assert_eq!(app.world().resource::<BlindContext>().target_score, 300);

        for (ante, attendu) in [(2u8, 480u64), (7, 5033), (8, 8053)] {
            app.world_mut().resource_mut::<RunSession>().ante = ante;
            rentrer_dans_une_blind(&mut app);
            assert_eq!(
                app.world().resource::<BlindContext>().target_score,
                attendu,
                "ante {ante}"
            );
        }

        // Le rang de la blind multiplie la cible. `current_blind_definition`
        // ne rend qu'un petit blind à cette étape : la comparaison passe donc
        // par la fonction de courbe, seule à voir les trois rangs.
        let partie = session(CupId::Standard, 0);
        let petit = BlindDefinition {
            kind: BlindType::Small,
            ..BlindDefinition::default()
        };
        let boss = BlindDefinition {
            kind: BlindType::Boss,
            ..BlindDefinition::default()
        };
        assert_eq!(
            target_score(&partie, &boss),
            target_score(&partie, &petit).saturating_mul(2)
        );
    }

    #[test]
    fn test_victory_requires_the_three_conditions() {
        // Contre-épreuves de `test_victory_skips_blind_setup` : chacune retire
        // une condition et une seule. Sans elles, supprimer n'importe laquelle
        // des trois passerait inaperçu, le cas nominal les satisfaisant toutes.
        for (kind, ante, atteint, motif) in [
            (
                BlindType::Boss,
                1,
                true,
                "Mise Boss d'un ante intermédiaire",
            ),
            (
                BlindType::Small,
                FINAL_ANTE,
                true,
                "petit blind de l'ante final",
            ),
            (
                BlindType::Boss,
                FINAL_ANTE,
                false,
                "Mise Boss de l'ante final, cible non atteinte",
            ),
        ] {
            let mut app = app_en_run(CupId::Standard);
            arme_et_rentre(&mut app, kind, ante, atteint);

            assert!(
                matches!(
                    *app.world().resource::<NextState<AppState>>(),
                    NextState::Unchanged
                ),
                "{motif} : la victoire a été déclarée à tort"
            );
        }
    }

    #[test]
    fn test_victory_skips_blind_setup() {
        let mut app = app_en_run(CupId::Standard);

        // Témoin : ce marquage ne survivrait pas à la construction d'un
        // nouveau contexte.
        app.world_mut()
            .resource_mut::<BlindContext>()
            .used_hands
            .mark(YahtzeeHand::Yahtzee);
        arme_et_rentre(&mut app, BlindType::Boss, FINAL_ANTE, true);

        assert!(
            matches!(
                *app.world().resource::<NextState<AppState>>(),
                NextState::PendingIfNeq(AppState::Victory)
            ),
            "la victoire n'est pas en attente"
        );
        assert!(
            app.world()
                .resource::<BlindContext>()
                .used_hands
                .contains(YahtzeeHand::Yahtzee),
            "une manche a été montée pour une run déjà gagnée"
        );
    }
}

#[cfg(test)]
mod tests_relances_et_valeurs_forcees {
    use super::*;
    use core_engine::cups::CupId;
    use core_engine::relics::{RelicId, RelicState};

    use crate::systems::fixtures::{
        app_a_la_graine, des_tries, entrer_dans_roll, frapper, inventaire, session, valeurs_des,
    };

    fn blind_nue() -> BlindDefinition {
        BlindDefinition::default()
    }

    fn blind_plafonnee(cap: u8) -> BlindDefinition {
        BlindDefinition {
            modifier: Some(BlindModifier::MaxRerolls(cap)),
            ..BlindDefinition::default()
        }
    }

    fn stock(id: CupId, defs: &[RelicId]) -> RelicInventory {
        let partie = session(id, 0);
        let mut stock = inventaire(&partie.config);
        for def in defs {
            stock.add_relic(*def).expect("slot libre");
        }
        stock
    }

    #[test]
    fn test_obsidian_reduces_rerolls_in_chain() {
        let partie = session(CupId::Standard, 0);
        assert_eq!(
            partie.config.base_rerolls, 2,
            "le gobelet standard part de deux"
        );

        let avec = stock(CupId::Standard, &[RelicId::UnstableObsidian]);
        assert_eq!(resolve_rerolls(&partie, &blind_nue(), &avec), 1);
    }

    #[test]
    fn test_obsidian_underflow_still_saturates() {
        // Gobelet Abandonné, une Obsidienne, et L'Étau par-dessus : zéro, jamais
        // 255. La saturation est celle d'`apply_delta`, inchangée.
        let partie = session(CupId::Abandoned, 0);
        let avec = stock(CupId::Abandoned, &[RelicId::UnstableObsidian]);

        assert_eq!(resolve_rerolls(&partie, &blind_plafonnee(1), &avec), 0);
    }

    #[test]
    fn test_two_obsidians_stack() {
        let partie = session(CupId::Standard, 0);
        let deux = stock(
            CupId::Standard,
            &[RelicId::UnstableObsidian, RelicId::UnstableObsidian],
        );

        assert_eq!(resolve_rerolls(&partie, &blind_nue(), &deux), 0);
    }

    #[test]
    fn test_disabled_relic_contributes_nothing() {
        // Une relique éteinte perd ses effets **et** son malus : c'est tout
        // l'objet du prédicat partagé. Sans lui, le joueur subirait la
        // contrepartie sans le bonus.
        let partie = session(CupId::Standard, 0);
        let mut eteinte = stock(CupId::Standard, &[RelicId::UnstableObsidian]);
        eteinte.slots[0].as_mut().expect("relique au slot 0").state = RelicState::Disabled;

        assert_eq!(resolve_rerolls(&partie, &blind_nue(), &eteinte), 2);
    }

    #[test]
    fn test_positive_relic_delta_survives_the_aggregate() {
        // **Aucune relique de l'Étape 5 n'a de delta positif**, et c'est
        // précisément pourquoi ce test existe : un agrégat qui passerait par un
        // `u8` avalerait le premier que l'Étape 9 apportera, sans erreur.
        // L'agrégat est éprouvé directement, la chaîne étant déjà couverte.
        // À travers l'inventaire, un agrégat qui forcerait le signe rendrait
        // les mêmes valeurs qu'un agrégat correct : aucune relique n'a de delta
        // positif. C'est donc l'arithmétique qu'on éprouve, seule à pouvoir en
        // recevoir un.
        assert_eq!(agreger_deltas([1i8, 2].into_iter()), 3);
        assert_eq!(agreger_deltas([-1i8, -1].into_iter()), -2);
        assert_eq!(agreger_deltas([3i8, -1].into_iter()), 2);
        assert_eq!(agreger_deltas(std::iter::empty()), 0);
        assert_eq!(agreger_deltas(std::iter::repeat_n(-1i8, 300)), i8::MIN);
        assert_eq!(agreger_deltas(std::iter::repeat_n(1i8, 300)), i8::MAX);

        // Et la relique réelle traverse bien l'inventaire.
        assert_eq!(
            relic_reroll_malus(
                &stock(CupId::Standard, &[RelicId::UnstableObsidian]),
                &BlindDefinition::default(),
            ),
            -1
        );
    }

    // ---- TASK-75 : *La Fissure* ----

    /// Manche cachant `n` dés.
    fn manche_fissuree(n: u8) -> BlindDefinition {
        BlindDefinition {
            modifier: Some(BlindModifier::HideDice(n)),
            ..BlindDefinition::default()
        }
    }

    fn masques(app: &mut App) -> Vec<DieId> {
        let mut q = app
            .world_mut()
            .query_filtered::<&Die, With<crate::components::Hidden>>();
        let mut v: Vec<DieId> = q.iter(app.world()).map(|die| die.id).collect();
        v.sort_unstable();
        v
    }

    /// Entre dans une main sous la manche donnée.
    fn main_sous(app: &mut App, blind: BlindDefinition) {
        app.world_mut().resource_mut::<BlindContext>().blind = blind;
        entrer_dans_roll(app);
        app.update();
    }

    #[test]
    fn test_hidden_dice_are_reproducible() {
        let cacher = |graine: u64| {
            let mut app = app_a_la_graine(CupId::Standard, graine);
            main_sous(&mut app, manche_fissuree(2));
            masques(&mut app)
        };

        let gauche = cacher(11);
        assert_eq!(gauche.len(), 2, "deux dés cachés, distincts");
        assert_eq!(gauche, cacher(11), "même graine, mêmes dés");
        // **Graine fixée, assertion sur ce qu'elle produit.** Sans les valeurs
        // en clair, un mélange changé de forme rend d'autres dés et reste
        // reproductible : le test passerait sans rien garder.
        assert_eq!(gauche, vec![DieId(2), DieId(4)]);

        // Contre-épreuve : sans elle, un tirage constant passerait.
        let mut autre = (12..40).map(cacher).filter(|v| *v != gauche);
        assert!(
            autre.next().is_some(),
            "aucune graine ne cache d'autres dés : le tirage est constant"
        );
    }

    #[test]
    fn test_hidden_draw_does_not_shift_the_dice_stream() {
        // **Le motif du flux séparé.** Tirer les dés cachés sur `rng.dice`
        // décalerait la séquence des lancers selon la présence du boss : deux
        // joueurs de même graine n'auraient plus les mêmes dés dès la première
        // Mise Boss. Sans ce test, un tirage sur le mauvais flux passe au vert.
        let valeurs = |blind: BlindDefinition| {
            let mut app = app_a_la_graine(CupId::Standard, 11);
            main_sous(&mut app, blind);
            valeurs_des(&mut app)
        };

        assert_eq!(
            valeurs(manche_fissuree(2)),
            valeurs(BlindDefinition::default()),
            "le boss a décalé la séquence des dés"
        );
    }

    #[test]
    fn test_reroll_keeps_hidden_markers() {
        let mut app = app_a_la_graine(CupId::Standard, 11);
        main_sous(&mut app, manche_fissuree(2));
        let avant = masques(&mut app);
        assert_eq!(avant.len(), 2);

        // Une relance : les valeurs changent, les marqueurs restent.
        let valeurs = valeurs_des(&mut app);
        frapper(&mut app, KeyCode::Space);
        app.update();
        assert_ne!(valeurs_des(&mut app), valeurs, "la relance n'a rien roulé");

        assert_eq!(masques(&mut app), avant, "un marqueur a bougé à la relance");
    }

    #[test]
    fn test_hidden_cleared_between_blinds() {
        let mut app = app_a_la_graine(CupId::Standard, 11);
        main_sous(&mut app, manche_fissuree(2));
        assert_eq!(masques(&mut app).len(), 2);

        // Manche suivante, sans boss : aucun marqueur ne survit.
        main_sous(&mut app, BlindDefinition::default());
        assert!(
            masques(&mut app).is_empty(),
            "un marqueur a survécu au boss"
        );
        assert!(
            !app.world()
                .resource::<HandContext>()
                .active_evaluations
                .is_empty(),
            "la liste est restée vide sans raison visible"
        );
    }

    #[test]
    fn test_hidden_ranks_draw_without_replacement() {
        // Le tirage est pur : il s'éprouve sans monde Bevy.
        let mut rng = core_engine::rng::RunRng::from_seed(3);

        for (demande, total, attendu) in [(2_u8, 5_usize, 2_usize), (9, 5, 5), (2, 0, 0), (0, 5, 0)]
        {
            let rangs = core_engine::blinds::hidden_ranks(demande, total, &mut rng.boss);
            assert_eq!(rangs.len(), attendu, "{demande} sur {total}");
            assert!(rangs.iter().all(|r| *r < total), "rang hors bornes");

            let mut uniques = rangs.clone();
            uniques.dedup();
            assert_eq!(uniques, rangs, "un rang est tiré deux fois");
        }

        // **Une graine qui sépare les deux formes de mélange.** Un mélange
        // partiel et un mélange avec remise rendent tous deux des rangs
        // distincts, le tableau restant une permutation : seule la valeur
        // tirée les distingue. Mesuré, la graine 11 les confond et la 4 les
        // sépare ; c'est donc celle-ci qu'il faut épingler.
        let mut graine_4 = core_engine::rng::RunRng::from_seed(4);
        assert_eq!(
            core_engine::blinds::hidden_ranks(2, 5, &mut graine_4.boss),
            vec![2, 4]
        );
    }

    // ---- TASK-74 : la cage vaut pour tous les hooks ----

    /// Définition mettant un slot en cage.
    fn manche_en_cage(slot: u8) -> BlindDefinition {
        BlindDefinition {
            modifier: Some(BlindModifier::DisableRelicSlot(slot)),
            ..BlindDefinition::default()
        }
    }

    #[test]
    fn test_caged_relic_loses_all_hooks() {
        // **Le point du ticket.** Sans cette règle, une *Obsidienne Instable*
        // en cage perdrait son `MultiplyMult(200)` tout en gardant son
        // `reroll_delta: -1` : le joueur subirait la contrepartie sans le
        // bonus, sur un boss dont le contrat est de neutraliser la relique.
        let partie = session(CupId::Standard, 0);
        let stock = stock(CupId::Standard, &[RelicId::UnstableObsidian]);

        assert_eq!(
            relic_reroll_malus(&stock, &BlindDefinition::default()),
            -1,
            "l'Obsidienne n'imposait déjà rien : le montage ne prouve rien"
        );
        assert_eq!(
            resolve_rerolls(&partie, &BlindDefinition::default(), &stock),
            1,
            "deux relances moins celle de l'Obsidienne"
        );

        // En cage, le malus disparaît avec le bonus.
        assert_eq!(relic_reroll_malus(&stock, &manche_en_cage(0)), 0);
        assert_eq!(resolve_rerolls(&partie, &manche_en_cage(0), &stock), 2);

        // Et une cage sur un autre slot ne la touche pas.
        assert_eq!(resolve_rerolls(&partie, &manche_en_cage(1), &stock), 1);
    }

    #[test]
    fn test_caged_ghost_die_does_not_force() {
        // **Le cinquième parcours, que le ticket ne nommait pas.**
        // `apply_forced_values` consultait la neutralisation sans connaître la
        // manche : un *Dé Fantôme* en cage forçait encore son six.
        let mut app = app_a_la_graine(CupId::Standard, GRAINE_SANS_UN);
        app.world_mut()
            .resource_mut::<RelicInventory>()
            .add_relic(RelicId::GhostDie)
            .expect("slot libre");
        entrer_dans_roll(&mut app);
        app.update();

        let force: Vec<u8> = valeurs_des(&mut app);
        assert!(
            force.contains(&6),
            "le Dé Fantôme ne forçait déjà rien : le montage ne prouve rien"
        );

        // La même graine, la même relique, mais le slot en cage.
        let mut en_cage = app_a_la_graine(CupId::Standard, GRAINE_SANS_UN);
        en_cage
            .world_mut()
            .resource_mut::<RelicInventory>()
            .add_relic(RelicId::GhostDie)
            .expect("slot libre");
        en_cage
            .world_mut()
            .resource_mut::<BlindContext>()
            .blind
            .modifier = Some(BlindModifier::DisableRelicSlot(0));
        entrer_dans_roll(&mut en_cage);
        en_cage.update();

        assert_ne!(
            valeurs_des(&mut en_cage),
            force,
            "le Dé Fantôme en cage a quand même forcé sa valeur"
        );
    }

    /// Graine dont le premier jet standard ne contient aucun 1.
    const GRAINE_SANS_UN: u64 = 6;

    #[test]
    fn test_ghost_die_forces_value_after_first_roll() {
        // Le test ne choisit pas les faces : `setup_round` les tire du flux
        // graine. Ce qui se garde ici est le **câblage** — la logique de la
        // relique a ses cinq tests à TASK-61.
        let valeurs = |graine: u64, avec_relique: bool| {
            let mut app = app_a_la_graine(CupId::Standard, graine);
            if avec_relique {
                app.world_mut()
                    .resource_mut::<RelicInventory>()
                    .add_relic(RelicId::GhostDie)
                    .expect("slot libre");
            }
            entrer_dans_roll(&mut app);
            des_tries(&mut app)
                .into_iter()
                .map(|(_, de, _)| de.current_value)
                .collect::<Vec<u8>>()
        };

        // **La graine est choisie pour que le jet ne contienne aucun 1.** Sans
        // cela le test prend la branche muette de la relique et ne mesure rien
        // — mesuré : la graine 7 sort `[3, 3, 4, 4, 1]`. L'assertion sur le jet
        // nu verrouille ce choix : si le flux change, le test rougit au lieu de
        // devenir vide.
        let nu = valeurs(GRAINE_SANS_UN, false);
        assert!(!nu.contains(&1), "graine devenue inutilisable : {nu:?}");

        let force = valeurs(GRAINE_SANS_UN, true);
        let plus_faible = nu.iter().copied().min().expect("au moins un dé");
        let rang = nu
            .iter()
            .position(|valeur| *valeur == plus_faible)
            .expect("le minimum est dans la liste");

        assert_eq!(force[rang], 6, "le dé le plus faible n'a pas été relevé");
        for (index, (avant, apres)) in nu.iter().zip(force.iter()).enumerate() {
            if index != rang {
                assert_eq!(avant, apres, "le dé {index} a bougé sans raison");
            }
        }
    }

    #[test]
    fn test_force_values_address_by_identity_not_by_position() {
        // **Le seul montage qui sépare les deux.** `setup_round` spawne les dés
        // avec `DieId(k)` à l'indice `k` : identité et position y coïncident,
        // et un `get_mut(id)` survit à tous les autres tests. Ici les
        // identifiants sont désordonnés, et aucun ne vaut son rang.
        let mut monde = World::new();
        for (id, valeur) in [(2u32, 3u8), (0, 4), (1, 5)] {
            let mut de = Die::new(DieId(id), 6);
            de.current_value = valeur;
            monde.spawn(de);
        }

        {
            let mut requete = monde.query::<&mut Die>();
            let mut des: Vec<Mut<Die>> = requete.iter_mut(&mut monde).collect();
            appliquer_valeurs_forcees(&mut des, &[(DieId(0), 6)]);
        }

        let mut requete = monde.query::<&Die>();
        let mut vus: Vec<(u32, u8)> = requete
            .iter(&monde)
            .map(|de| (de.id.0, de.current_value))
            .collect();
        vus.sort_unstable();

        assert_eq!(
            vus,
            vec![(0, 6), (1, 5), (2, 3)],
            "adressé par rang : c'est le dé d'identifiant 2 qui aurait bougé"
        );
    }

    #[test]
    fn test_force_values_ignores_unknown_die_id() {
        // Un identifiant absent est ignoré sans panique. Le montage passe par
        // l'application directe, le seul moyen de viser un dé qui n'existe pas.
        let mut app = app_a_la_graine(CupId::Standard, 3);
        entrer_dans_roll(&mut app);
        let avant: Vec<u8> = des_tries(&mut app)
            .into_iter()
            .map(|(_, de, _)| de.current_value)
            .collect();

        let mut requete = app.world_mut().query::<&mut Die>();
        let mut des: Vec<Mut<Die>> = requete.iter_mut(app.world_mut()).collect();
        appliquer_valeurs_forcees(&mut des, &[(DieId(9_999), 6)]);
        drop(des);

        let apres: Vec<u8> = des_tries(&mut app)
            .into_iter()
            .map(|(_, de, _)| de.current_value)
            .collect();
        assert_eq!(apres, avant);
    }

    #[test]
    fn test_force_values_consumes_no_rng() {
        // Le flux de dés doit être au même point avec et sans la relique :
        // `force_values` est déterministe, et un tirage ici ferait diverger
        // deux runs de même graine.
        let etat = |avec_relique: bool| {
            let mut app = app_a_la_graine(CupId::Standard, 11);
            if avec_relique {
                app.world_mut()
                    .resource_mut::<RelicInventory>()
                    .add_relic(RelicId::GhostDie)
                    .expect("slot libre");
            }
            entrer_dans_roll(&mut app);
            // L'état du flux se lit en le faisant produire : quatre dés
            // d'appoint roulés depuis le point où `setup_round` l'a laissé.
            let mut session = app.world_mut().resource_mut::<RunSession>();
            (0..4)
                .map(|_| {
                    let mut temoin = Die::new(DieId(0), 6);
                    temoin.roll(&mut session.rng.dice, false);
                    temoin.current_value
                })
                .collect::<Vec<u8>>()
        };

        assert_eq!(etat(false), etat(true));
    }
}
