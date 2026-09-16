//! Le filtre cathodique : un `FullscreenMaterial` ordonné dans le set
//! `PostProcess` de la 2D, après le tonemapping, et absent des caméras quand
//! il est coupé (TASK-88). Le corps du shader est TASK-89.
//!
//! # Ce que l'API impose, relevé dans les sources 0.19.1
//!
//! `FullscreenMaterial` exige `Component + ExtractComponent + Clone + Copy +
//! ShaderType + WriteInto + Default`
//! (`bevy_core_pipeline-0.19.1/src/fullscreen_material.rs:77`) : **le
//! matériau est lui-même l'uniforme**, extrait comme composant de la caméra
//! et poussé dans un tampon dynamique par `UniformComponentPlugin` (`:52`).
//! Ni `Asset`, ni `AsBindGroup`, ni `#[uniform(0)]` : le bind group 0 est
//! fixé par le trait, texture d'écran en 0, sampler en 1, uniforme en 2
//! (`:142-147`). Le document source écrivait un asset ; c'est l'exception que
//! le backlog v2 avait consignée à TASK-82. `CrtMaterial` garde la forme du
//! bloc, un champ `params: CrtUniform`, et l'uniforme d'une struct à un seul
//! membre struct a exactement les seize octets de ce membre : le WGSL de
//! TASK-89 déclare `CrtUniform` à plat, champ pour champ, offsets en
//! commentaire. Le `Default` exigé dérive des zéros que personne n'emploie :
//! la synchronisation écrit toujours depuis les réglages.
//!
//! Les défauts du trait sont la 3D, set `PostProcess` **avant** le
//! tonemapping (`:86-96`). Ici : `Core2d`, `Core2dSystems::PostProcess`,
//! `.after(tonemapping)`, le tonemapping y tournant avant l'upscaling
//! (`core_2d/mod.rs:88-89`). Jamais le set précoce, jamais la prépasse 2D.
//!
//! # La passe n'est pas ordonnancée quand le filtre est coupé
//!
//! Le système de passe tourne par vue, sur les seules vues qui portent le
//! composant : sans lui, pipeline et bind groups sont retirés (`:195`,
//! `:245`) et il n'y a rien à dessiner. « Non ordonnancée » se dit donc par
//! l'absence du composant sur la caméra, décidée hors du monde de rendu par
//! la condition pure de TASK-83, `CrtSettings::is_active(&SafeMode)` : pas de
//! branchement sur le GPU, pas de coût, image strictement identique. Aucun
//! booléen ne transite par l'uniforme, et `graphics/` ne lit jamais le
//! drapeau lui-même.
//!
//! # Raccord D
//!
//! Le composant n'est réécrit que quand `CrtSettings` ou `SafeMode` a changé,
//! `intensity` multipliant les quatre intensités unitaires à l'écriture : le
//! joueur touche son slider quelques fois par partie, l'uniforme s'écrit
//! quelques fois par partie.

use bevy::camera::Camera2d;
use bevy::core_pipeline::fullscreen_material::FullscreenMaterial;
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core2d, Core2dSystems};
use bevy::ecs::schedule::{ScheduleConfigs, ScheduleLabel};
use bevy::ecs::system::BoxedSystem;
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;

use super::plugin::CRT_POSTPROCESS_SHADER;
use crate::settings::{CrtSettings, SafeMode};

/// Le matériau du filtre, posé sur la caméra 2D quand le filtre est actif.
///
/// Un composant, jamais une ressource ni un asset : c'est ce que le trait
/// exige, et c'est sa présence qui ordonnance la passe.
#[derive(Component, ExtractComponent, ShaderType, Debug, Clone, Copy, Default)]
pub struct CrtMaterial {
    /// Le bloc d'uniformes, seize octets.
    pub params: CrtUniform,
}

/// Le bloc d'uniformes du filtre : quatre `f32`, seize octets pile, aucun
/// padding.
#[derive(ShaderType, Debug, Clone, Copy, Default)]
pub struct CrtUniform {
    /// Courbure du tube.
    pub curvature: f32, // offset  0
    /// Intensité des lignes de balayage, statiques.
    pub scanline_intensity: f32, // offset  4
    /// Rondeur du vignettage.
    pub vignette_roundness: f32, // offset  8
    /// Aberration chromatique radiale.
    pub chromatic_aberration: f32, // offset 12  -> 16 o pile, aucun padding
}
// Aucun champ `enabled` : une passe désactivée n'est pas ordonnancée du tout.

impl CrtUniform {
    /// Les quatre intensités unitaires des réglages, multipliées par
    /// `intensity` : le slider atténue les quatre effets continûment et
    /// ensemble, en Rust, à l'écriture, jamais dans le shader.
    #[must_use]
    pub fn from_settings(settings: &CrtSettings) -> Self {
        Self {
            curvature: settings.curvature * settings.intensity,
            scanline_intensity: settings.scanline_intensity * settings.intensity,
            vignette_roundness: settings.vignette_roundness * settings.intensity,
            chromatic_aberration: settings.chromatic_aberration * settings.intensity,
        }
    }
}

impl FullscreenMaterial for CrtMaterial {
    /// Le fragment du filtre, désigné par le chemin que le plugin publie.
    fn fragment_shader() -> ShaderRef {
        CRT_POSTPROCESS_SHADER.into()
    }

    /// La 2D, et non la 3D du défaut.
    fn schedule() -> impl ScheduleLabel + Clone {
        Core2d
    }

    /// Le set `PostProcess` de la 2D, après le tonemapping et avant
    /// l'upscaling.
    fn schedule_configs(system: ScheduleConfigs<BoxedSystem>) -> ScheduleConfigs<BoxedSystem> {
        system.in_set(Core2dSystems::PostProcess).after(tonemapping)
    }
}

/// La condition de la passe, comme condition d'exécution : vraie quand le
/// filtre doit tourner. C'est la condition pure de TASK-83.
pub fn crt_pass_wanted(settings: Res<CrtSettings>, safe_mode: Res<SafeMode>) -> bool {
    settings.is_active(&safe_mode)
}

/// Pose, réécrit ou retire le matériau sur chaque caméra 2D.
///
/// Filtre actif : le composant est inséré s'il manque, et réécrit seulement
/// quand les réglages ou le mode dégradé ont changé. Filtre coupé : le
/// composant est retiré, et la passe n'existe plus pour cette vue.
pub fn sync_crt_material(
    mut commands: Commands,
    settings: Res<CrtSettings>,
    safe_mode: Res<SafeMode>,
    mut cameras: Query<(Entity, Option<&mut CrtMaterial>), With<Camera2d>>,
) {
    let active = settings.is_active(&safe_mode);
    let changed = settings.is_changed() || safe_mode.is_changed();
    for (camera, material) in &mut cameras {
        match (active, material) {
            (false, Some(_)) => {
                commands.entity(camera).remove::<CrtMaterial>();
            }
            (false, None) => {}
            (true, None) => {
                commands.entity(camera).insert(CrtMaterial {
                    params: CrtUniform::from_settings(&settings),
                });
            }
            (true, Some(mut material)) => {
                if changed {
                    material.params = CrtUniform::from_settings(&settings);
                }
            }
        }
    }
}
