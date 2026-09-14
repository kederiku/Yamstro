//! Hiérarchie de la boutique : quatre cartes, deux boutons, un solde.
//!
//! # Ce que les cartes ne portent pas
//!
//! Une carte porte **un rang, et rien d'autre**. Ni article, ni identifiant de
//! relique, ni prix, ni rareté : elle les lit dans l'étalage et par le barème.
//! Un article recopié sur l'entité serait une seconde source de vérité,
//! désynchronisée dès la première relance, **sans erreur de compilation** et
//! sans test qui le voie hors de celui qui inspecte l'archétype.
//!
//! # Le nom et la description manquent, et c'est voulu
//!
//! Le corpus n'a **aucun** texte joueur : l'internationalisation est un
//! livrable de l'Étape 9, aucun identifiant ne porte de chaîne, et le catalogue
//! des boss a explicitement refusé d'écrire les noms français dans le code.
//! Les emplacements existent donc, **vides**, et l'Étape 9 les remplira depuis
//! ses ressources. Y mettre des chaînes provisoires serait une dette à défaire ;
//! y afficher le `{:?}` d'un identifiant ferait fuir un nom de développeur.
//!
//! # La preuve est mécanique, pas nominale
//!
//! **Un test qui inspecte l'archétype par le nom des composants ne prouve
//! rien** : hors de la feature `debug`, chaque nom sort
//! `"<Enable the debug feature to see the name>"`, et l'assertion passe sur une
//! carte portant tout ce qu'elle devrait refuser. Mesuré.
//!
//! La vraie garantie est plus forte, et elle est à la compilation : aucun des
//! types de jeu n'est un `Component`, donc **aucun ne peut être attaché à une
//! entité**. La paire de doc-tests ci-dessous l'établit. Ils ne diffèrent que
//! par le type, ce qui est la seule façon de montrer qu'un `compile_fail`
//! échoue pour la bonne raison : sans son jumeau positif, il passe aussi bien
//! sur une faute de frappe.
//!
//! ```
//! fn exige_component<T: bevy::ecs::component::Component>() {}
//! exige_component::<shop_system::ui::ShopCardUI>();
//! ```
//!
//! Un par bloc : groupés, un seul type promu les laisserait au vert, les
//! autres suffisant à faire échouer la compilation.
//!
//! ```compile_fail
//! fn exige_component<T: bevy::ecs::component::Component>() {}
//! exige_component::<core_engine::shop::ShopItem>();
//! ```
//!
//! ```compile_fail
//! fn exige_component<T: bevy::ecs::component::Component>() {}
//! exige_component::<core_engine::relics::RelicId>();
//! ```
//!
//! ```compile_fail
//! fn exige_component<T: bevy::ecs::component::Component>() {}
//! exige_component::<core_engine::relics::RelicInstance>();
//! ```
//!
//! ```compile_fail
//! fn exige_component<T: bevy::ecs::component::Component>() {}
//! exige_component::<core_engine::relics::RelicRarity>();
//! ```
//!
//! # Placement
//!
//! Tout s'écrit sur `Node`, jamais sur `Transform` : la passe d'interface
//! calcule sa propre transformation, et y écrire n'aurait aucun effet.

use bevy::prelude::*;
use core_engine::shop::ShopInventory;
use core_engine::shop::pricing::price_of;

/// Le rang d'un article dans l'étalage. **`Component` uniquement.**
///
/// Une dérivation `Resource` posée ici compilerait parfaitement et ne
/// laisserait qu'**une seule** carte à l'écran, `Resource` étant un sous-trait
/// de `Component` et l'insertion d'une seconde copie despawnant la première.
/// Seul un comptage d'entités l'attrape.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShopCardUI(pub usize);

/// Le bouton de relance. Son libellé porte le coût courant.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RerollButton;

/// Le bouton de sortie. Sa transition appartient au ticket d'assemblage.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContinueButton;

/// Le solde d'or, que la mise en scène anime.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoldDisplay;

/// L'emplacement d'un texte joueur, laissé vide jusqu'à l'Étape 9.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerTextSlot;

/// Taille de texte de la boutique, en pixels logiques.
const TAILLE_TEXTE: f32 = 18.0;

/// Un texte d'interface, à la taille de la boutique.
///
/// `TextFont.font_size` exige un `FontSize`, et `font` un `FontSource` : ce ne
/// sont plus un flottant et une poignée nus depuis la migration vers Parley
/// (bevy_text 0.19.1, `src/text.rs`). La police reste celle par défaut.
fn texte(contenu: &str) -> (Text, TextFont) {
    (
        Text::new(contenu),
        TextFont {
            font_size: FontSize::Px(TAILLE_TEXTE),
            ..default()
        },
    )
}

/// Pose la hiérarchie de la boutique et rend la racine.
///
/// **Publique, et appelée depuis le ticket aval.** Ce ticket n'enregistre aucun
/// système : la pose se fera à l'entrée en boutique, depuis le système qui
/// génère l'étalage. Une fonction privée sans appelant serait du code mort,
/// refusé sous `-D warnings`.
pub fn spawn_shop_ui(commands: &mut Commands, inventory: &ShopInventory) -> Entity {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            children![],
        ))
        .with_children(|racine| {
            // Le solde. Le compteur est **celui de la mise en scène** : aucun
            // second compteur animé n'existe dans le projet.
            racine.spawn((
                GoldDisplay,
                ui_and_juice::animation::AnimatedNumber {
                    displayed: 0.0,
                    target: 0.0,
                    rate: 12.0,
                    decimals: 0,
                },
                texte(""),
            ));

            // Les quatre cartes. Le rang est la seule donnée portée.
            racine
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceEvenly,
                    ..default()
                })
                .with_children(|rangee| {
                    for index in 0..CARTES_PAR_ETALAGE {
                        let prix = inventory.items.get(index).map_or(0, price_of);
                        rangee
                            .spawn((
                                ShopCardUI(index),
                                Button,
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    ..default()
                                },
                            ))
                            .with_children(|carte| {
                                // Nom et description : vides jusqu'à l'Étape 9.
                                carte.spawn((PlayerTextSlot, texte("")));
                                carte.spawn((PlayerTextSlot, texte("")));
                                carte.spawn(texte(&prix.to_string()));
                            });
                    }
                });

            // Les deux boutons.
            racine
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                })
                .with_children(|barre| {
                    barre
                        .spawn((RerollButton, Button, Node::default()))
                        .with_children(|bouton| {
                            bouton.spawn(texte(&inventory.reroll_cost.to_string()));
                        });
                    barre
                        .spawn((ContinueButton, Button, Node::default()))
                        .with_children(|bouton| {
                            bouton.spawn((PlayerTextSlot, texte("")));
                        });
                });
        })
        .id()
}

/// Le nombre de cartes d'un étalage, tel que le générateur le produit.
const CARTES_PAR_ETALAGE: usize = 4;
