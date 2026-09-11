//! *Dé Fantôme* (Peu commune) — au premier lancer, relève le dé le plus faible.
//!
//! # Pourquoi il lit `ctx.dice` et non `hand.scoring_dice`
//!
//! **C'est la seule exception à la règle « tout effet par dé lit les dés
//! comptabilisés », et elle tient au déclencheur, pas à la relique.** Au moment
//! du lancer, aucune figure n'est retenue : `scoring_dice` est vide ou porte
//! encore la main précédente, et la relique ne partirait jamais. Un agent qui
//! « corrige » vers `scoring_dice` par cohérence avec les onze autres obtient
//! du code qui compile et une relique inerte.
//!
//! # Ce qui est littéral
//!
//! La condition porte sur la valeur **1** et la cible sur la valeur **6**, au
//! pied de la lettre : ni « la face minimale », ni « la face maximale ». Sur un
//! D8 (ADR-008), un dé forcé passe à 6, pas à 8.
//!
//! # Déterminisme
//!
//! Aucun aléa. Le lancer a déjà eu lieu ; cette relique ne fait que désigner un
//! dé et une valeur cible. À égalité sur la valeur, le plus petit `DieId`
//! l'emporte — même règle de départage que l'évaluateur, et deux exécutions de
//! même graine rendent la même paire.
//!
//! Le balayage ne filtre pas les dés verrouillés : le montage de manche remet
//! tous les verrous à faux avant le premier lancer, et la garde `roll_index ==
//! 0` précède le balayage. Un filtre serait une branche qu'aucun test ne peut
//! atteindre, donc dont on ne saurait jamais si elle est juste.

use crate::scoring::{RollModifier, TriggerCtx};

pub(crate) fn roll_modifier(ctx: &TriggerCtx) -> RollModifier {
    if ctx.roll_index != 0 {
        return RollModifier::default();
    }
    if ctx.dice.iter().any(|de| de.current_value == 1) {
        return RollModifier::default();
    }

    // Par identité, jamais par position : *La Meule* retire un dé en cours de
    // manche, et tout indice capturé avant serait faux après.
    let Some(cible) = ctx.dice.iter().min_by_key(|de| (de.current_value, de.id)) else {
        return RollModifier::default();
    };

    RollModifier {
        force_values: vec![(cible.id, 6)],
        ..RollModifier::default()
    }
}
