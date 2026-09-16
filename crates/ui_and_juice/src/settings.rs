//! Réglages visuels : intensité de la mise en scène, filtre cathodique, mode
//! dégradé.
//!
//! **Un seul module de réglages visuels** (raccord B de l'Étape 7) :
//! `JuiceSettings` depuis l'Étape 4, `CrtSettings` et `SafeMode` depuis
//! TASK-83, à côté, et rien d'autre. Une seule chose à sérialiser à l'Étape 10,
//! une seule à exposer à l'Étape 11, qui ne crée aucune de ces ressources.
//!
//! `JuiceSettings` existe **dès l'Étape 4**, avec ses deux champs : l'Étape 11
//! se contente de l'exposer dans l'interface des paramètres, elle ne la crée
//! pas.
//!
//! `flash_intensity` n'a **aucun consommateur avant TASK-51**, et c'est voulu.
//! Le mode « réduire les flashs » est un tout — suppression du flash plein
//! écran sans perte d'information, plafond de trois changements de luminance
//! par seconde, dégradation en pulsation locale — et le découper en morceaux
//! livrerait un demi-mode qui donnerait l'illusion d'être là.

use bevy::prelude::*;

/// Intensité de la mise en scène, pour la photosensibilité.
///
/// **`Resource` uniquement.** En 0.19, `Resource` est un sous-trait de
/// `Component` : poser cette ressource sur une entité « pour la caméra »
/// despawnerait silencieusement les autres porteurs.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct JuiceSettings {
    /// Intensité des flashs. Consommée à partir de TASK-51.
    pub flash_intensity: f32,
    /// Intensité de la secousse de caméra.
    pub shake_intensity: f32,
}

impl Default for JuiceSettings {
    /// **Le juice complet.** Un `derive(Default)` donnerait `(0.0, 0.0)`,
    /// c'est-à-dire tout coupé par défaut : c'est le joueur photosensible qui
    /// baisse, jamais le réglage d'usine.
    fn default() -> Self {
        Self {
            flash_intensity: 1.0,
            shake_intensity: 1.0,
        }
    }
}

/// Réglages du filtre cathodique.
///
/// **`Resource` uniquement**, comme `JuiceSettings` : en 0.19, `Resource` est
/// un sous-trait de `Component` (`bevy_ecs-0.19.1/src/resource.rs:87`),
/// dériver les deux ne compile pas, et poser la ressource sur une entité
/// « pour la caméra » despawnerait silencieusement les autres porteurs. Les
/// champs sont ceux du document source, verbatim ; rien d'autre n'est dérivé,
/// ni `Copy` ni `PartialEq`.
///
/// # Trois règles, posées ici, tenues par TASK-88
///
/// 1. **`intensity` multiplie les quatre intensités unitaires** au moment
///    d'écrire l'uniforme du filtre, qui fait seize octets pile, quatre `f32`.
///    Ce n'est pas un cinquième champ d'uniforme : TASK-88 y écrit
///    `curvature * intensity`, `scanline_intensity * intensity`, et ainsi de
///    suite.
/// 2. **Aucun booléen ne transite par l'uniforme.** Si le filtre est coupé,
///    si son intensité est nulle ou si le mode dégradé est actif, la passe
///    n'est pas ordonnancée du tout : pas de branchement sur le GPU, pas de
///    coût, identité de l'image garantie par l'absence de passe et non par un
///    `if` dans le fragment. Le prédicat est [`Self::is_active`].
/// 3. **L'écriture de l'uniforme est gardée par `is_changed()`** sur cette
///    ressource : hors transition de palette, aucun système n'écrit dans un
///    matériau. Un `ResMut<Assets<M>>` déréférencé chaque frame produit un
///    `AssetEvent::Modified` par frame, coût que le guide 0.19 signale.
#[derive(Resource, Debug, Clone)]
pub struct CrtSettings {
    /// Interrupteur global.
    pub enabled: bool,
    /// `0.0 ..= 1.0`, slider exposé à l'Étape 11.
    pub intensity: f32,
    /// Courbure du tube.
    pub curvature: f32,
    /// Intensité des lignes de balayage, statiques.
    pub scanline_intensity: f32,
    /// Rondeur du vignettage.
    pub vignette_roundness: f32,
    /// Aberration chromatique radiale.
    pub chromatic_aberration: f32,
}

impl Default for CrtSettings {
    /// **Le filtre allumé, à pleine intensité** : c'est la présentation
    /// nominale du jeu, pas une option. Un `derive(Default)` donnerait
    /// `enabled == false` et `intensity == 0.0`, l'inverse exact de
    /// l'intention, sans la moindre erreur : le jeu démarrerait sans son
    /// filtre. Les quatre intensités unitaires n'ont aucune valeur normative
    /// dans le corpus, comme `decay` et `max_offset` de l'Étape 4 : ce sont des
    /// réglages de départ, à calibrer à TASK-89 quand le shader existera.
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: 1.0,
            curvature: 0.12,
            scanline_intensity: 0.25,
            vignette_roundness: 0.55,
            chromatic_aberration: 0.002,
        }
    }
}

impl CrtSettings {
    /// Le prédicat de garde de la passe cathodique : vrai quand la passe doit
    /// être ordonnancée.
    ///
    /// Faux si le filtre est coupé, si son intensité est nulle ou si le mode
    /// dégradé est actif : les trois cas produisent le **même** résultat, une
    /// passe absente de l'ordonnancement. TASK-88 s'en sert comme condition
    /// d'ordonnancement, et c'est le seul endroit qui lit le drapeau :
    /// `graphics/` ne le déclare, ne l'initialise ni ne le lit jamais, et la CI
    /// l'interdit.
    #[must_use]
    pub fn is_active(&self, safe_mode: &SafeMode) -> bool {
        self.enabled && self.intensity > 0.0 && !safe_mode.enabled
    }
}

/// Le mode dégradé : coupe tous les shaders avancés sans faire tomber le jeu.
///
/// **`Resource` uniquement**, pour la même raison que [`CrtSettings`]. Il
/// dérive `Default` et vaut donc `false` : le mode ne s'active jamais tout seul
/// au premier lancement. Ses replis (fond plat, passe cathodique absente,
/// contour uni) et sa bascule automatique sur un shader en échec sont
/// TASK-92 ; ici il n'est que déclaré.
#[derive(Resource, Debug, Clone, Default)]
pub struct SafeMode {
    /// Vrai quand le mode dégradé est actif.
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_juice_settings_defaults() {
        // **Un `derive(Default)` serait faux** : il donnerait (0.0, 0.0),
        // c'est-à-dire tout le juice coupé par défaut. Le défaut est le juice
        // complet ; c'est le joueur photosensible qui le baisse.
        let reglages = JuiceSettings::default();
        assert_eq!(reglages.flash_intensity, 1.0);
        assert_eq!(reglages.shake_intensity, 1.0);
    }
}
