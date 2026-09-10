//! Le canal unique entre la mise en scène du score et l'Étape 8.
//!
//! # Aucune ressource de son n'est créée ici, et ce n'est pas un oubli
//!
//! Cette étape ne crée aucun pool de sons, aucune banque, aucune modulation de
//! hauteur, aucun fichier de son, et `Cargo.toml` ne déclare aucune dépendance
//! audio. La v1 avait **trois ressources concurrentes pour un seul rôle**
//! (défaut C38) : `AudioFeedbackConfig` et `ScoringAudioPool` à l'Étape 4,
//! `SoundEffectBank` à l'Étape 8, réparties sur deux documents et destinées à
//! diverger. **L'Étape 8 possède seule le son, le mixage et la montée
//! harmonique, par `SoundEffectBank`.** Les deux premiers noms sont proscrits ;
//! ils ne sont écrits ici que pour être interdits, et la garde du volet 1 est
//! ancrée sur une forme de **déclaration ou d'emploi**, jamais sur le nom nu,
//! pour que ce paragraphe ait le droit d'exister.
//!
//! La montée de hauteur suit le **demi-ton tempéré de l'Étape 8** (C41), appliqué
//! là-bas et plafonné à 24 demi-tons. Le « +0,05 linéaire » de la v1 n'existe
//! plus, et aucune constante de hauteur n'a de raison d'apparaître dans cette
//! crate.

use bevy::prelude::*;
use core_engine::scoring::{ScoreAction, ScoreStep, StepSource};

/// Un palier de score vient d'être joué à l'écran.
///
/// **Message tamponné, jamais composant ni ressource.** En 0.19 le trait des
/// événements tamponnés est `Message` — `Event` désigne désormais ce que lisent
/// les observateurs. Le dérivé est `Message`, l'écriture passe par
/// `MessageWriter`, l'enregistrement par `App::add_message`, et le tampon est la
/// ressource `Messages<ScoreStepPlayed>` (bevy_ecs 0.19.1,
/// `src/message/mod.rs`; bevy_app 0.19.1, `App::add_message`).
///
/// # Deux champs, et c'est un plafond
///
/// Ni index, ni horodatage, ni `chips_after`, ni hauteur, ni volume, ni entité.
/// L'Étape 8 dérive tout de ces deux champs : le timbre de `source`,
/// l'intensité de `action`, le demi-ton de son propre compteur. Un champ de plus
/// ici serait une décision de son prise dans la mauvaise crate.
///
/// # La ressemblance avec `ScoreEffect` est voulue, et ne doit pas devenir un
/// emprunt
///
/// `core_engine::scoring::ScoreEffect` porte exactement les deux mêmes champs.
/// Un `ScoreStepPlayed(ScoreEffect)` supprimerait la duplication — et ferait
/// hériter le message de **chaque champ futur** du type moteur, c'est-à-dire
/// exactement ce que le plafond ci-dessus interdit. Les deux types ont des
/// propriétaires et des règles d'évolution différents : `ScoreEffect` appartient
/// au journal du moteur, `ScoreStepPlayed` au canal de présentation.
///
/// `From<&ScoreStep>` est le **point de conversion unique**. Le jour où le type
/// moteur gagne un champ, la compilation casse ici, à un seul endroit, et
/// quelqu'un décide si ce champ entre dans le message. Sans lui, un dépileur qui
/// recopie les champs à la main laisse la question sans réponse et sans signal.
///
/// **Corrigé à TASK-49 :** la conversion portait d'abord sur `ScoreEffect`, qui
/// ne circule pas sur ce chemin. La file transporte des `ScoreStep` — les deux
/// mêmes champs, plus les trois valeurs d'après — et c'est un `&ScoreStep` que
/// le dépileur reçoit. Une conversion depuis un type que personne ne fait
/// passer n'est pas un point de passage.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreStepPlayed {
    pub source: StepSource,
    pub action: ScoreAction,
}

impl From<&ScoreStep> for ScoreStepPlayed {
    fn from(palier: &ScoreStep) -> Self {
        let ScoreStep {
            source,
            action,
            chips_after: _,
            mult_after: _,
            score_after: _,
        } = *palier;
        Self { source, action }
    }
}

#[cfg(test)]
mod tests {
    // `use super::*` suffit : les imports du module parent traversent la
    // frontière du module de test, et les redéclarer est refusé sous
    // `-D warnings`.
    use super::*;
    use core_engine::hands::YahtzeeHand;

    fn palier() -> ScoreStep {
        ScoreStep {
            source: StepSource::HandBase {
                hand: YahtzeeHand::Yahtzee,
            },
            action: ScoreAction::MultiplyMult(150),
            chips_after: 120,
            mult_after: 430,
            score_after: 516,
        }
    }

    #[test]
    fn test_juice_plugin_registers_the_message_buffer() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::JuicePlugin));
        app.init_resource::<Recu>();
        app.add_systems(
            Update,
            (
                |mut w: MessageWriter<ScoreStepPlayed>| {
                    w.write(ScoreStepPlayed::from(&palier()));
                },
                |mut r: MessageReader<ScoreStepPlayed>, mut recu: ResMut<Recu>| {
                    recu.0.extend(r.read().copied());
                },
            )
                .chain(),
        );
        app.update();
        assert_eq!(
            app.world().resource::<Recu>().0,
            vec![ScoreStepPlayed::from(&palier())]
        );
    }

    #[derive(Resource, Default)]
    struct Recu(Vec<ScoreStepPlayed>);

    #[test]
    fn test_score_step_played_has_exactly_two_fields() {
        let ScoreStepPlayed { source, action } = ScoreStepPlayed::from(&palier());
        assert_eq!(source, palier().source);
        assert_eq!(action, palier().action);
    }

    #[test]
    fn test_from_score_step_copies_both_fields() {
        let effet = palier();
        let message = ScoreStepPlayed::from(&effet);
        assert_eq!(message.source, effet.source);
        assert_eq!(message.action, effet.action);
    }

    /// Aucune ressource audio n'est enregistrée par `JuicePlugin`.
    ///
    /// **Le nom est celui qu'impose le corpus ; la méthode ne l'est pas.** Il
    /// proposait de scanner le registre de types du `World`, or
    /// `ComponentInfo::name()` rend un `DebugName` vide sans la feature
    /// `bevy_utils/debug`, absente de notre graphe : tous les noms valent le
    /// même texte de remplacement, un diff par nom annonce que le plugin
    /// n'ajoute **aucune** ressource alors qu'il en ajoute deux, et un scan
    /// cherchant « Audio » passerait sur une application pleine de ressources
    /// audio.
    ///
    /// L'inventaire porte donc sur des **types concrets** et un **delta de
    /// compte** : ajouter la moindre ressource au plugin fait échouer ce test,
    /// audio ou non.
    #[test]
    fn test_no_audio_resource_registered() {
        let mut base = App::new();
        base.add_plugins(MinimalPlugins);
        let avant = base.world().iter_resources().count();

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::JuicePlugin));
        let apres = app.world().iter_resources().count();

        assert!(
            app.world()
                .contains_resource::<crate::animation::ScreenShake>()
        );
        assert!(
            app.world()
                .contains_resource::<crate::settings::JuiceSettings>()
        );
        assert!(app.world().contains_resource::<Messages<ScoreStepPlayed>>());
        assert_eq!(apres - avant, 3, "inventaire de JuicePlugin");
    }
}
