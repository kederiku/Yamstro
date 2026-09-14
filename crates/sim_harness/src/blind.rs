//! Assemblage d'une manche, réimplémenté depuis l'API publique du moteur.
//!
//! **Le moteur n'expose aucune fonction qui assemble une définition de manche.**
//! Il donne le type, la courbe, le catalogue des boss et leur tirage ;
//! l'assemblage, lui, vit dans un système de la crate d'états, que le harnais
//! ne peut pas appeler. Il réassemble donc tout, littéraux de récompense
//! compris — troisième ligne du tableau des six réimplémentations autorisées,
//! et une réimplémentation de la crate d'états, jamais du moteur.
//!
//! Le manque est consigné dans `MISSING_API.md`. Le jour où le moteur livre la
//! fonction souhaitée, [`blind_definition`] disparaît d'une seule pièce : elle
//! en a exactement la forme.
//!
//! # L'ordre d'assemblage est porté par les types, pas par une consigne
//!
//! Le modificateur se tire **avant** que la cible se calcule, la courbe le
//! prenant en cinquième facteur. Ici [`blind_definition`] reçoit le boss en
//! paramètre : calculer une cible de Mise Boss sans lui **ne compile pas**. La
//! règle cesse d'être une phrase qu'on peut oublier de lire.
//!
//! # Ce fichier n'écrit jamais les deux nombres de la courbe
//!
//! Ni en constante, ni en valeur attendue, ni en commentaire : la base de
//! l'ante un et la croissance par ante vivent à un seul endroit du dépôt, et
//! les valeurs attendues des tests sont ce que **la courbe rend**, jamais ce
//! qu'on aurait recalculé ici.

use core_engine::blinds::definitions::{BossDefinition, boss_definition, draw_boss};
use core_engine::blinds::{BlindContext, BlindDefinition, BlindType, target_score};
use core_engine::config::RunConfig;
use core_engine::cups::CupId;
use core_engine::hands::HandGrid;
use rand_chacha::ChaCha8Rng;

/// La récompense d'une manche, en dollars.
///
/// **Écrite ici et lue au gain, jamais recalculée.** Deux sites de calcul
/// donneraient deux économies différentes le jour où l'Étape 10 la modifiera,
/// et l'écart se lirait comme un défaut d'équilibrage.
#[must_use]
pub fn reward_for(kind: BlindType) -> u32 {
    match kind {
        BlindType::Small => 3,
        BlindType::Big => 4,
        BlindType::Boss => 5,
    }
}

/// Tire le boss d'une manche. **Seul endroit du harnais qui consomme de
/// l'aléa de manche, et seul emprunt mutable de ce module.**
///
/// Une Petite ou une Grosse Mise ne tire **rien** : un tirage inconditionnel
/// « pour simplifier » ferait avancer le flux des boss trois fois par ante au
/// lieu d'une, et la même graine ne produirait plus la même suite de boss que
/// le jeu — le fixture d'accord tomberait sans qu'on sache pourquoi.
#[must_use]
pub fn draw_boss_for(
    kind: BlindType,
    relic_capacity: u8,
    rng: &mut ChaCha8Rng,
) -> Option<BossDefinition> {
    if kind != BlindType::Boss {
        return None;
    }
    let id = draw_boss(rng);
    Some(boss_definition(id, rng, relic_capacity))
}

/// La définition d'une manche. **Fonction pure**, de la forme exacte que
/// `MISSING_API.md` demande au moteur.
///
/// Le boss lui arrive **déjà tiré** : c'est ce qui rend l'ordre mécanique, et
/// ce qui garde cette fonction sans aléa, donc rejouable à volonté.
#[must_use]
pub fn blind_definition(
    ante: u8,
    kind: BlindType,
    cup: CupId,
    stake_level: u8,
    boss: Option<BossDefinition>,
) -> BlindDefinition {
    let modifier = boss.map(|def| def.modifier);
    BlindDefinition {
        kind,
        // La cible est **lue** de la courbe du moteur, jamais recalculée : une
        // seconde courbe divergerait au premier calibrage, et c'est ce
        // calibrage que l'étape existe pour produire.
        target_score: target_score(ante, kind, cup, stake_level, modifier.as_ref()),
        reward: reward_for(kind),
        modifier,
    }
}

/// L'enveloppe de manche, construite **par littéral** : le type ne dérive pas
/// `Default`, et ses cinq champs sont publics.
///
/// Aucun littéral de jouabilité : le nombre de mains vient de la
/// configuration, jamais d'un chiffre écrit à la main.
#[must_use]
pub fn blind_context(definition: BlindDefinition, config: &RunConfig) -> BlindContext {
    BlindContext {
        target_score: definition.target_score,
        blind: definition,
        current_score: 0,
        hands_remaining: config.hands_per_blind,
        used_hands: HandGrid::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::blinds::BlindModifier;
    use core_engine::cups::definitions::cup;
    use core_engine::rng::RunRng;
    use rand_chacha::rand_core::Rng;

    fn config_neutre() -> RunConfig {
        RunConfig::from_cup(&cup(CupId::Standard))
    }

    /// Observe un flux **sans le consommer** : le clone tire, l'original non.
    fn empreinte(flux: &ChaCha8Rng) -> Vec<u64> {
        let mut clone = flux.clone();
        (0..4).map(|_| clone.next_u64()).collect()
    }

    fn empreintes(rng: &RunRng) -> [Vec<u64>; 4] {
        [
            empreinte(&rng.dice),
            empreinte(&rng.shop),
            empreinte(&rng.boss),
            empreinte(&rng.relic_effects),
        ]
    }

    #[test]
    fn test_blind_definition_matches_setup_blind() {
        // Les mêmes valeurs que `test_base_target_progression` (TASK-69) et
        // `test_next_blind_target_is_correct` (TASK-80). Ce sont des attendus
        // produits par un appel au moteur, jamais des constantes d'ici.
        for (kind, attendu) in [
            (BlindType::Small, 8053),
            (BlindType::Big, 12080),
            (BlindType::Boss, 16106),
        ] {
            let def = blind_definition(8, kind, CupId::Standard, 1, None);
            assert_eq!(def.target_score, attendu, "{kind:?}");
            assert_eq!(def.kind, kind);
            assert_eq!(def.modifier, None);
        }

        assert_eq!(
            blind_definition(2, BlindType::Big, CupId::Standard, 1, None).target_score,
            720
        );
    }

    #[test]
    fn test_reward_is_three_four_five() {
        for (kind, attendu) in [
            (BlindType::Small, 3),
            (BlindType::Big, 4),
            (BlindType::Boss, 5),
        ] {
            assert_eq!(
                blind_definition(1, kind, CupId::Standard, 1, None).reward,
                attendu,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn test_boss_blind_draws_from_boss_stream_only() {
        let mut rng = RunRng::from_seed(7);
        let avant = empreintes(&rng);

        let boss = draw_boss_for(
            BlindType::Boss,
            config_neutre().relic_capacity,
            &mut rng.boss,
        );
        assert!(boss.is_some(), "une Mise Boss porte un boss");

        let apres = empreintes(&rng);
        assert_eq!(avant[0], apres[0], "le flux des dés a bougé");
        assert_eq!(avant[1], apres[1], "le flux de boutique a bougé");
        assert_eq!(avant[3], apres[3], "le flux d'effets a bougé");
        // **C'est cette ligne qui rend le test non vacuous** : sans elle, un
        // assemblage qui ne tirerait rien du tout passerait au vert.
        assert_ne!(avant[2], apres[2], "le flux des boss n'a pas avancé");
    }

    #[test]
    fn test_small_and_big_draw_nothing() {
        for kind in [BlindType::Small, BlindType::Big] {
            let mut rng = RunRng::from_seed(7);
            let avant = empreintes(&rng);

            let boss = draw_boss_for(kind, config_neutre().relic_capacity, &mut rng.boss);
            assert_eq!(boss, None, "{kind:?} ne porte pas de boss");
            assert_eq!(empreintes(&rng), avant, "{kind:?} a consommé de l'aléa");
        }
    }

    #[test]
    fn test_boss_modifier_reaches_the_target() {
        // *Le Mur* triple la cible. Le modificateur doit atteindre le calcul :
        // c'est la dépendance d'ordre que le paramètre `boss` rend mécanique.
        let mur = BossDefinition {
            id: core_engine::blinds::definitions::BossId::ALL[0],
            modifier: BlindModifier::TargetMultiplier(3_000),
        };
        let neutre = blind_definition(1, BlindType::Boss, CupId::Standard, 1, None);
        let avec = blind_definition(1, BlindType::Boss, CupId::Standard, 1, Some(mur));

        assert_eq!(neutre.target_score, 600);
        assert_eq!(
            avec.target_score, 1800,
            "le modificateur n'est pas entré dans la cible"
        );
        assert_eq!(avec.modifier, Some(BlindModifier::TargetMultiplier(3_000)));
    }

    #[test]
    fn test_blind_context_starts_empty() {
        let config = config_neutre();
        let def = blind_definition(3, BlindType::Small, CupId::Standard, 1, None);
        let cible = def.target_score;
        let contexte = blind_context(def, &config);

        assert_eq!(contexte.current_score, 0);
        assert!(contexte.used_hands.is_empty());
        assert_eq!(contexte.hands_remaining, config.hands_per_blind);

        // **Cette moitié seule discrimine, et elle exige une configuration
        // qu'aucun gobelet ne produit.** Les cinq gobelets portent le même
        // nombre de mains : un littéral quatre passerait les cinq, et le banc
        // l'a montré survivant jusqu'à ce que ces trois lignes existent. Même
        // technique que le troisième cas du plafond d'intérêts (TASK-70).
        let mut inhabituelle = config_neutre();
        inhabituelle.hands_per_blind = 7;
        let def = blind_definition(3, BlindType::Small, CupId::Standard, 1, None);
        assert_eq!(
            blind_context(def, &inhabituelle).hands_remaining,
            7,
            "le nombre de mains vient de la configuration, pas d'un littéral"
        );
        assert_eq!(contexte.target_score, cible, "la cible suit la définition");
        assert_eq!(contexte.blind.target_score, cible);
    }

    #[test]
    fn test_missing_api_has_the_three_entries() {
        let chemin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("MISSING_API.md");
        let contenu = std::fs::read_to_string(&chemin)
            .unwrap_or_else(|erreur| panic!("{} : {erreur}", chemin.display()));

        for attendu in [
            "fn blind_definition(",
            "mult_permille",
            "PartialOrd, Ord",
            "CupId",
        ] {
            assert!(contenu.contains(attendu), "l'entrée « {attendu} » manque");
        }
        // Quatre entrées, quatre propriétaires : 1 pour le catalogue de
        // gobelets, 6 pour l'assemblage et la décomposition, 2 pour l'ordre.
        assert_eq!(contenu.matches("Étape propriétaire").count(), 4);
        for proprietaire in [
            "Étape propriétaire : 1",
            "Étape propriétaire : 6",
            "Étape propriétaire : 2",
        ] {
            assert!(contenu.contains(proprietaire), "{proprietaire} manque");
        }
    }
}
