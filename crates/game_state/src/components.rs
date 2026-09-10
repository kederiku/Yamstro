//! Composants d'affichage et marqueurs. Aucune donnée de jeu ne vit ici.
//!
//! # La source de vérité est une ressource, jamais une entité
//!
//! Les entités ne portent que de l'affichage. `RelicSlotUI` ne transporte
//! qu'un index ; l'identité de la relique, son état et son numéro d'exemplaire
//! vivent dans l'inventaire, qui est une ressource. Une entité qui porterait
//! l'un de ces trois-là deviendrait une seconde source de vérité, désynchronisée
//! dès la première réorganisation de l'inventaire par le joueur. De même,
//! `DieView` ne porte pas la valeur du dé : elle est dans le `Die` de
//! `core_engine`, sur la même entité.
//!
//! # Les six types sont des composants, et rien d'autre
//!
//! En 0.19, `Resource` est un **sous-trait** de `Component` : dériver
//! `Resource` implémente les deux. Promouvoir l'un de ces types en ressource
//! compile sans un mot, et casse tout à l'exécution.
//!
//! **Le symptôme n'est pas celui qu'on croit.** Mesuré sur six entités portant
//! chacune une copie :
//!
//! ```text
//! composant pur : 6/6 portent encore leur composant
//! ressource     : 1/6 le portent, 6/6 entités existent, survivant = le PREMIER
//! ```
//!
//! Les entités **ne sont pas détruites** : elles gardent leur `Die`, leur
//! transformation et leur sprite, et perdent seulement le composant. Elles
//! cessent donc de répondre aux requêtes qui le demandent, sans disparaître de
//! l'écran. Et c'est la **première** insertion qui survit, pas la dernière :
//! chercher ce qui casse « au spawn du sixième dé » mène au mauvais endroit.
//!
//! La preuve mécanique est la paire de doc-tests ci-dessous. Ils ne diffèrent
//! que par la borne, ce qui est la seule façon d'établir qu'un `compile_fail`
//! échoue pour la bonne raison — sans son jumeau positif, un `compile_fail`
//! passe aussi bien sur une faute de frappe.
//!
//! ```
//! fn exige_component<T: bevy::ecs::component::Component>() {}
//! exige_component::<game_state::components::DieView>();
//! exige_component::<game_state::components::Locked>();
//! exige_component::<game_state::components::Scoring>();
//! exige_component::<game_state::components::Hidden>();
//! exige_component::<game_state::components::RelicSlotUI>();
//! exige_component::<game_state::components::PunchScale>();
//! ```
//!
//! **Un bloc par type, et non les six d'un coup.** Mesuré : groupés, ils ne
//! garderaient qu'une disjonction — « au moins un des six n'est pas une
//! ressource » — et resteraient au vert avec un seul type promu, les cinq
//! autres suffisant à faire échouer la compilation. Séparés, chacun mord sur
//! le sien.
//!
//! ```compile_fail
//! fn exige_resource<T: bevy::ecs::resource::Resource>() {}
//! exige_resource::<game_state::components::DieView>();
//! ```
//!
//! ```compile_fail
//! fn exige_resource<T: bevy::ecs::resource::Resource>() {}
//! exige_resource::<game_state::components::Locked>();
//! ```
//!
//! ```compile_fail
//! fn exige_resource<T: bevy::ecs::resource::Resource>() {}
//! exige_resource::<game_state::components::Scoring>();
//! ```
//!
//! ```compile_fail
//! fn exige_resource<T: bevy::ecs::resource::Resource>() {}
//! exige_resource::<game_state::components::Hidden>();
//! ```
//!
//! ```compile_fail
//! fn exige_resource<T: bevy::ecs::resource::Resource>() {}
//! exige_resource::<game_state::components::RelicSlotUI>();
//! ```
//!
//! ```compile_fail
//! fn exige_resource<T: bevy::ecs::resource::Resource>() {}
//! exige_resource::<game_state::components::PunchScale>();
//! ```

use bevy::prelude::*;

/// Rang d'affichage, dérivé de l'index dans la liste des dés. **Jamais un
/// identifiant** : il se recalcule à chaque mise en place de manche, et
/// retrouver un dé passe par son `DieId`, pas par son rang.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DieView {
    pub order: u8,
}

/// Dé conservé à la relance.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locked;

/// Dé participant à la figure soumise. Posé au moment de la soumission.
///
/// Le nom n'a pas de suffixe, et c'est délibéré : le suffixe entrerait en
/// collision avec la variante du journal de score qui impute un pas à un dé.
/// Deux concepts homonymes dans un corpus qui parle sans cesse de « dé qui
/// score » se confondraient à la lecture.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scoring;

/// Dé masqué par un modificateur de blind.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hidden;

/// Emplacement d'affichage d'une relique. **Un index, et rien d'autre.**
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelicSlotUI(pub u8);

/// Impulsion d'échelle posée par commande, consommée par l'unique système
/// d'animation.
///
/// Les flottants sont légitimes ici : l'interdiction porte sur l'arithmétique
/// de score, tenue en point fixe, pas sur l'animation. Les réglages viennent de
/// l'appelant ; ce fichier n'en fixe aucun.
///
/// **Les champs ont changé à TASK-39**, et ce n'est pas un renommage. La forme
/// d'origine portait une échelle courante ; un ressort amorti a besoin d'une
/// **vitesse**, sans quoi son intégration est inexprimable. `offset` est
/// l'écart d'échelle courant, et l'échelle appliquée vaut
/// `base_scale * (1.0 + offset)`.
///
/// `base_scale` vaut `Vec3::ONE` par convention : toute entité animable est
/// instanciée à `Transform.scale == Vec3::ONE`, sa taille visuelle venant du
/// sprite ou du nœud d'interface.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PunchScale {
    pub base_scale: Vec3,
    pub offset: f32,
    pub velocity: f32,
    /// Raideur du ressort.
    pub elasticity: f32,
    /// Amortissement.
    pub decay: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::dice::{Die, DieId};

    #[test]
    fn test_all_six_components_attach_and_read_back() {
        let mut world = World::new();
        let entite = world
            .spawn((
                DieView { order: 3 },
                Locked,
                Scoring,
                Hidden,
                RelicSlotUI(2),
                PunchScale {
                    base_scale: Vec3::ONE,
                    offset: 0.2,
                    velocity: 0.0,
                    elasticity: 0.5,
                    decay: 0.8,
                },
            ))
            .id();

        assert_eq!(world.get::<DieView>(entite), Some(&DieView { order: 3 }));
        assert_eq!(world.get::<Locked>(entite), Some(&Locked));
        assert_eq!(world.get::<Scoring>(entite), Some(&Scoring));
        assert_eq!(world.get::<Hidden>(entite), Some(&Hidden));
        assert_eq!(world.get::<RelicSlotUI>(entite), Some(&RelicSlotUI(2)));
        let punch = world.get::<PunchScale>(entite).expect("PunchScale posé");
        assert_eq!(punch.base_scale, Vec3::ONE);
        assert_eq!(punch.offset, 0.2);
        assert_eq!(punch.velocity, 0.0);
        assert_eq!(punch.elasticity, 0.5);
        assert_eq!(punch.decay, 0.8);
    }

    #[test]
    fn test_die_view_order_is_a_rank_not_an_id() {
        // `order` décrit une place à l'écran, `DieId` une identité. Permuter
        // les rangs ne renomme personne.
        let mut world = World::new();
        let a = world
            .spawn((Die::new(DieId(7), 6), DieView { order: 0 }))
            .id();
        let b = world
            .spawn((Die::new(DieId(9), 6), DieView { order: 1 }))
            .id();

        world.get_mut::<DieView>(a).expect("rang de a").order = 1;
        world.get_mut::<DieView>(b).expect("rang de b").order = 0;

        assert_eq!(world.get::<Die>(a).expect("dé a").id, DieId(7));
        assert_eq!(world.get::<Die>(b).expect("dé b").id, DieId(9));
        assert_eq!(world.get::<DieView>(a).expect("rang de a").order, 1);
        assert_eq!(world.get::<DieView>(b).expect("rang de b").order, 0);
    }

    #[test]
    fn test_many_die_views_coexist() {
        // Filet de sécurité du piège des rôles ECS. Mesuré : un type promu en
        // ressource ne perd pas ses entités — elles survivent toutes — mais
        // une seule garde le composant, et c'est la **première** posée. Ce test
        // compte donc les composants, jamais les entités : compter ces
        // dernières rendrait six sur six et ne mordrait sur rien.
        let mut world = World::new();
        let entites: Vec<Entity> = (0..6)
            .map(|rang| world.spawn(DieView { order: rang }).id())
            .collect();

        let portent = entites
            .iter()
            .filter(|entite| world.get::<DieView>(**entite).is_some())
            .count();

        assert_eq!(
            portent, 6,
            "un seul rang survit : le type est-il devenu une ressource ?"
        );
        for (rang, entite) in entites.iter().enumerate() {
            let vue = world.get::<DieView>(*entite).expect("rang présent");
            assert_eq!(
                vue.order,
                u8::try_from(rang).expect("six rangs tiennent dans un u8")
            );
        }
    }

    #[test]
    fn test_relic_slot_ui_carries_only_an_index() {
        // Le test n'affirme que ce qui se vérifie à l'exécution : l'index se
        // relit tel quel. Qu'il n'y ait **qu'un** champ est tenu par le type
        // lui-même et par la garde de la DoD interdisant toute identité de
        // relique dans ce fichier, pas par une assertion.
        let mut world = World::new();
        let entite = world.spawn(RelicSlotUI(2)).id();

        let slot = world.get::<RelicSlotUI>(entite).expect("slot posé");
        assert_eq!(slot.0, 2);
        assert_eq!(*slot, RelicSlotUI(2));
    }

    #[test]
    fn test_components_are_not_resources() {
        // La preuve **négative** est le couple de doc-tests en tête de fichier :
        // le bloc à borne `Component` compile, celui à borne `Resource` non, et
        // ils ne diffèrent que par cette borne. Ici, la contrepartie positive.
        fn exige_component<T: Component>() {}
        exige_component::<DieView>();
        exige_component::<Locked>();
        exige_component::<Scoring>();
        exige_component::<Hidden>();
        exige_component::<RelicSlotUI>();
        exige_component::<PunchScale>();
    }
}
