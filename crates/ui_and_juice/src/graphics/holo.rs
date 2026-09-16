//! Le matériau de contour holographique, son bloc d'uniformes, la banque de
//! variantes pré-construites (TASK-90), l'habillage des dés et le choix de
//! leur variante par échange de handle (TASK-91), et le contour uni du mode
//! dégradé (TASK-92).
//!
//! # Le mode dégradé des dés
//!
//! En mode dégradé, la banque n'est pas construite et aucun
//! `HoloOutlineMaterial` n'existe : un dé porte un `Sprite` uni de la couleur
//! de son état, sans pulsation ni balayage, et `Hidden` donne le carré gris
//! neutre, le dos, sans que la valeur soit lue. **La banque absente est le
//! mode dégradé des dés** : les deux systèmes lisent `Option<Res<HoloMaterials>>`
//! et choisissent la tenue sur sa présence, sans lire le mode. Repos et survol
//! partagent la même couleur unie : le survol n'est pas une information de
//! jeu, TASK-94 calibrera. La bascule dépouille les dés, et l'habillage les
//! rhabille à la frame suivante dans la tenue du moment.
//!
//! # Raccord C : le rendu ne lit jamais la valeur d'un dé
//!
//! Le choix de variante lit les marqueurs `Hidden` et `Scoring`, le survol,
//! le sceau et les modificateurs du dé : jamais sa valeur, ni pour choisir une
//! texture, ni pour teinter, ni pour journaliser, et la CI l'interdit dans
//! tout `graphics/`. Sous le masque, c'est le shader qui dessine un dos
//! neutre sans échantillonner la face. Le masquage prime sur tout : un dé
//! caché et scoré reste caché, quel que soit l'ordre des marqueurs.
//!
//! # L'échange
//!
//! Les dés n'ont aucun composant visuel à leur spawn (`game_state`, qui ne
//! connaît pas cette crate) : `dress_dice` pose sur chaque `DieView` un quad
//! partagé et le handle de sa variante, sans écrire de `Transform`, celui que
//! `Mesh2d` requiert naissant à l'identité, la disposition des dés n'étant pas
//! de cette étape. `sync_die_outline` réconcilie ensuite chaque frame la
//! variante voulue avec le handle porté, et n'écrit le composant que si le
//! handle diffère : la pose et le retrait de `Scoring`, `Hidden` et le survol
//! sont suivis par construction, la restauration est gratuite, et rien ne
//! bouge au repos. Un écrit de composant, jamais un `AssetEvent::Modified`.
//! Le survol est `bevy_ui::Interaction`, l'API des cartes de relique, sans
//! feature de plus : le système de focus ne touche pas les entités sans
//! `Node`, un futur picking ou une couche UI la posera.
//!
//! Les cartes de relique sont des nœuds UI : un `Material2d` ne les habille
//! pas, et « un handle par carte » attendra un rendu qui l'admette.
//!
//! # Forme A, et une texture de face
//!
//! Un seul `#[uniform(0)]`, sur un champ unique portant un `ShaderType`, comme
//! le fond de TASK-84. La texture de face et son échantillonneur sont
//! `#[texture(1)]` et `#[sampler(2)]`, l'écriture du `ColorMaterial` de Bevy
//! (`bevy_sprite_render-0.19.1/src/mesh2d/color_material.rs:43-45`) ; ce ne
//! sont pas un second uniforme. La texture est un `Option<Handle<Image>>` :
//! aucun atlas de faces n'existe dans le dépôt, et un `Option` à `None` se lie
//! sur l'image de repli de Bevy (`bevy_render-0.19.1/src/render_resource/
//! bind_group.rs:158-159`), là où un `Handle<Image>` d'un fichier absent ne
//! serait jamais préparé. L'atlas viendra avec les faces.
//!
//! # Trente-deux octets
//!
//! `LinearRgba` occupe les offsets 0 à 15 ; les trois `f32` tiennent en 16, 20
//! et 24 et laissent la struct à 28 ; `_pad: f32` à l'offset 28 la porte à 32,
//! multiple de 16. Les offsets sont en commentaire, et la struct WGSL de
//! TASK-91 en sera le miroir champ pour champ : un décalage ne produit aucune
//! erreur, il produit un contour d'une autre couleur. `mask_face` est un
//! `f32`, 0 ou 1, en miroir du WGSL ; il sélectionne une région de texture
//! dans un shader qui tourne de toute façon, ce n'est pas l'interrupteur
//! d'ordonnancement proscrit à TASK-88. Aucun champ de temps : pulsation et
//! balayage viennent de `globals.time` côté WGSL.
//!
//! # Raccord D : la banque
//!
//! Le contour se pilote par échange de handle, jamais par écriture de
//! matériau. Les huit variantes des dés, quatre états fois uni ou irisé, sont
//! construites une fois au démarrage et retenues dans `HoloMaterials` : huit
//! `AssetEvent::Added` au démarrage, jamais un `Modified`. `rainbow_shift`
//! est fixé à la construction et jamais réécrit. Les cartes de relique auront
//! chacune leur handle, à leur spawn, avec leur texture et leur rareté.
//!
//! # Valeurs de départ
//!
//! Le document nomme un contour blanc ou doré pulsant, un balayage irisé et un
//! dos neutre, sans rien chiffrer. Les couleurs et largeurs ci-dessous sont
//! des réglages de départ, en sRGB converti une fois en linéaire par
//! `Srgba::hex`, la règle de TASK-86 ; TASK-91 et TASK-94 les calibreront avec
//! le shader. Les dés n'ont aujourd'hui aucun composant visuel : TASK-91
//! devra leur poser un `Mesh2d` et un `MeshMaterial2d` avant d'échanger quoi
//! que ce soit.

use bevy::asset::Asset;
use bevy::color::{LinearRgba, Srgba};
use bevy::math::primitives::Rectangle;
use bevy::mesh::Mesh2d;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite::Sprite;
use bevy::sprite_render::{Material2d, MeshMaterial2d};
use bevy::ui::Interaction;
use core_engine::dice::Die;
use game_state::{DieView, Hidden, Scoring};

use super::plugin::HOLO_CARD_SHADER;
use crate::settings::SafeMode;

// ---- Forme A : un champ unique portant un ShaderType, et la texture de face.

/// Le matériau de contour des dés et des cartes de relique, rendu en
/// `MainPass`.
///
/// Un `Asset`, ni `Component` ni `Resource` : c'est `MeshMaterial2d` qui est le
/// composant, et il porte un `Handle`.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct HoloOutlineMaterial {
    /// Le bloc d'uniformes, en un seul binding.
    #[uniform(0)]
    pub params: HoloUniform,
    /// La texture de face : `None` tant qu'aucun atlas n'existe, et l'image de
    /// repli de Bevy se lie à sa place.
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
}

/// Le bloc d'uniformes du contour : 32 octets, aligné std140.
#[derive(ShaderType, Debug, Clone)]
pub struct HoloUniform {
    /// Couleur du contour.
    pub outline_color: LinearRgba, // offset  0, 16 o
    /// Largeur du contour, en unités du shader.
    pub outline_width: f32, // offset 16,  4 o
    /// Balayage irisé : 0 pour l'uni, 1 pour l'irisé, fixé à la construction.
    pub rainbow_shift: f32, // offset 20,  4 o
    /// Masque de face : 1 pour le dos neutre d'un dé caché, 0 sinon.
    pub mask_face: f32, // offset 24,  4 o
    /// Remplissage explicite, jamais lu par le shader.
    pub _pad: f32, // offset 28,  4 o  -> 32 o, multiple de 16
}
// PAS de champ `time` : il vient de globals.time côté WGSL (TASK-91).

impl Material2d for HoloOutlineMaterial {
    /// Le fragment du contour, désigné par le chemin que le plugin publie.
    fn fragment_shader() -> ShaderRef {
        HOLO_CARD_SHADER.into()
    }
}

/// La banque des variantes pré-construites des dés.
///
/// **`Resource` uniquement.** Indexée par `état * 2 + irisé as usize` : quatre
/// états, chacun en uni et en irisé, huit handles bâtis une fois au démarrage
/// et seulement échangés ensuite.
#[derive(Resource, Debug, Clone)]
pub struct HoloMaterials {
    /// Indexé par `état * 2 + irisé as usize`.
    pub dice: [Handle<HoloOutlineMaterial>; 8],
}

impl HoloMaterials {
    /// Repos.
    pub const IDLE: usize = 0;
    /// Survol.
    pub const HOVER: usize = 1;
    /// Marqueur `Scoring`.
    pub const SCORING: usize = 2;
    /// Marqueur `Hidden`, la seule variante à `mask_face == 1.0`.
    pub const HIDDEN: usize = 3;

    /// `state` dans `IDLE..=HIDDEN`, `iridescent` selon `DieSeal` ou
    /// `DieModifier`.
    #[must_use]
    pub fn die(&self, state: usize, iridescent: bool) -> &Handle<HoloOutlineMaterial> {
        &self.dice[state * 2 + iridescent as usize]
    }

    /// Construit la banque : huit matériaux insérés, huit handles retenus, le
    /// balayage irisé ne différant que par `rainbow_shift`. Appelée au
    /// démarrage, et à chaque sortie du mode dégradé.
    #[must_use]
    pub fn build(materials: &mut Assets<HoloOutlineMaterial>) -> Self {
        let variants = starting_variants();
        let dice = core::array::from_fn(|index| {
            let (outline_color, outline_width, mask_face) = variants[index / 2];
            let rainbow_shift = if index % 2 == 1 { 1.0 } else { 0.0 };
            materials.add(HoloOutlineMaterial {
                params: HoloUniform {
                    outline_color,
                    outline_width,
                    rainbow_shift,
                    mask_face,
                    _pad: 0.0,
                },
                texture: None,
            })
        });
        Self { dice }
    }
}

/// La couleur de contour d'un état, celle de sa variante de départ : c'est
/// aussi la couleur unie du dé en mode dégradé.
#[must_use]
pub fn outline_color(state: usize) -> LinearRgba {
    starting_variants()[state].0
}

/// Les réglages de départ de chaque état, dans l'ordre des index : couleur,
/// largeur, masque de face. Le `expect` sur la conversion est admis là et
/// seulement là : les chaînes sont littérales, et le test de la banque les
/// parcourt à chaque exécution.
fn starting_variants() -> [(LinearRgba, f32, f32); 4] {
    [
        (
            Srgba::hex("#FFFFFF").expect("littéral sRGB").into(),
            1.0,
            0.0,
        ),
        (
            Srgba::hex("#FFFFFF").expect("littéral sRGB").into(),
            2.5,
            0.0,
        ),
        (
            Srgba::hex("#FFD54A").expect("littéral sRGB").into(),
            3.0,
            0.0,
        ),
        (
            Srgba::hex("#9AA0A6").expect("littéral sRGB").into(),
            1.5,
            1.0,
        ),
    ]
}

/// Construit la banque des dés, une fois, au démarrage ; rien en mode
/// dégradé, où aucun `HoloOutlineMaterial` ne doit exister.
pub fn build_holo_bank(
    mut commands: Commands,
    mut materials: ResMut<Assets<HoloOutlineMaterial>>,
    safe_mode: Res<SafeMode>,
) {
    if safe_mode.is_engaged() {
        return;
    }
    commands.insert_resource(HoloMaterials::build(&mut materials));
}

/// Le côté du quad d'un dé, en pixels logiques : une valeur de départ, la
/// disposition des dés n'étant pas de cette étape.
pub const DIE_QUAD_SIZE: f32 = 64.0;

/// L'état de contour d'un dé, dans l'ordre de priorité : le masquage d'abord,
/// toujours, puis le marqueur de score, puis le survol.
#[must_use]
pub fn outline_state(hidden: bool, scoring: bool, hovered: bool) -> usize {
    if hidden {
        return HoloMaterials::HIDDEN;
    }
    if scoring {
        return HoloMaterials::SCORING;
    }
    if hovered {
        HoloMaterials::HOVER
    } else {
        HoloMaterials::IDLE
    }
}

/// Un dé porte le balayage irisé s'il a un sceau ou un modificateur.
#[must_use]
pub fn is_iridescent(die: &Die) -> bool {
    die.seal.is_some() || !die.modifiers.is_empty()
}

fn is_hovered(interaction: Option<&Interaction>) -> bool {
    matches!(
        interaction,
        Some(Interaction::Hovered | Interaction::Pressed)
    )
}

/// Ce que lit le choix de variante : les deux marqueurs et le survol, tous
/// optionnels, et jamais la valeur du dé.
type OutlineInputs<'a> = (
    Option<&'a Hidden>,
    Option<&'a Scoring>,
    Option<&'a Interaction>,
);

/// Un dé vu mais pas encore habillé, ni par la banque ni à plat.
type Undressed = (With<DieView>, Without<Mesh2d>, Without<Sprite>);

/// Les dés habillés par la banque.
type HoloDressed = (With<DieView>, With<MeshMaterial2d<HoloOutlineMaterial>>);

/// Les dés habillés à plat, en mode dégradé.
type FlatDressed = (With<DieView>, With<Sprite>);

/// La tenue d'un dé, l'une ou l'autre, jamais les deux.
type DieDress<'a> = (
    Option<&'a mut MeshMaterial2d<HoloOutlineMaterial>>,
    Option<&'a mut Sprite>,
);

/// Le carré uni d'un dé en mode dégradé.
fn flat_die(state: usize) -> Sprite {
    Sprite::from_color(outline_color(state), Vec2::splat(DIE_QUAD_SIZE))
}

/// Dépouille un dé de sa tenue holographique ; `dress_dice` le rhabille.
fn undress_holo(commands: &mut Commands, die: Entity) {
    commands
        .entity(die)
        .remove::<(Mesh2d, MeshMaterial2d<HoloOutlineMaterial>)>();
}

/// Dépouille un dé de sa tenue plate ; `dress_dice` le rhabille.
fn undress_flat(commands: &mut Commands, die: Entity) {
    commands.entity(die).remove::<Sprite>();
}

fn state_of((hidden, scoring, interaction): OutlineInputs<'_>) -> usize {
    outline_state(hidden.is_some(), scoring.is_some(), is_hovered(interaction))
}

/// Habille chaque dé nouvellement vu : avec la banque, un quad partagé, bâti
/// une fois, et le handle de sa variante ; sans elle, le carré uni de son
/// état. Aucun `Transform` n'est écrit.
pub fn dress_dice(
    mut commands: Commands,
    bank: Option<Res<HoloMaterials>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut quad: Local<Option<Handle<Mesh>>>,
    dice: Query<(Entity, &Die, OutlineInputs<'_>), Undressed>,
) {
    for (entity, die, inputs) in &dice {
        let state = state_of(inputs);
        let Some(bank) = bank.as_deref() else {
            commands.entity(entity).insert(flat_die(state));
            continue;
        };
        let quad = quad
            .get_or_insert_with(|| {
                meshes.add(Mesh::from(Rectangle::new(DIE_QUAD_SIZE, DIE_QUAD_SIZE)))
            })
            .clone();
        let material = bank.die(state, is_iridescent(die)).clone();
        commands
            .entity(entity)
            .insert((Mesh2d(quad), MeshMaterial2d(material)));
    }
}

/// Réconcilie la tenue de chaque dé avec ses marqueurs et son survol : le
/// handle avec la banque, la couleur unie sans elle, et n'écrit que si la
/// valeur diffère.
pub fn sync_die_outline(
    bank: Option<Res<HoloMaterials>>,
    mut dice: Query<(&Die, DieDress<'_>, OutlineInputs<'_>), With<DieView>>,
) {
    for (die, (material, sprite), inputs) in &mut dice {
        let state = state_of(inputs);
        match (bank.as_deref(), material, sprite) {
            (Some(bank), Some(mut material), _) => {
                let target = bank.die(state, is_iridescent(die));
                if material.0.id() != target.id() {
                    material.0 = target.clone();
                }
            }
            (None, _, Some(mut sprite)) => {
                let target = Color::from(outline_color(state));
                if sprite.color != target {
                    sprite.color = target;
                }
            }
            _ => {}
        }
    }
}

/// Les dés, sur changement du mode (TASK-92) : engagé, la banque tombe avec
/// ses huit matériaux et les dés sont dépouillés de leur tenue
/// holographique ; désengagé, la banque est reconstruite, une fois, et les
/// dés dépouillés de leur tenue plate. `dress_dice` les rhabille à la frame
/// suivante. Rien n'est fait si l'état est déjà le bon.
pub fn apply_safe_mode_to_dice(
    mut commands: Commands,
    safe_mode: Res<SafeMode>,
    bank: Option<Res<HoloMaterials>>,
    mut materials: ResMut<Assets<HoloOutlineMaterial>>,
    holo_dice: Query<Entity, HoloDressed>,
    flat_dice: Query<Entity, FlatDressed>,
) {
    if safe_mode.is_engaged() {
        if bank.is_some() {
            commands.remove_resource::<HoloMaterials>();
        }
        for die in &holo_dice {
            undress_holo(&mut commands, die);
        }
    } else {
        if bank.is_none() {
            commands.insert_resource(HoloMaterials::build(&mut materials));
        }
        for die in &flat_dice {
            undress_flat(&mut commands, die);
        }
    }
}
