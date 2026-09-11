//! Cartes de relique : une entité par slot occupé, et rien de plus.
//!
//! # Aucune donnée de jeu dans une entité
//!
//! Une carte porte son **index de slot**, et c'est tout. Le reste se lit dans
//! `RelicInventory`, source de vérité unique. C'est la correction de C37 :
//! l'ancienne architecture portait trois représentations concurrentes du même
//! inventaire, et une entité qui porterait une instance se désynchroniserait au
//! premier réordonnancement.
//!
//! La règle est ici tenue **par le compilateur** : ni `RelicInstance`, ni
//! `RelicId`, ni `RelicState` ne dérivent `Component`, donc les poser sur une
//! entité ne compile pas. Ce qui reste à garder est qu'aucun type local ne
//! vienne les recopier, et c'est une garde de CI.
//!
//! # Une carte est un nœud UI, et n'a donc pas de `Transform`
//!
//! **Mesuré, et c'est une conséquence à transmettre.** En Bevy 0.19, `Node`
//! requiert `UiTransform`, pas `Transform`, et le layout écrit
//! `UiGlobalTransform` : aucun `&mut Transform` n'existe dans tout `bevy_ui`.
//!
//! Or `animate_punch_scale` interroge `(Entity, &mut Transform, &mut
//! PunchScale)`. Une carte qui ne porte qu'un `Node` **n'est jamais vue par
//! cette requête** : le `PunchScale` que le dépileur de l'Étape 4 pose dessus
//! n'est ni animé, ni retiré, et reste posé pour toujours. Aucune erreur,
//! aucun test rouge — le test d'Étape 4 passe parce qu'il fabrique sa carte à
//! la main avec un `Transform` et sans `Node`.
//!
//! **La branche relique du dépileur ne peut donc pas se refermer sur ce
//! gabarit.** Elle était déjà listée comme partielle à la clôture de l'Étape 4 ;
//! la raison exacte est celle-ci, et la forme du correctif aussi :
//! `UiTransform` porte un `scale: Vec2`, et il faudra un ressort qui l'écrive.
//! Ce ticket ne le fait pas — son périmètre s'arrête à rendre la cible
//! résoluble.
//!
//! # Le placement est une fonction de l'index, et rien d'autre
//!
//! `layout_relic_slots` écrit dans `Node`, jamais dans `Transform` : les deux
//! types sont disjoints, donc l'invariant de TASK-52 tient sans dérogation.
//! Aucun état de placement n'est mémorisé dans l'entité.

use bevy::prelude::*;
use core_engine::relics::RelicInventory;

use crate::components::RelicSlotUI;
use crate::states::AppState;

/// Écart horizontal entre deux cartes. **Provisoire** : l'habillage des cartes
/// appartient à l'Étape 7, qui fixera la vraie grille. Ce qui est normatif ici,
/// c'est que la position soit une fonction de l'index.
const PAS_HORIZONTAL: f32 = 96.0;

/// Le slot qu'occupe la relique d'identifiant `uid`, s'il en reste une.
///
/// **Seule surface dont `ui_and_juice` a besoin**, et elle respecte le sens des
/// dépendances. La résolution passe par l'inventaire à chaque appel, jamais par
/// un index de position mémorisé : la capacité varie d'un gobelet à l'autre, le
/// joueur réordonne, une relique se vend. Un `uid` inconnu rend `None` — le
/// dépileur n'a alors rien à secouer, et surtout rien à faire paniquer.
#[must_use]
pub fn slot_of_uid(inventory: &RelicInventory, uid: u32) -> Option<u8> {
    inventory
        .iter_slots()
        .find(|(_, instance)| instance.uid == uid)
        .map(|(slot, _)| slot)
}

/// Fait naître les cartes des slots nouvellement occupés et mourir celles des
/// slots vidés.
///
/// **Un différentiel, jamais une reconstruction.** Tout despawner à chaque
/// changement serait plus court et jetterait au passage ce que le dépileur
/// aurait posé sur une carte. Une carte survit tant que son slot reste occupé :
/// un réordonnancement ne change que le **contenu** d'un slot, et une carte ne
/// portant aucune donnée de jeu, elle n'a rien à changer.
pub fn sync_relic_cards(
    mut commands: Commands,
    inventory: Res<RelicInventory>,
    cards: Query<(Entity, &RelicSlotUI)>,
) {
    for (slot, _) in inventory.iter_slots() {
        if !cards.iter().any(|(_, carte)| carte.0 == slot) {
            commands.spawn((RelicSlotUI(slot), Node::default()));
        }
    }

    for (entite, carte) in &cards {
        let occupe = inventory
            .slots
            .get(usize::from(carte.0))
            .is_some_and(Option::is_some);
        if !occupe {
            commands.entity(entite).despawn();
        }
    }
}

/// Place chaque carte d'après son index de slot.
///
/// Écrit `Node`, jamais `Transform` — voir l'en-tête du module. Ce n'est pas
/// une incohérence à harmoniser : c'est ce qui laisse l'unique écrivain de
/// `Transform` du projet là où l'Étape 4 l'a mis.
pub fn layout_relic_slots(mut slots: Query<(&RelicSlotUI, &mut Node)>) {
    for (carte, mut node) in &mut slots {
        node.position_type = PositionType::Absolute;
        node.left = Val::Px(f32::from(carte.0) * PAS_HORIZONTAL);
    }
}

pub(crate) fn register(app: &mut App) {
    // Conditionnés, jamais supposés : la validation des paramètres passe par
    // `get_param`, qui rend un `Result`, et un `Res` d'une ressource absente
    // remonte une erreur au schedule au lieu de paniquer franchement.
    app.add_systems(
        Update,
        (sync_relic_cards, layout_relic_slots).chain().run_if(
            // `and_then` et non `and` : ce dernier est **déprécié en 0.19.1**
            // au profit de `and_then`, court-circuitant, et de `and_eager`, qui
            // évalue les deux. Hors run, il n'y a aucune raison d'aller
            // interroger la ressource.
            in_state(AppState::InRun).and_then(resource_exists::<RelicInventory>),
        ),
    );
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::state::app::StatesPlugin;
    use core_engine::cups::CupId;
    use core_engine::relics::{RelicId, RelicInventory};

    use super::*;
    use crate::components::RelicSlotUI;
    use crate::states::AppState;
    use crate::systems::fixtures::app_en_run;

    fn deux_frames(app: &mut App) {
        app.update();
        app.update();
    }

    fn cartes(app: &mut App) -> Vec<(Entity, u8)> {
        let mut etat = app.world_mut().query::<(Entity, &RelicSlotUI)>();
        let mut v: Vec<(Entity, u8)> = etat
            .iter(app.world())
            .map(|(entite, slot)| (entite, slot.0))
            .collect();
        v.sort_unstable_by_key(|(_, slot)| *slot);
        v
    }

    fn remplir(app: &mut App) -> usize {
        let capacite = app.world().resource::<RelicInventory>().slots.len();
        {
            let mut stock = app.world_mut().resource_mut::<RelicInventory>();
            while stock.add_relic(RelicId::CrackedDie).is_some() {}
        }
        capacite
    }

    #[test]
    fn test_card_count_follows_relic_capacity() {
        let mut standard = app_en_run(CupId::Standard);
        remplir(&mut standard);
        deux_frames(&mut standard);

        let mut fortune = app_en_run(CupId::Fortune);
        remplir(&mut fortune);
        deux_frames(&mut fortune);

        // L'écart, jamais les nombres : un littéral de capacité dans le test
        // laisserait passer un littéral dans le système.
        assert_eq!(cartes(&mut fortune).len(), cartes(&mut standard).len() + 1);
        assert!(!cartes(&mut standard).is_empty());
    }

    #[test]
    fn test_removing_relic_despawns_its_card() {
        let mut app = app_en_run(CupId::Standard);
        {
            let mut stock = app.world_mut().resource_mut::<RelicInventory>();
            for _ in 0..3 {
                stock.add_relic(RelicId::CrackedDie).expect("slot libre");
            }
        }
        deux_frames(&mut app);
        assert_eq!(cartes(&mut app).len(), 3);
        let avant: Vec<u32> = app
            .world()
            .resource::<RelicInventory>()
            .iter_slots()
            .map(|(_, inst)| inst.uid)
            .collect();

        app.world_mut()
            .resource_mut::<RelicInventory>()
            .remove_relic(1)
            .expect("relique au slot 1");
        deux_frames(&mut app);

        assert_eq!(
            cartes(&mut app).iter().map(|(_, s)| *s).collect::<Vec<_>>(),
            vec![0, 2],
            "la carte du slot vidé subsiste"
        );
        let apres: Vec<u32> = app
            .world()
            .resource::<RelicInventory>()
            .iter_slots()
            .map(|(_, inst)| inst.uid)
            .collect();
        assert_eq!(apres, vec![avant[0], avant[2]]);
    }

    #[test]
    fn test_uid_to_entity_after_reorder() {
        let mut app = app_en_run(CupId::Standard);
        {
            let mut stock = app.world_mut().resource_mut::<RelicInventory>();
            for _ in 0..3 {
                stock.add_relic(RelicId::CrackedDie).expect("slot libre");
            }
        }
        deux_frames(&mut app);
        let uids: Vec<u32> = app
            .world()
            .resource::<RelicInventory>()
            .iter_slots()
            .map(|(_, inst)| inst.uid)
            .collect();

        app.world_mut()
            .resource_mut::<RelicInventory>()
            .reorder(0, 2);
        deux_frames(&mut app);

        // Le premier `uid` est passé du slot 0 au slot 2 : une résolution par
        // index figé rendrait encore 0.
        let stock = app.world().resource::<RelicInventory>();
        assert_eq!(slot_of_uid(stock, uids[0]), Some(2));
        assert_eq!(slot_of_uid(stock, uids[1]), Some(0));
        assert_eq!(slot_of_uid(stock, uids[2]), Some(1));

        let table = cartes(&mut app);
        for (rang, uid) in uids.iter().enumerate() {
            let slot =
                slot_of_uid(app.world().resource::<RelicInventory>(), *uid).expect("uid connu");
            let carte = table.iter().find(|(_, s)| *s == slot);
            assert!(carte.is_some(), "uid {uid} (rang {rang}) sans carte");
        }
    }

    #[test]
    fn test_layout_gives_each_slot_its_own_position() {
        // **Ce que le placement peut réellement rater**, c'est d'ignorer
        // l'index : deux cartes superposées. L'autre moitié du test prescrit —
        // « aucun `Transform` n'est écrit » — ne mesure rien, aucun test du
        // dépôt ne montant `UiPlugin` et la carte ne portant pas de
        // `Transform` du tout. Elle est tenue par une garde de CI.
        let mut app = app_en_run(CupId::Standard);
        {
            let mut stock = app.world_mut().resource_mut::<RelicInventory>();
            for _ in 0..3 {
                stock.add_relic(RelicId::CrackedDie).expect("slot libre");
            }
        }
        deux_frames(&mut app);

        let mut etat = app.world_mut().query::<(&RelicSlotUI, &Node)>();
        let mut gauches: Vec<(u8, Val)> = etat
            .iter(app.world())
            .map(|(slot, node)| (slot.0, node.left))
            .collect();
        gauches.sort_unstable_by_key(|(slot, _)| *slot);

        assert_eq!(gauches.len(), 3);
        assert_ne!(
            gauches[0].1, gauches[1].1,
            "les slots 0 et 1 se superposent"
        );
        assert_ne!(
            gauches[1].1, gauches[2].1,
            "les slots 1 et 2 se superposent"
        );
    }

    #[test]
    fn test_card_is_a_ui_node_and_carries_no_transform() {
        // **« Aucune entité ne porte de `RelicInstance` » n'est pas testable** :
        // ni `RelicInstance`, ni `RelicId`, ni `RelicState` ne dérivent
        // `Component`, donc les poser sur une entité ne compile pas. Et la
        // vérification par nom de composant serait vaine de toute façon :
        // `ComponentInfo::name()` rend un `DebugName` vide sans
        // `bevy_utils/debug`, absent de ce graphe.
        //
        // Ce qui se mesure, c'est la forme de la carte : un nœud UI, et **pas**
        // de `Transform` — car `Node` requiert `UiTransform` en 0.19, non
        // `Transform`. C'est cette absence-là qui empêche `animate_punch_scale`
        // d'animer une carte, et il vaut mieux qu'un test la constate que
        // qu'elle se découvre à l'écran.
        let mut app = app_en_run(CupId::Standard);
        app.world_mut()
            .resource_mut::<RelicInventory>()
            .add_relic(RelicId::CrackedDie)
            .expect("slot libre");
        deux_frames(&mut app);

        let (carte, _) = cartes(&mut app)[0];
        let monde = app.world();
        assert!(
            monde.get::<Node>(carte).is_some(),
            "la carte n'est pas un nœud UI"
        );
        assert!(
            monde.get::<Transform>(carte).is_none(),
            "la carte porte un Transform : le ressort de l'Étape 4 la verrait, \
             et le placement pourrait l'écrire"
        );
    }

    #[test]
    fn test_relic_slot_ui_is_component_only() {
        // Contre-épreuve : une dérivation `Resource` compilerait et ne
        // laisserait qu'une seule des deux entités.
        let mut monde = World::new();
        let premiere = monde.spawn(RelicSlotUI(0)).id();
        let seconde = monde.spawn(RelicSlotUI(1)).id();

        assert_eq!(monde.get::<RelicSlotUI>(premiere), Some(&RelicSlotUI(0)));
        assert_eq!(monde.get::<RelicSlotUI>(seconde), Some(&RelicSlotUI(1)));
    }

    #[test]
    fn test_systems_are_conditioned_out_of_run() {
        // Aucune `RelicInventory` insérée, aucune run entamée : les systèmes
        // doivent ne pas tourner, et non paniquer sur un paramètre absent.
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, bevy::input::InputPlugin));
        app.add_plugins(crate::GameStatePlugin);
        for _ in 0..4 {
            app.update();
        }

        assert_eq!(
            app.world().resource::<State<AppState>>().get(),
            &AppState::MainMenu
        );
        assert!(cartes(&mut app).is_empty());
    }

    #[test]
    fn test_slot_of_uid_is_a_slot_index_not_a_rank() {
        // **Le cas discriminant, et le seul.** Sur un inventaire contigu, le
        // rang d'une relique parmi les slots occupés et son index de slot
        // coïncident : aucun des autres tests ne les sépare, et une résolution
        // par rang leur survit. `remove_relic` prend par `take`, donc il laisse
        // un trou — c'est ce trou qui fait diverger les deux.
        let mut app = app_en_run(CupId::Standard);
        {
            let mut stock = app.world_mut().resource_mut::<RelicInventory>();
            for _ in 0..3 {
                stock.add_relic(RelicId::CrackedDie).expect("slot libre");
            }
        }
        let uids: Vec<u32> = app
            .world()
            .resource::<RelicInventory>()
            .iter_slots()
            .map(|(_, inst)| inst.uid)
            .collect();

        app.world_mut()
            .resource_mut::<RelicInventory>()
            .remove_relic(1)
            .expect("relique au slot 1");
        deux_frames(&mut app);

        let stock = app.world().resource::<RelicInventory>();
        assert_eq!(slot_of_uid(stock, uids[0]), Some(0));
        assert_eq!(
            slot_of_uid(stock, uids[2]),
            Some(2),
            "résolu par rang : la troisième relique est au slot 2, pas au rang 1"
        );
        assert_eq!(slot_of_uid(stock, uids[1]), None, "relique retirée");
    }

    #[test]
    fn test_unknown_uid_resolves_to_none() {
        let stock = RelicInventory::new(3);
        assert_eq!(slot_of_uid(&stock, 42), None);
    }
}
