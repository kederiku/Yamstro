//! `VisualEffectsPlugin` : le point d'ancrage des effets visuels.
//!
//! Depuis TASK-83, il pose les deux ressources de réglages de l'étape,
//! `CrtSettings` et `SafeMode`, déclarées dans `settings.rs` à côté de
//! `JuiceSettings` : il ne les possède pas, il les insère.
//!
//! # Trois responsabilités de rendu, la deuxième peuplée depuis TASK-84
//!
//! 1. **Les trois shaders.** Le plugin publie leurs chemins, relatifs à la
//!    racine `assets/` du dépôt, en une seule source : les matériaux de
//!    TASK-84, TASK-88 et TASK-90 les désignent par `ShaderRef::Path`, et le
//!    repli de TASK-92 les surveille. Il ne les **charge** pas lui-même, et
//!    c'est une contrainte du moteur, pas un choix : une poignée forte lâchée
//!    décharge l'asset dès la fin de son chargement, et le type `Shader` n'est
//!    enregistré que par le plugin de rendu, si bien qu'un `load::<Shader>`
//!    sous `MinimalPlugins` plus le serveur d'assets fait paniquer ce dernier
//!    quand il traite l'événement d'échec d'un type inconnu. Retenir les
//!    poignées et lire `LoadState::Failed` appartient à TASK-92.
//! 2. **L'enregistrement des matériaux.** `build` est le point
//!    d'enregistrement des `Material2dPlugin` : le fond y est branché
//!    (TASK-84), TASK-88 y branche le filtre cathodique, TASK-90 le contour.
//!    Chaque matériau est un `Asset`, et `Material2dPlugin::build` appelle
//!    `init_asset`, qui lit `AssetServer`
//!    (`bevy_asset-0.19.1/src/lib.rs:639`) : **ce plugin exige le serveur
//!    d'assets**, et les tests headless de l'étape montent `AssetPlugin`, sans
//!    rendu ni fenêtre.
//! 3. **Le placement des passes.** Le vortex et le contour sont des `Material2d`
//!    rendus en `MainPass` ; le filtre cathodique va dans
//!    `Core2dSystems::PostProcess`, après le tonemapping, jamais dans
//!    `EarlyPostProcess` ni dans la prépasse 2D. TASK-88 remplit ce placement.
//!
//! # Chemins d'import, relevés dans les sources 0.19.1
//!
//! `Material2d`, `Material2dPlugin` et `MeshMaterial2d` vivent dans la crate
//! `bevy_sprite_render`, ré-exportée sous `bevy::sprite_render` ; `Mesh2d` vit
//! dans `bevy_mesh`, sous `bevy::mesh`. La crate `bevy_material`, extraite en
//! 0.19, n'en contient aucun : elle porte la couche commune aux matériaux 2D et
//! 3D. **`bevy_sprite_render` n'est pas dans l'arbre de cette crate** avec la
//! liste de features de TASK-42, mesuré par `cargo tree` : le ticket qui
//! déclarera le premier matériau devra ajouter la feature du même nom, et le
//! justifier.

use bevy::prelude::*;
use bevy::sprite_render::Material2dPlugin;

use super::background::{BackgroundMaterial, resize_background_quad, spawn_background_quad};
use crate::settings::{CrtSettings, SafeMode};

/// Le vortex d'arrière-plan, rendu en `MainPass` sur le quad de fond.
pub const PSYCHE_BACKGROUND_SHADER: &str = "shaders/psyche_background.wgsl";

/// Le filtre cathodique plein écran, set `PostProcess` de la 2D.
pub const CRT_POSTPROCESS_SHADER: &str = "shaders/crt_postprocess.wgsl";

/// Le contour et le balayage irisé des dés et des cartes de relique.
pub const HOLO_CARD_SHADER: &str = "shaders/holo_card.wgsl";

/// Les trois chemins, dans l'ordre du document source.
///
/// Relatifs à la racine `assets/`, la forme qu'un `ShaderRef::Path` attend. La
/// racine elle-même est résolue par le binaire, jamais par cette crate :
/// variable `BEVY_ASSET_ROOT`, sinon le manifeste du binaire lancé par cargo,
/// sinon le dossier de l'exécutable.
pub const SHADER_PATHS: [&str; 3] = [
    PSYCHE_BACKGROUND_SHADER,
    CRT_POSTPROCESS_SHADER,
    HOLO_CARD_SHADER,
];

/// Plugin des effets visuels.
///
/// Il se monte sans rendu ni fenêtre, mais pas sans serveur d'assets : le test
/// headless de l'étape le vérifie sous `MinimalPlugins` plus `AssetPlugin`.
/// Son seul état est les deux ressources de réglages qu'il insère et les
/// matériaux qu'il enregistre.
#[derive(Debug, Clone, Copy, Default)]
pub struct VisualEffectsPlugin;

impl Plugin for VisualEffectsPlugin {
    fn build(&self, app: &mut App) {
        // Les deux ressources de réglages de l'étape, dès maintenant : le
        // filtre allumé à pleine intensité, le mode dégradé éteint. TASK-88 et
        // TASK-92 les lisent par le prédicat `is_active`, jamais par le
        // drapeau lui-même.
        app.init_resource::<CrtSettings>();
        app.init_resource::<SafeMode>();

        // Point d'enregistrement des `Material2dPlugin`, une ligne par
        // matériau : le fond (TASK-84) ; TASK-88 y branche le filtre, TASK-90
        // le contour.
        app.add_plugins(Material2dPlugin::<BackgroundMaterial>::default());

        // Le quad de fond (TASK-85) : spawné une fois au démarrage, redimensionné
        // en place sur `WindowResized`, sans jamais réécrire son `Transform`.
        app.add_systems(Startup, spawn_background_quad);
        app.add_systems(Update, resize_background_quad);
    }
}
