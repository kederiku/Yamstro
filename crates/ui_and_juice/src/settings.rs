//! Réglages d'intensité de la mise en scène.
//!
//! La ressource existe **dès l'Étape 4**, avec ses deux champs : l'Étape 11 se
//! contente de l'exposer dans l'interface des paramètres, elle ne la crée pas.
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
