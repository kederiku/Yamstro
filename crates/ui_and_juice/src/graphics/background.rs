//! Le matériau du vortex d'arrière-plan, son bloc d'uniformes, le quad qui le
//! porte, son redimensionnement, et son repli plat.
//!
//! Le type et son bloc sont TASK-84 ; le spawn du quad et le redimensionnement
//! sont TASK-85, le shader vivant sous `assets/shaders/`. Les quatre palettes
//! et la conversion sRGB → linéaire sont TASK-86, l'interpolation sur 1,5 s
//! TASK-87. Aucune couleur n'est écrite ici, aucun `impl Default` : le spawn
//! part de la palette de la Petite Mise, `theme::SMALL`, la palette hors run.
//!
//! # Le mode dégradé du fond (TASK-92)
//!
//! Une seule entité `BackgroundQuad`, toujours, qui porte soit le maillage et
//! son `BackgroundMaterial`, soit un `Sprite` uni aux dimensions logiques de
//! la fenêtre, jamais les deux. Au démarrage, le spawn lit le mode et choisit
//! la forme : en mode dégradé, **aucun `BackgroundMaterial` n'est instancié**.
//! La bascule échange les composants sur place, par [`flatten_background`] et
//! [`restore_background`] ; les assets tombent avec leurs derniers handles.
//! La couleur de l'aplat suit la palette cible, sans interpolation, écrite
//! par le thème seulement quand la cible change.
//!
//! # Raccord A : le quad ne s'agrandit pas par son `Transform`
//!
//! L'audit de l'Étape 4 admet exactement deux écrivains de `Transform` dans
//! cette crate, le ressort et la secousse de caméra, et la CI interdit toute
//! requête mutable ailleurs. Le `Transform` du quad est posé une fois au spawn
//! et n'est jamais réécrit : le redimensionnement **reconstruit le rectangle
//! dans `Assets<Mesh>`**, en place, sans échanger le handle. L'échange créerait
//! un `Mesh` par événement, et un glissement de bord de fenêtre en émet des
//! dizaines par seconde ; l'écriture en place produit un
//! `AssetEvent::Modified<Mesh>` par événement, rare, borné, et hors du compteur
//! du critère § 4.3, qui porte sur le matériau.
//!
//! # Unités
//!
//! Le rectangle est dimensionné en pixels logiques : `WindowResized` porte la
//! taille logique (`bevy_window-0.19.1/src/event.rs:39`), et la projection 2D
//! par défaut est `ScalingMode::WindowSize`, une unité monde par pixel logique
//! (`bevy_camera-0.19.1/src/projection.rs:529`), avec `near = -1000`
//! (`:773`) : le quad à `z = -100` est dans le champ. Sans fenêtre primaire,
//! le rectangle fait 1 × 1 et attend le premier `WindowResized`.
//!
//! # Chemins d'import, relevés dans les sources 0.19.1
//!
//! - `Material2d` et `Material2dPlugin` : crate `bevy_sprite_render`,
//!   `src/mesh2d/material.rs:136` et `:268`, ré-exportés à la racine de la
//!   crate (`src/lib.rs:30`), donc `bevy::sprite_render::Material2d`. La
//!   feature `bevy_sprite_render` de la façade est requise ; absente de la
//!   liste de TASK-42, ce ticket l'ajoute au manifeste de cette crate.
//! - `ShaderRef` : crate `bevy_shader`, `src/shader.rs:406`, ré-exportée
//!   `bevy::shader::ShaderRef` ; `From<&'static str>` construit la variante
//!   `Path`.
//! - `AsBindGroup` et `ShaderType` : `bevy::render::render_resource`
//!   (`bevy_render-0.19.1/src/render_resource/bind_group.rs:10` et
//!   `mod.rs:73`).
//! - `LinearRgba` implémente `ShaderType` parce que la feature `bevy_render`
//!   active `bevy_color/encase` (`bevy_internal-0.19.1/Cargo.toml:158`) ;
//!   `Vec2` de même, par `glam/encase` (`bevy_render-0.19.1/Cargo.toml:178`).
//! - `bevy_material`, extraite en 0.19, ne porte aucun de ces types.
//!
//! # Alignement std140
//!
//! Les blocs font seize octets. Trois `LinearRgba` occupent les offsets 0, 16
//! et 32 ; les deux `f32` qui suivent tiennent en 48 et 52 et laissent la
//! struct à 56 octets, d'où `_pad: Vec2` à l'offset 56, qui la porte à 64,
//! multiple de 16. Le padding est explicite des deux côtés : la struct WGSL de
//! TASK-85 est écrite en miroir, champ pour champ, offsets en commentaire.
//! **Un décalage d'alignement ne produit aucune erreur**, ni panique, ni
//! avertissement, ni ligne de log : il produit des couleurs fausses, qu'une
//! revue visuelle prend pour un choix artistique.
//!
//! Ce que le test de taille voit, mesuré au banc de mutation : un bloc de
//! 80 octets (un `Vec4` à la place du `Vec2`), pas le retrait de `_pad`.
//! `encase` arrondit la taille d'une struct à son alignement, 16 ici, celui
//! des `LinearRgba` : avec ou sans `_pad`, `min_size()` rend 64. Le
//! remplissage explicite est tenu par la CI, et son rôle est le miroir WGSL,
//! relu champ pour champ. Ce qu'aucun test de taille ne voit, c'est l'ordre
//! des champs : un bloc Rust et un bloc WGSL de 64 octets aux champs permutés
//! ont la même taille et des couleurs fausses. C'est TASK-85 qui tient le
//! miroir.

use bevy::asset::Asset;
use bevy::color::LinearRgba;
use bevy::ecs::query::Has;
use bevy::math::Vec2;
use bevy::math::primitives::Rectangle;
use bevy::mesh::Mesh2d;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite::Sprite;
use bevy::sprite_render::{Material2d, MeshMaterial2d};
use bevy::window::{PrimaryWindow, Window, WindowResized};

use super::plugin::PSYCHE_BACKGROUND_SHADER;
use super::theme::{SMALL, VisualThemeController};
use crate::settings::SafeMode;

// ---- Forme A (retenue pour ce projet) : un champ unique portant un ShaderType.

/// Le matériau du vortex, rendu en `MainPass` sur le quad de fond.
///
/// Un `Asset`, ni `Component` ni `Resource` : c'est `MeshMaterial2d` qui est le
/// composant, et il porte un `Handle`. **Un seul `#[uniform(0)]`** : répéter
/// l'attribut sur plusieurs champs n'agrège pas ces champs dans un même
/// uniform buffer. La forme B, l'attribut au niveau de la struct avec un
/// `From<&BackgroundMaterial>` qui pose `_pad`, est valide mais non retenue ;
/// jamais un mélange des deux.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct BackgroundMaterial {
    /// Le bloc d'uniformes, en un seul binding.
    #[uniform(0)]
    pub params: BackgroundUniform,
}

/// Le bloc d'uniformes du vortex : 64 octets, aligné std140.
#[derive(ShaderType, Debug, Clone)]
pub struct BackgroundUniform {
    /// Couleur primaire de la palette.
    pub primary_color: LinearRgba, // offset  0, 16 o
    /// Couleur secondaire.
    pub secondary_color: LinearRgba, // offset 16, 16 o
    /// Couleur d'accent.
    pub accent_color: LinearRgba, // offset 32, 16 o
    /// Vitesse de l'animation, appliquée à `globals.time` côté WGSL.
    pub speed: f32, // offset 48,  4 o
    /// Facteur de distorsion de l'espace UV.
    pub swirl_factor: f32, // offset 52,  4 o
    /// Remplissage explicite, jamais lu par le shader.
    pub _pad: Vec2, // offset 56,  8 o  -> 64 o, multiple de 16
}
// PAS de champ `time` : il vient de globals.time côté WGSL.

impl Material2d for BackgroundMaterial {
    /// Le fragment du vortex, désigné par le chemin que le plugin publie :
    /// une seule orthographe du chemin dans la crate, gardée par la CI.
    fn fragment_shader() -> ShaderRef {
        PSYCHE_BACKGROUND_SHADER.into()
    }
}

/// Le marqueur de l'entité du fond : une seule, pour toute la partie.
///
/// Le redimensionnement lit son `Mesh2d`, et TASK-87 lira son
/// `MeshMaterial2d` pour prendre le `Handle<BackgroundMaterial>` que son
/// contrôleur de thème portera. TASK-92 garde le spawn derrière le mode
/// dégradé, où l'entité de fond est un dégradé plat sans matériau.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct BackgroundQuad;

/// Spawne l'entité du fond, une seule fois, au démarrage.
///
/// Le rectangle prend les dimensions logiques de la fenêtre primaire, le
/// matériau part de la Petite Mise, la palette hors run que TASK-87 fera
/// évoluer, et le `Transform` est posé ici, à
/// `z = -100`, derrière tout le reste, pour ne plus jamais être réécrit. Le
/// système est unique et nommé : TASK-92 le garde derrière le mode dégradé.
pub fn spawn_background_quad(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BackgroundMaterial>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    safe_mode: Res<SafeMode>,
) {
    let size = window_size(window);
    if safe_mode.is_engaged() {
        commands.spawn((
            BackgroundQuad,
            flat_background(size),
            background_transform(),
        ));
        return;
    }
    commands.spawn((
        BackgroundQuad,
        Mesh2d(meshes.add(background_mesh(size))),
        MeshMaterial2d(materials.add(starting_material())),
        background_transform(),
    ));
}

/// Les dimensions logiques de la fenêtre primaire, 1 × 1 sans fenêtre.
fn window_size(window: Option<Single<&Window, With<PrimaryWindow>>>) -> Vec2 {
    window.map_or(Vec2::ONE, |w| Vec2::new(w.width(), w.height()))
}

/// Le `Transform` du fond, posé une fois : `z = -100`, derrière tout le reste.
fn background_transform() -> Transform {
    Transform::from_xyz(0.0, 0.0, -100.0)
}

/// Le rectangle du fond aux dimensions données.
fn background_mesh(size: Vec2) -> Mesh {
    Mesh::from(Rectangle::new(size.x, size.y))
}

/// Le matériau de départ, la Petite Mise.
fn starting_material() -> BackgroundMaterial {
    BackgroundMaterial {
        params: BackgroundUniform::from(*SMALL),
    }
}

/// L'aplat du mode dégradé : un `Sprite` uni, aucun matériau.
fn flat_background(size: Vec2) -> Sprite {
    Sprite::from_color(SMALL.primary, size)
}

/// Le repli du fond : le quad perd maillage et matériau et porte l'aplat.
fn flatten_background(commands: &mut Commands, quad: Entity, size: Vec2) {
    commands
        .entity(quad)
        .remove::<(Mesh2d, MeshMaterial2d<BackgroundMaterial>)>()
        .insert(flat_background(size));
}

/// Le retour du vortex : l'aplat tombe, maillage et matériau sont créés une
/// fois ; rend le handle du matériau, pour le contrôleur de thème.
fn restore_background(
    commands: &mut Commands,
    quad: Entity,
    size: Vec2,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<BackgroundMaterial>,
) -> Handle<BackgroundMaterial> {
    let material = materials.add(starting_material());
    commands.entity(quad).remove::<Sprite>().insert((
        Mesh2d(meshes.add(background_mesh(size))),
        MeshMaterial2d(material.clone()),
    ));
    material
}

/// Reconstruit le rectangle du fond en place sur `WindowResized`.
///
/// Le dernier message de la frame fait foi ; le maillage est réécrit dans
/// `Assets<Mesh>` sous le même handle, jamais échangé. Idempotent : deux
/// messages de même taille produisent le même maillage. Aucun `Transform`
/// n'est lu ni écrit.
pub fn resize_background_quad(
    mut resized: MessageReader<WindowResized>,
    mut quads: Query<(Option<&Mesh2d>, Option<&mut Sprite>), With<BackgroundQuad>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(last) = resized.read().last() else {
        return;
    };
    let size = Vec2::new(last.width, last.height);
    for (quad, sprite) in &mut quads {
        if let Some(quad) = quad
            && let Some(mut mesh) = meshes.get_mut(&quad.0)
        {
            *mesh = background_mesh(size);
        }
        if let Some(mut sprite) = sprite
            && sprite.custom_size != Some(size)
        {
            sprite.custom_size = Some(size);
        }
    }
}

/// Le fond, sur changement du mode (TASK-92) : engagé, le quad perd
/// maillage et matériau pour l'aplat et le contrôleur de thème tombe ;
/// désengagé, l'aplat tombe, maillage et matériau reviennent, une fois, et
/// le contrôleur renaît avec le handle. Rien n'est fait si l'état est déjà
/// le bon : idempotent par constat, pas par mémoire.
pub fn apply_safe_mode_to_background(
    mut commands: Commands,
    safe_mode: Res<SafeMode>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    background: Query<(Entity, Has<Mesh2d>), With<BackgroundQuad>>,
    controller: Option<Res<VisualThemeController>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BackgroundMaterial>>,
) {
    let engaged = safe_mode.is_engaged();
    let size = window_size(window);
    for (quad, has_mesh) in &background {
        if engaged && has_mesh {
            flatten_background(&mut commands, quad, size);
        } else if !engaged && !has_mesh {
            let handle = restore_background(&mut commands, quad, size, &mut meshes, &mut materials);
            commands.insert_resource(VisualThemeController::at_rest(*SMALL, handle));
        }
    }
    if engaged && controller.is_some() {
        commands.remove_resource::<VisualThemeController>();
    }
}
