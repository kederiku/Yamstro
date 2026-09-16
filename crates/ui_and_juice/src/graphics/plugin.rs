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
//!    TASK-84, TASK-88 et TASK-90 les désignent par `ShaderRef::Path`. Il ne
//!    les **charge** pas lui-même, et c'est une contrainte du moteur, pas un
//!    choix : une poignée forte lâchée décharge l'asset dès la fin de son
//!    chargement, et le type `Shader` n'est enregistré que par le plugin de
//!    rendu (`bevy_render-0.19.1/src/lib.rs:353`). Le repli de TASK-92 ne
//!    retient donc aucune poignée : il écoute le message
//!    `AssetLoadFailedEvent<Shader>` (`bevy_asset-0.19.1/src/event.rs:10`),
//!    émis dans le monde principal pour tout shader dont le chargement
//!    échoue, d'où qu'il ait été demandé. Ce message n'existe que si
//!    `Assets<Shader>` est enregistré : les tests headless de l'étape font
//!    `init_asset::<Shader>()`, ce que le plugin de rendu fait dans le jeu.
//!
//! # Le mode dégradé (TASK-92)
//!
//! Trois déclencheurs : le réglage persistant (Étape 10), l'argument de ligne
//! de commande `--safe-mode`, lu dans `build` juste après la pose de la
//! ressource, et la bascule automatique sur un shader en échec, qui journalise
//! un avertissement et n'a jamais paniqué. Cette dernière couvre un fichier
//! absent ou illisible ; un échec de **compilation** sur le GPU est une erreur
//! du cache de pipelines dans le monde de rendu, que Bevy journalise en
//! sautant le dessin, et qu'aucun message du monde principal ne porte :
//! consigné pour TASK-94. Les replis vivent à côté de leurs effets, dans
//! `background.rs` et `holo.rs` ; le filtre cathodique passe déjà par
//! `is_active`. La réconciliation tourne sur `resource_changed::<SafeMode>`
//! seulement, et elle est idempotente par constat d'état, pas par mémoire :
//! elle ne construit jamais ce qui existe, ne retire jamais ce qui manque.
//! `graphics/` ne lit et n'écrit le mode que par ses méthodes.
//! 2. **L'enregistrement des matériaux.** `build` est le point
//!    d'enregistrement des matériaux : le fond (TASK-84), le filtre
//!    cathodique (TASK-88, par `FullscreenMaterialPlugin`) et le contour
//!    (TASK-90) y sont branchés.
//!    Chaque matériau est un `Asset`, et `Material2dPlugin::build` appelle
//!    `init_asset`, qui lit `AssetServer`
//!    (`bevy_asset-0.19.1/src/lib.rs:639`) : **ce plugin exige le serveur
//!    d'assets**, et les tests headless de l'étape montent `AssetPlugin`, sans
//!    rendu ni fenêtre.
//! 3. **Le placement des passes.** Le vortex et le contour sont des `Material2d`
//!    rendus en `MainPass` ; le filtre cathodique va dans
//!    `Core2dSystems::PostProcess`, après le tonemapping, jamais dans le set
//!    précoce ni dans la prépasse 2D. TASK-88 l'a placé, dans `crt.rs`, par
//!    `FullscreenMaterial::schedule_configs`.
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

use bevy::asset::AssetLoadFailedEvent;
use bevy::core_pipeline::fullscreen_material::FullscreenMaterialPlugin;
use bevy::prelude::*;
use bevy::shader::Shader;
use bevy::sprite_render::Material2dPlugin;
use log::warn;

use super::background::{
    BackgroundMaterial, apply_safe_mode_to_background, resize_background_quad,
    spawn_background_quad,
};
use super::crt::{CrtMaterial, sync_crt_material};
use super::holo::{
    HoloOutlineMaterial, apply_safe_mode_to_dice, build_holo_bank, dress_dice, sync_die_outline,
};
use super::theme::{animate_visual_theme, init_visual_theme, sync_flat_background};
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

        // Le mode dégradé demandé sur la ligne de commande (TASK-92) : lu
        // ici, avant tout démarrage, et engagé par sa méthode. Sur WASM,
        // `std::env::args()` est vide.
        if SafeMode::requested_by(std::env::args()) {
            app.world_mut().resource_mut::<SafeMode>().engage();
        }

        // Point d'enregistrement des matériaux, une ligne par matériau : le
        // fond (TASK-84), le filtre cathodique (TASK-88), le contour (TASK-90).
        app.add_plugins(Material2dPlugin::<BackgroundMaterial>::default());
        app.add_plugins(FullscreenMaterialPlugin::<CrtMaterial>::default());
        app.add_plugins(Material2dPlugin::<HoloOutlineMaterial>::default());

        // La banque des contours (TASK-90) : huit handles bâtis une fois au
        // démarrage, jamais réécrits, seulement échangés.
        app.add_systems(Startup, build_holo_bank);

        // Les dés (TASK-91) : habillés à leur apparition, puis leur variante
        // réconciliée chaque frame, le handle réécrit seulement s'il diffère.
        app.add_systems(Update, (dress_dice, sync_die_outline).chain());

        // Le filtre cathodique (TASK-88) : présent sur la caméra 2D quand il
        // est actif, absent sinon, réécrit seulement quand les réglages
        // changent. Sa présence est ce qui ordonnance la passe.
        app.add_systems(Update, sync_crt_material);

        // Le quad de fond (TASK-85) : spawné une fois au démarrage, redimensionné
        // en place sur `WindowResized`, sans jamais réécrire son `Transform`.
        // Le contrôleur de thème (TASK-87) naît juste après, avec le handle du
        // quad : la chaîne pose un point de synchronisation entre les deux.
        app.add_systems(Startup, (spawn_background_quad, init_visual_theme).chain());
        app.add_systems(Update, resize_background_quad);
        app.add_systems(Update, animate_visual_theme);

        // Le mode dégradé (TASK-92) : l'échec d'un shader l'engage, puis la
        // réconciliation, sur changement seulement, avant tout ce qui habille
        // ou anime, pour que la frame du changement voie déjà la bonne tenue.
        // L'aplat du fond suit la palette cible, sans interpolation.
        app.add_systems(
            Update,
            (
                engage_safe_mode_on_shader_failure,
                (apply_safe_mode_to_background, apply_safe_mode_to_dice)
                    .run_if(resource_changed::<SafeMode>),
            )
                .chain()
                .before(dress_dice)
                .before(animate_visual_theme)
                .before(sync_flat_background)
                .before(resize_background_quad),
        );
        app.add_systems(Update, sync_flat_background);
    }
}

/// La bascule automatique : un shader dont le chargement échoue engage le mode
/// dégradé, avec un avertissement qui nomme le chemin et l'erreur. Jamais une
/// panique : c'est le cas que le mode existe pour couvrir.
pub fn engage_safe_mode_on_shader_failure(
    mut failures: MessageReader<AssetLoadFailedEvent<Shader>>,
    mut safe_mode: ResMut<SafeMode>,
) {
    for failure in failures.read() {
        warn!(
            "shader `{}` en échec de chargement ({}) : mode dégradé engagé",
            failure.path, failure.error
        );
        if !safe_mode.is_engaged() {
            safe_mode.engage();
        }
    }
}
