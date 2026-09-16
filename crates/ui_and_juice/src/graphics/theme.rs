//! Les quatre palettes de manche et leur conversion : la moitié données du
//! thème (TASK-86). L'interpolation, le contrôleur de thème et son minuteur
//! sont TASK-87, l'autre moitié de ce module.
//!
//! # Une seule conversion, dans un seul sens
//!
//! Les couleurs de design sont données en hexadécimal sRGB et converties une
//! seule fois, par `Srgba::hex(..)` puis `.into()` vers `LinearRgba`. Jamais
//! une composante linéaire écrite à la main depuis un hexadécimal : une couleur
//! sRGB traitée comme linéaire paraît délavée, et rien ne le signale, ni la
//! compilation, ni le rendu. Le test de double conversion est le seul
//! garde-fou : reconvertir assombrit, et l'écart se mesure canal par canal.
//!
//! # Des `static`, pas des `const`
//!
//! `Srgba::hex` est faillible (`bevy_color-0.19.1/src/srgba.rs:127`) et la
//! conversion sRGB → linéaire passe par une puissance (`From<Srgba> for
//! LinearRgba`, `:392`) : ni l'une ni l'autre ne s'évalue en `const`, et Rust
//! n'admet pas de `static` associé dans un `impl`. Les quatre palettes sont
//! donc quatre `static LazyLock` au niveau du module, converties une fois par
//! exécution, au premier accès ; `target_palette` les lit en `*SMALL`. Un
//! `const LazyLock` serait inliné à chaque usage, le verrou reconstruit à
//! chaque lecture, la conversion refaite à chaque frame : clippy le signale,
//! et la CI l'interdit avant lui. Le `.unwrap()` est admis ici et seulement
//! ici : les chaînes sont littérales, et le test des quatre palettes les
//! parcourt à chaque exécution.
//!
//! `target_palette` est publique, là où le document l'écrit privée : sans
//! appelant avant TASK-87, une fonction privée est du code mort sous
//! `-D warnings`, et la règle du projet est d'exposer, jamais d'`#[allow]`.

use std::sync::LazyLock;

use bevy::color::{LinearRgba, Srgba};
use bevy::math::Vec2;
use core_engine::blinds::{BlindDefinition, BlindType};
use game_state::RunPhase;

use super::background::BackgroundUniform;

/// Une palette : trois couleurs linéaires, vitesse, distorsion.
///
/// Verbatim du § 3.2, plus `PartialEq`, la seule extension admise : les tests
/// comparent des palettes. Ni `Default`, ni `Resource`, ni `Component`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemePalette {
    /// La dominante sombre du fond.
    pub primary: LinearRgba,
    /// La dominante de la spirale, mélangée à la primaire sur la spirale.
    pub secondary: LinearRgba,
    /// La teinte du cœur du vortex.
    pub accent: LinearRgba,
    /// Vitesse de l'animation.
    pub speed: f32,
    /// Distorsion de l'espace UV.
    pub swirl_factor: f32,
}

/// Petite Mise : bleu nuit / violet néon. Aussi la palette hors run.
pub static SMALL: LazyLock<ThemePalette> = LazyLock::new(|| ThemePalette {
    primary: Srgba::hex("#0B1026").unwrap().into(),
    secondary: Srgba::hex("#7B2FF7").unwrap().into(),
    accent: Srgba::hex("#39E1F7").unwrap().into(),
    speed: 0.4,
    swirl_factor: 0.9,
});

/// Grosse Mise : vert émeraude / doré ambré.
pub static BIG: LazyLock<ThemePalette> = LazyLock::new(|| ThemePalette {
    primary: Srgba::hex("#053B2C").unwrap().into(),
    secondary: Srgba::hex("#E0A128").unwrap().into(),
    accent: Srgba::hex("#FFE7A3").unwrap().into(),
    speed: 0.7,
    swirl_factor: 1.2,
});

/// Mise Boss : rubis incandescent / pourpre sombre. Le vortex accélère et la
/// distorsion fait plus que doubler : 2,1 est voulu, ce n'est pas 1,2.
pub static BOSS: LazyLock<ThemePalette> = LazyLock::new(|| ThemePalette {
    primary: Srgba::hex("#2E0618").unwrap().into(),
    secondary: Srgba::hex("#E11D3C").unwrap().into(),
    accent: Srgba::hex("#FF7A18").unwrap().into(),
    speed: 1.3,
    swirl_factor: 2.1,
});

/// Boutique : sépia / bleu pétrole.
pub static SHOP: LazyLock<ThemePalette> = LazyLock::new(|| ThemePalette {
    primary: Srgba::hex("#3A2C1E").unwrap().into(),
    secondary: Srgba::hex("#125A66").unwrap().into(),
    accent: Srgba::hex("#D8C6A0").unwrap().into(),
    speed: 0.3,
    swirl_factor: 0.6,
});

/// La palette cible d'une phase et d'une manche.
///
/// Verbatim du § 3.2, à la seule adaptation près des `static` : `*SHOP` pour
/// `ThemePalette::SHOP`. Deux propriétés tiennent à la forme du `match`, à ne
/// pas réordonner : la boutique prime sur le type de blind, le bras
/// `RunPhase::Shop` étant testé en premier ; et l'absence de blind retombe sur
/// `SMALL`, le bras `_` couvrant `None` et `Some(BlindType::Small)`. Hors run,
/// c'est `SMALL` : pas de cinquième palette de menu.
#[must_use]
pub fn target_palette(phase: RunPhase, blind: Option<&BlindDefinition>) -> ThemePalette {
    match phase {
        RunPhase::Shop => *SHOP,
        _ => match blind.map(|b| b.kind) {
            Some(BlindType::Big) => *BIG,
            Some(BlindType::Boss) => *BOSS,
            _ => *SMALL,
        },
    }
}

impl From<ThemePalette> for BackgroundUniform {
    /// Le bloc d'uniformes d'une palette, remplissage à zéro.
    ///
    /// Le spawn de TASK-85 en part pour la Petite Mise ; TASK-87 s'en sert
    /// pour écrire la palette interpolée pendant une transition.
    fn from(palette: ThemePalette) -> Self {
        Self {
            primary_color: palette.primary,
            secondary_color: palette.secondary,
            accent_color: palette.accent,
            speed: palette.speed,
            swirl_factor: palette.swirl_factor,
            _pad: Vec2::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blind(kind: BlindType) -> BlindDefinition {
        BlindDefinition {
            kind,
            target_score: 300,
            reward: 3,
            modifier: None,
        }
    }

    /// Quatre palettes deux à deux distinctes, six comparaisons ; la Boss
    /// accélère à 1,3.
    #[test]
    fn test_palette_for_each_blind_type() {
        assert_ne!(*SMALL, *BIG);
        assert_ne!(*SMALL, *BOSS);
        assert_ne!(*SMALL, *SHOP);
        assert_ne!(*BIG, *BOSS);
        assert_ne!(*BIG, *SHOP);
        assert_ne!(*BOSS, *SHOP);
        assert_eq!(BOSS.speed, 1.3);
    }

    /// La boutique prime sur le type de blind, quel qu'il soit.
    #[test]
    fn test_shop_phase_overrides_blind_type() {
        for kind in [BlindType::Small, BlindType::Big, BlindType::Boss] {
            assert_eq!(target_palette(RunPhase::Shop, Some(&blind(kind))), *SHOP);
        }
    }

    /// Sans blind, hors boutique, c'est la Petite Mise : au menu principal
    /// comme au choix de la manche.
    #[test]
    fn test_missing_blind_falls_back_to_small() {
        assert_eq!(target_palette(RunPhase::Roll, None), *SMALL);
        assert_eq!(target_palette(RunPhase::BlindSelect, None), *SMALL);
    }

    /// Hors boutique, chaque type de blind rend sa palette.
    #[test]
    fn test_blind_type_selects_its_palette() {
        assert_eq!(
            target_palette(RunPhase::Roll, Some(&blind(BlindType::Big))),
            *BIG
        );
        assert_eq!(
            target_palette(RunPhase::Scoring, Some(&blind(BlindType::Boss))),
            *BOSS
        );
        assert_eq!(
            target_palette(RunPhase::RoundEnd, Some(&blind(BlindType::Small))),
            *SMALL
        );
    }

    /// La palette porte la conversion exacte de l'hexadécimal, une fois. Les
    /// mêmes composantes reconverties, traitées comme du sRGB, donnent une
    /// couleur différente et plus sombre sur chaque canal non nul : c'est la
    /// faute silencieuse que ce test seul attrape. Sans dièse ici, exprès : la
    /// CI compte les douze appels des palettes, écrits avec dièse.
    #[test]
    fn test_srgb_converted_exactly_once() {
        let attendu: LinearRgba = Srgba::hex("7B2FF7").unwrap().into();
        assert_eq!(attendu, SMALL.secondary);

        let reconvertie: LinearRgba =
            Srgba::new(attendu.red, attendu.green, attendu.blue, attendu.alpha).into();
        assert_ne!(reconvertie, attendu);
        for (deux_fois, une_fois) in [
            (reconvertie.red, attendu.red),
            (reconvertie.green, attendu.green),
            (reconvertie.blue, attendu.blue),
        ] {
            assert!(une_fois > 0.0);
            assert!(
                deux_fois < une_fois,
                "{deux_fois} devrait être plus sombre que {une_fois}"
            );
        }
    }

    /// La distorsion de la Boss fait plus que doubler celle de la Petite Mise.
    #[test]
    fn test_boss_swirl_is_2_1() {
        assert_eq!(SMALL.swirl_factor, 0.9);
        assert_eq!(BOSS.swirl_factor, 2.1);
        assert!(BOSS.swirl_factor > 2.0 * SMALL.swirl_factor);
    }

    /// Le quad est opaque : alpha à 1 sur les douze couleurs.
    #[test]
    fn test_all_palettes_are_opaque() {
        for palette in [*SMALL, *BIG, *BOSS, *SHOP] {
            for couleur in [palette.primary, palette.secondary, palette.accent] {
                assert_eq!(couleur.alpha, 1.0);
            }
        }
    }

    /// La conversion vers l'uniforme recopie les cinq champs et met le
    /// remplissage à zéro.
    #[test]
    fn test_palette_converts_to_uniform() {
        let uniforme = BackgroundUniform::from(*BOSS);
        assert_eq!(uniforme.primary_color, BOSS.primary);
        assert_eq!(uniforme.secondary_color, BOSS.secondary);
        assert_eq!(uniforme.accent_color, BOSS.accent);
        assert_eq!(uniforme.speed, BOSS.speed);
        assert_eq!(uniforme.swirl_factor, BOSS.swirl_factor);
        assert_eq!(uniforme._pad, Vec2::ZERO);
    }
}
