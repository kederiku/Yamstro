//! *Miroir Double* (Rare) — ré-émet la production de sa voisine de gauche.
//!
//! # Pourquoi cette relique a fait tomber l'architecture v1
//!
//! Écrite naïvement — une boucle `for` sur les slots empruntés en écriture, où
//! chaque relique mute le `ScoreContext` —, elle réclame, **pendant** l'emprunt
//! exclusif de son propre élément, un **second emprunt exclusif d'un autre
//! élément du même slice**. Rien ne sauve ce code localement : ni copier
//! l'instance voisine, ni garder un index de côté, ni passer par une fonction
//! libre à deux paramètres exclusifs. Le repli qui vient ensuite à l'esprit,
//! split_at_mut, compile : c'est un repli **inférieur**, et il est écarté. Il
//! conserve la mutation pendant le parcours et **n'ordonne pas le journal** —
//! or l'Étape 4 le dépile palier par palier : ce serait la v1 avec un cachet de
//! conformité.
//!
//! Le correctif est **structurel**, et il est acquis depuis l'Étape 2 : le
//! pipeline a deux phases. En phase A, chaque slot produit ses effets depuis un
//! contexte **en lecture seule** — rien n'est muté, donc rien n'est emprunté en
//! écriture, et ce module n'a qu'à **lire** une tranche immuable. En phase B,
//! les effets sont repliés séquentiellement sur le `ScoreContext`. Le conflit
//! ne peut pas se reformer parce que la phase A ne dispose d'aucun accès
//! exclusif. C'est pourquoi ce module ne nomme ni l'inventaire ni le
//! `ScoreContext` : il ne connaît que `ctx.left_effects`, `ctx.uid`, et le type
//! de l'effet qu'il rend.
//!
//! # Ce qui est recopié, et ce qui ne l'est pas
//!
//! **Seule l'action est recopiée ; la source devient celle du Miroir.** Garder
//! la source d'origine est le geste naturel — on recopie l'effet entier — et le
//! score reste juste au centième près : seule l'attribution change. Elle n'est
//! visible que dans le journal, et c'est elle qui décide quelle carte l'Étape 4
//! secoue : celle du Miroir, non celle de la voisine, déjà secouée à son propre
//! palier.
//!
//! La tranche est ré-émise **dans l'ordre**, sans tri ni déduplication. Vide,
//! elle donne une liste vide : jamais une panique, jamais un effet neutre de
//! remplissage.

use smallvec::SmallVec;

use crate::relics::RelicId;
use crate::scoring::{Hook, ScoreEffect, StepSource, TriggerCtx};

pub(crate) fn effects(hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]> {
    let mut effects = SmallVec::new();
    // Miroiter par dé comptabilisé démultiplierait la voisine par le nombre de
    // dés. Le malus de relance de l'Obsidienne échappe de même au miroir : il
    // sort d'une autre fonction, sur un autre hook, que la phase A n'atteint
    // pas.
    if hook != Hook::OnHandScored {
        return effects;
    }

    // `left_effects` est un canal à **une case** : la production du slot
    // immédiatement à gauche, vide au slot 0 comme sur un voisin absent ou
    // éteint. Remonter plus loin pour « retrouver la vraie voisine » rendrait
    // le Miroir transparent aux boss d'extinction, et exigerait depuis ici un
    // accès à l'inventaire que la signature interdit.
    effects.extend(ctx.left_effects.iter().map(|effect| ScoreEffect {
        source: StepSource::Relic {
            uid: ctx.uid,
            def: RelicId::DoubleMirror,
        },
        action: effect.action,
    }));
    effects
}
