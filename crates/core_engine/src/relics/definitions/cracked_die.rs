//! *Le Dé Fêlé* (Commune) — un Mult par dé impair comptabilisé.
//!
//! Sur la main de référence — cinq dés comptabilisés, faces uniformes sur un à
//! six — l'espérance est de 2,5 dés impairs, soit 2,5 unités de compte sur un
//! budget Commune de 40 : **63 %**.
//!
//! **Ne lit que le dé courant.** Le parcours appartient au pipeline, qui appelle
//! cette fonction une fois par dé comptabilisé ; itérer le plateau ici
//! produirait un effet par dé écarté **et** multiplierait les effets par le
//! nombre de dés de la figure.
//!
//! Aucune de ces trois reliques ne tire d'aléatoire : la parité, la face et
//! l'appartenance à la figure sont entièrement déterminées par le contexte.

use smallvec::SmallVec;

use crate::relics::RelicId;
use crate::scoring::{Hook, ScoreAction, ScoreEffect, StepSource, TriggerCtx};

/// **Aucun déballage paniquant** : rien n'interdit structurellement un appel
/// sans dé courant, et la forme de lecture rend alors une file vide.
pub(crate) fn effects(hook: Hook, ctx: &TriggerCtx) -> SmallVec<[ScoreEffect; 2]> {
    let mut effects = SmallVec::new();
    if hook != Hook::OnScoringDie {
        return effects;
    }
    let Some((_, value)) = ctx.die else {
        return effects;
    };

    // Une face au-delà de six garde sa parité naturelle : sept est impair.
    if value % 2 == 1 {
        effects.push(ScoreEffect {
            source: StepSource::Relic {
                uid: ctx.uid,
                def: RelicId::CrackedDie,
            },
            // Le Mult est en centièmes : un Mult s'écrit cent.
            action: ScoreAction::AddMult(100),
        });
    }
    effects
}
