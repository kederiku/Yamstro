//! Le gain de fin de manche, **vu du dehors**.
//!
//! Tests purs : ni ECS, ni ressource, ni Bevy. `calculate_payout` ne connaît
//! que ses cinq paramètres, et c'est tout ce qu'il faut pour l'éprouver.

use core_engine::blinds::{BlindDefinition, BlindType};
use core_engine::config::RunConfig;
use core_engine::cups::CupId;
use core_engine::cups::definitions::cup;
use core_engine::economy::{INTEREST_TRANCHE, Payout, calculate_payout};

fn config_avec_plafond(max_interest: u32) -> RunConfig {
    RunConfig {
        max_interest,
        ..RunConfig::from_cup(&cup(CupId::Standard))
    }
}

fn manche(kind: BlindType, reward: u32) -> BlindDefinition {
    BlindDefinition {
        kind,
        target_score: 300,
        reward,
        modifier: None,
    }
}

#[test]
fn test_payout_interest_capped_by_config() {
    // **Le troisième cas est le seul qui discrimine.** Diviser par
    // `config.max_interest` ou plafonner par `INTEREST_TRANCHE` donne bien 4 et
    // 5 sur les deux premiers ; seul celui-ci sépare la constante de règle du
    // paramètre de run.
    //
    // Et il exige une configuration qu'**aucun gobelet ne produit** : les cinq
    // valent `max_interest: 5`, et le 10 vient d'une relique de l'Étape 9 qui
    // n'existe pas. Dans une partie réelle aujourd'hui, `min(max_interest)` et
    // `min(5)` sont donc indiscernables — ce test est la seule chose qui les
    // sépare, et sans lui la relique naîtrait silencieusement inerte.
    let boss = manche(BlindType::Boss, 5);

    for (or, plafond, attendu) in [(23u32, 5u32, 4u32), (63, 5, 5), (63, 10, 10)] {
        let payout = calculate_payout(&boss, 0, or, &config_avec_plafond(plafond), 0);
        assert_eq!(payout.interest, attendu, "or {or}, plafond {plafond}");
    }

    assert_eq!(INTEREST_TRANCHE, 5, "la tranche est une règle du jeu");
}

#[test]
fn test_payout_unused_hands_bonus() {
    // La décomposition, pas seulement le total : un terme faux qui compense un
    // autre passerait sur la seule somme.
    let boss = manche(BlindType::Boss, 5);
    let config = config_avec_plafond(5);

    let payout = calculate_payout(&boss, 2, 23, &config, 3);
    assert_eq!(
        payout,
        Payout {
            blind_reward: 5,
            unused_hands: 2,
            interest: 4,
            relic_gold: 3,
            total: 14,
        }
    );

    // **`gold = u32::MAX - 1` ne peut pas déborder** : une division ne déborde
    // pas, et le plafond ramène le résultat à `max_interest`. Le cas prouve que
    // le plafond tient à l'extrême, rien de plus.
    let extreme = calculate_payout(&boss, 2, u32::MAX - 1, &config, 0);
    assert_eq!(extreme.interest, config.max_interest);
    assert_eq!(extreme.total, 5 + 2 + 5);

    // **Le débordement réel est ailleurs** : c'est le total, quand l'or des
    // reliques est grand. C'est là que la saturation existe.
    let sature = calculate_payout(&boss, 2, 23, &config, u32::MAX);
    assert_eq!(sature.relic_gold, u32::MAX);
    assert_eq!(sature.total, u32::MAX);
}

#[test]
fn test_blind_reward_is_read_not_recomputed() {
    // **Le seul montage qui sépare la lecture du recalcul.** Partout ailleurs
    // les fixtures donnent à `reward` la valeur canonique de son rang — 3, 4,
    // 5 —, si bien qu'un `match blind.kind` rendrait exactement la même chose
    // et survivrait à tous les tests. Mesuré.
    //
    // Le cas discriminant est celui que le Stake 2 de l'Étape 10 produira :
    // une Petite Mise dont la récompense est forcée à zéro. Un recalcul local
    // la contredirait sans erreur de compilation, et le joueur toucherait
    // trois dollars que la règle lui retire.
    let config = config_avec_plafond(5);

    let petite_muette = calculate_payout(&manche(BlindType::Small, 0), 0, 0, &config, 0);
    assert_eq!(petite_muette.blind_reward, 0);
    assert_eq!(petite_muette.total, 0);

    // Et l'inverse : une récompense qu'aucun rang ne porte.
    let genereuse = calculate_payout(&manche(BlindType::Big, 11), 0, 0, &config, 0);
    assert_eq!(genereuse.blind_reward, 11);
}

#[test]
fn test_total_is_the_sum_of_four_terms() {
    let config = config_avec_plafond(5);
    for (kind, reward, mains, or, relique) in [
        (BlindType::Small, 3u32, 4u8, 0u32, 0u32),
        (BlindType::Big, 4, 1, 17, 2),
        (BlindType::Boss, 5, 0, 63, 9),
        (BlindType::Boss, 5, 3, 4, 100_000),
    ] {
        let payout = calculate_payout(&manche(kind, reward), mains, or, &config, relique);
        assert_eq!(
            payout.total,
            payout
                .blind_reward
                .saturating_add(payout.unused_hands)
                .saturating_add(payout.interest)
                .saturating_add(payout.relic_gold),
            "{kind:?}"
        );
    }
}

#[test]
fn test_zero_relic_gold_changes_nothing() {
    let boss = manche(BlindType::Boss, 5);
    let config = config_avec_plafond(5);

    let sans = calculate_payout(&boss, 2, 23, &config, 0);
    let avec = calculate_payout(&boss, 2, 23, &config, 7);

    assert_eq!(sans.blind_reward, avec.blind_reward);
    assert_eq!(sans.unused_hands, avec.unused_hands);
    assert_eq!(sans.interest, avec.interest);
    assert_eq!(avec.relic_gold, sans.relic_gold + 7);
    assert_eq!(avec.total, sans.total + 7);
}

#[test]
fn test_zero_hands_remaining_is_no_bonus() {
    // Un bonus **nul**, jamais une soustraction : une blind battue sur la
    // dernière main ne coûte rien, elle ne rapporte simplement pas le bonus.
    let boss = manche(BlindType::Boss, 5);
    let config = config_avec_plafond(5);

    let payout = calculate_payout(&boss, 0, 0, &config, 0);
    assert_eq!(payout.unused_hands, 0);
    assert_eq!(payout.total, 5, "la récompense de manche reste entière");
}

#[test]
fn test_interest_depends_only_on_gold_and_cap() {
    // **Ce que `test_payout_is_pure` visait, en testable.** « La fonction n'a
    // rien écrit dans ses arguments » n'est pas exprimable : deux références
    // partagées et trois scalaires `Copy`, écrire dedans ne compile pas. Ce qui
    // peut échouer, c'est qu'un autre paramètre fuie dans le calcul des
    // intérêts.
    let config = config_avec_plafond(5);
    let reference = calculate_payout(&manche(BlindType::Boss, 5), 2, 23, &config, 3).interest;

    for variante in [
        calculate_payout(&manche(BlindType::Small, 0), 2, 23, &config, 3),
        calculate_payout(&manche(BlindType::Boss, 5), 0, 23, &config, 3),
        calculate_payout(&manche(BlindType::Boss, 5), 2, 23, &config, 999),
    ] {
        assert_eq!(variante.interest, reference);
    }
}
