//! Le matériau de contour holographique, son bloc d'uniformes et la banque de
//! variantes pré-construites (TASK-90). Le shader et l'échange de handle sont
//! TASK-91, le contour uni du mode dégradé TASK-92. Aucun système de réaction
//! ici, ni sur `Scoring`, ni sur `Hidden`, ni sur le survol.
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
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::Material2d;

use super::plugin::HOLO_CARD_SHADER;

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
}

/// Les réglages de départ de chaque état, dans l'ordre des index : couleur,
/// largeur, masque de face. Le `.unwrap()` est admis là et seulement là : les
/// chaînes sont littérales, et le test de la banque les parcourt à chaque
/// exécution.
fn starting_variants() -> [(LinearRgba, f32, f32); 4] {
    [
        (Srgba::hex("#FFFFFF").unwrap().into(), 1.0, 0.0),
        (Srgba::hex("#FFFFFF").unwrap().into(), 2.5, 0.0),
        (Srgba::hex("#FFD54A").unwrap().into(), 3.0, 0.0),
        (Srgba::hex("#9AA0A6").unwrap().into(), 1.5, 1.0),
    ]
}

/// Construit la banque des dés, une fois, au démarrage.
///
/// Huit matériaux insérés dans `Assets<HoloOutlineMaterial>`, huit handles
/// retenus ; le balayage irisé ne diffère que par `rainbow_shift`. TASK-92
/// gardera ce système derrière le mode dégradé.
pub fn build_holo_bank(mut commands: Commands, mut materials: ResMut<Assets<HoloOutlineMaterial>>) {
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
    commands.insert_resource(HoloMaterials { dice });
}
