//! Le matériau du vortex d'arrière-plan et son bloc d'uniformes.
//!
//! **Le type, son bloc, et rien d'autre** (TASK-84). Le shader WGSL et le quad
//! plein écran sont TASK-85, les quatre palettes et la conversion sRGB →
//! linéaire TASK-86, l'interpolation sur 1,5 s TASK-87. Aucune couleur n'est
//! écrite ici, aucun `impl Default` : un défaut qui porterait une palette
//! ferait de ce fichier une seconde source de couleurs.
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
use bevy::math::Vec2;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::Material2d;

use super::plugin::PSYCHE_BACKGROUND_SHADER;

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
