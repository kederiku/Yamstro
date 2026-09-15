//! Les six boss inauguraux, et rien d'autre.
//!
//! # Ce fichier traduit, il n'implémente pas
//!
//! Un boss y devient une contrainte, un point c'est tout. Le filtrage d'une
//! face, le plafond de relances, la moitié des bases, le refus d'une figure et
//! l'occultation des dés appartiennent aux quatre tickets suivants.
//!
//! # Six, et pas un de plus
//!
//! **N'esquisse aucun autre boss, pas même en commentaire.** La v1 donnait
//! trois exemples divergents dans trois documents, et c'est ce qui a rendu le
//! mécanisme illisible. Les boss restants appartiennent à l'Étape 9 ; *Le Mur*,
//! que la courbe de difficulté cite, est une fixture de calibrage et n'entre pas
//! dans ce catalogue.
//!
//! Les noms français sont des **textes joueur** : ils seront externalisés à
//! l'Étape 9, et aucune chaîne n'entre ici.
//!
//! # Un seul boss consomme de l'aléa
//!
//! Cinq rendent une contrainte constante. *La Cage* tire son slot, et elle le
//! tire **sur le flux des boss** — un flux partagé décalerait la séquence des
//! dés, et deux joueurs de même graine n'auraient plus les mêmes lancers dès la
//! première Mise Boss.
//!
//! Le tirage porte sur la **plage de slots**, pas sur les slots occupés : la
//! consommation du flux devient ainsi indépendante du contenu de l'inventaire,
//! ce qui garde une graine reproductible d'une sauvegarde à l'autre.
//! Conséquence assumée : sur un inventaire creux, *La Cage* peut ne rien
//! griser. **Décision de Lead, à réexaminer par l'Étape 6 bis si le boss se
//! révèle sans mordant.**

use rand::RngExt;
use smallvec::smallvec;

use crate::blinds::BlindModifier;
use crate::hands::YahtzeeHand;

/// Les six boss inauguraux.
///
/// Unit-only, donc `Copy`, sérialisable et utilisable comme clé de `match`.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BossId {
    OneEyed,
    Cage,
    Vise,
    Flint,
    Oblivion,
    Rift,
}

impl BossId {
    /// Les six, dans l'ordre de la table du corpus.
    pub const ALL: [BossId; 6] = [
        BossId::OneEyed,
        BossId::Cage,
        BossId::Vise,
        BossId::Flint,
        BossId::Oblivion,
        BossId::Rift,
    ];
}

/// Un boss : son identité et sa contrainte, rien d'autre.
///
/// **Pas de `Copy`** : la contrainte l'a perdu en accueillant une liste de
/// figures. Pas de nom non plus — c'est de l'i18n.
#[cfg_attr(feature = "bevy", derive(bevy_reflect::Reflect))]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BossDefinition {
    pub id: BossId,
    pub modifier: BlindModifier,
}

/// La contrainte d'un boss. **Fonction totale.**
///
/// Le `match` est exhaustif, sans bras attrape-tout, sans `Option`, sans valeur
/// par défaut : un boss sans contrainte déclarée doit être une **erreur de
/// compilation**. C'est ce qui rendra le catalogue de l'Étape 9 sûr à étendre.
///
/// Le flux et la capacité entrent dans la signature pour qu'aucun appelant ne
/// puisse tirer sur le mauvais flux ni borner sur le mauvais nombre. **Seul le
/// bras de *La Cage* consomme quoi que ce soit.**
// Borne simple `impl RngExt`, comme `Die::roll` : voir le commentaire qui
// l'explique là-bas.
pub fn boss_definition(id: BossId, rng: &mut impl RngExt, relic_capacity: u8) -> BossDefinition {
    let modifier = match id {
        // Les cinq qui ne tirent rien : leur contrainte est constante, et le
        // flux ressort au même point qu'il y est entré.
        BossId::OneEyed => BlindModifier::DisableFace(1),
        BossId::Vise => BlindModifier::MaxRerolls(1),
        BossId::Flint => BlindModifier::HalveBaseScores,
        BossId::Oblivion => {
            BlindModifier::DebuffHands(smallvec![YahtzeeHand::Chance, YahtzeeHand::Yahtzee])
        }
        BossId::Rift => BlindModifier::HideDice(2),

        // Le seul qui tire. La borne est gardée comme `Die::roll` garde la
        // sienne : `random_range` panique sur un intervalle vide, et la
        // signature prend un entier nu que rien n'empêche de valoir zéro.
        // Tirer quand même, plutôt que de sortir, garde la consommation du flux
        // indépendante de la capacité.
        BossId::Cage => BlindModifier::DisableRelicSlot(rng.random_range(0..relic_capacity.max(1))),
    };

    BossDefinition { id, modifier }
}

/// Tire un boss uniformément parmi les six, **sur le flux des boss**.
pub fn draw_boss(rng: &mut impl RngExt) -> BossId {
    BossId::ALL[rng.random_range(0..BossId::ALL.len())]
}

#[cfg(test)]
mod tests {
    use rand::RngExt;
    use rand_chacha::ChaCha8Rng;

    use super::*;
    use crate::blinds::BlindModifier;
    use crate::hands::YahtzeeHand;
    use crate::rng::RunRng;

    /// Capacité du gobelet standard, et celle du Gobelet de Fortune.
    const CAPACITE_STANDARD: u8 = 5;
    const CAPACITE_FORTUNE: u8 = 6;

    fn flux(graine: u64) -> RunRng {
        RunRng::from_seed(graine)
    }

    /// Les dix tirages suivants d'un flux, pour dire s'il a avancé.
    ///
    /// **C'est la forme que `RunRng` prescrit.** Son type n'a pas de
    /// `PartialEq`, et sa doc dit pourquoi : « l'égalité d'état interne n'étant
    /// pas le contrat à vérifier ; ce sont les tirages qui doivent coïncider ».
    /// Comparer des chaînes serde marcherait, mais lierait le test à une
    /// représentation que la bibliothèque peut changer.
    fn empreinte(rng: &mut ChaCha8Rng) -> Vec<u32> {
        (0..10).map(|_| rng.random_range(0..u32::MAX)).collect()
    }

    #[test]
    fn test_six_bosses_yield_canonical_modifier() {
        let mut rng = flux(1);
        let attendu = [
            (BossId::OneEyed, BlindModifier::DisableFace(1)),
            (BossId::Vise, BlindModifier::MaxRerolls(1)),
            (BossId::Flint, BlindModifier::HalveBaseScores),
            (BossId::Rift, BlindModifier::HideDice(2)),
        ];
        for (id, modifier) in attendu {
            let definition = boss_definition(id, &mut rng.boss, CAPACITE_STANDARD);
            assert_eq!(definition.id, id);
            assert_eq!(definition.modifier, modifier, "{id:?}");
        }

        // Les deux qui portent une charge non triviale.
        let oubli = boss_definition(BossId::Oblivion, &mut rng.boss, CAPACITE_STANDARD);
        assert!(matches!(oubli.modifier, BlindModifier::DebuffHands(_)));

        let cage = boss_definition(BossId::Cage, &mut rng.boss, CAPACITE_STANDARD);
        let BlindModifier::DisableRelicSlot(slot) = cage.modifier else {
            panic!("La Cage grise un slot");
        };
        assert!(slot < CAPACITE_STANDARD);
    }

    #[test]
    fn test_oblivion_debuffs_exactly_two_hands() {
        let mut rng = flux(2);
        let BlindModifier::DebuffHands(figures) =
            boss_definition(BossId::Oblivion, &mut rng.boss, CAPACITE_STANDARD).modifier
        else {
            panic!("L'Oubli affaiblit des figures");
        };
        assert_eq!(
            figures.as_slice(),
            [YahtzeeHand::Chance, YahtzeeHand::Yahtzee],
            "ni une, ni trois, et dans cet ordre"
        );
    }

    #[test]
    fn test_boss_draw_is_seed_stable() {
        let (mut gauche, mut droite) = (flux(7), flux(7));
        assert_eq!(draw_boss(&mut gauche.boss), draw_boss(&mut droite.boss));

        let suite_gauche: Vec<BossId> = (0..10).map(|_| draw_boss(&mut gauche.boss)).collect();
        let suite_droite: Vec<BossId> = (0..10).map(|_| draw_boss(&mut droite.boss)).collect();
        assert_eq!(suite_gauche, suite_droite);

        // Une autre graine ne donne pas la même suite : sans cela, le test
        // passerait sur un tirage constant.
        let mut autre = flux(8);
        let suite_autre: Vec<BossId> = (0..11).map(|_| draw_boss(&mut autre.boss)).collect();
        assert_ne!(suite_autre[1..], suite_gauche[..]);
    }

    #[test]
    fn test_cage_slot_drawn_on_boss_stream() {
        let mut rng = flux(11);
        let temoin = rng.clone();

        boss_definition(BossId::Cage, &mut rng.boss, CAPACITE_STANDARD);

        // Les trois autres flux n'ont pas bougé.
        for (apres, avant) in [
            (&mut rng.dice, &mut temoin.clone().dice),
            (&mut rng.shop, &mut temoin.clone().shop),
            (&mut rng.relic_effects, &mut temoin.clone().relic_effects),
        ] {
            assert_eq!(empreinte(apres), empreinte(avant));
        }

        // Celui des boss, si.
        assert_ne!(
            empreinte(&mut rng.boss),
            empreinte(&mut temoin.clone().boss),
            "La Cage n'a rien consommé"
        );
    }

    #[test]
    fn test_non_cage_bosses_consume_nothing() {
        for id in [
            BossId::OneEyed,
            BossId::Vise,
            BossId::Flint,
            BossId::Oblivion,
            BossId::Rift,
        ] {
            let mut rng = flux(13);
            let mut temoin = rng.clone();
            boss_definition(id, &mut rng.boss, CAPACITE_STANDARD);

            assert_eq!(
                empreinte(&mut rng.boss),
                empreinte(&mut temoin.boss),
                "{id:?} a consommé le flux"
            );
        }
    }

    #[test]
    fn test_cage_slot_respects_relic_capacity() {
        // **Graine fixée, assertion sur ce qu'elle produit.** « Cent tirages »
        // sur un flux quelconque rendrait le test presque sûr et non sûr : la
        // probabilité qu'aucun 5 ne sorte vaut environ un sur cent millions, ce
        // qui donne un échec irreproductible plutôt qu'un test.
        for capacite in [CAPACITE_STANDARD, CAPACITE_FORTUNE] {
            let mut rng = flux(17);
            let slots: Vec<u8> = (0..100)
                .map(|_| {
                    let BlindModifier::DisableRelicSlot(slot) =
                        boss_definition(BossId::Cage, &mut rng.boss, capacite).modifier
                    else {
                        panic!("La Cage grise un slot");
                    };
                    slot
                })
                .collect();

            assert!(
                slots.iter().all(|slot| *slot < capacite),
                "capacité {capacite}"
            );
            assert_eq!(
                slots.contains(&(CAPACITE_FORTUNE - 1)),
                capacite == CAPACITE_FORTUNE,
                "le dernier slot du Gobelet de Fortune"
            );
        }
    }

    #[test]
    fn test_cage_without_any_slot_does_not_panic() {
        // Aucun gobelet ne donne zéro slot, mais la signature prend un `u8` nu,
        // et `random_range` panique sur un intervalle vide — c'est la mesure que
        // `Die::roll` porte déjà. La borne est donc gardée, et le flux avance
        // quand même : sa consommation ne doit pas dépendre de la capacité,
        // sans quoi deux runs de même graine divergeraient.
        let mut rng = flux(19);
        let mut temoin = rng.clone();

        let definition = boss_definition(BossId::Cage, &mut rng.boss, 0);
        assert_eq!(definition.modifier, BlindModifier::DisableRelicSlot(0));
        assert_ne!(empreinte(&mut rng.boss), empreinte(&mut temoin.boss));
    }

    #[test]
    fn test_boss_catalog_has_exactly_six() {
        assert_eq!(BossId::ALL.len(), 6);

        let mut vus = BossId::ALL.to_vec();
        vus.sort_unstable_by_key(|id| format!("{id:?}"));
        vus.dedup();
        assert_eq!(vus.len(), 6, "deux variantes se confondent");
    }
}
