//! Les quatre palettes de manche, leur conversion, et le contrôleur qui les
//! fait se succéder : la moitié données (TASK-86) et la moitié système
//! (TASK-87) du thème.
//!
//! # Raccord D : aucune écriture de matériau au repos
//!
//! Le contrôleur n'écrit dans `Assets<BackgroundMaterial>` que pendant les
//! 1,5 s d'une transition. Chaque frame, il calcule la palette cible ; si elle
//! diffère de `to`, la transition repart de la palette **courante**, celle qui
//! est affichée, jamais de l'ancien `from`, sinon la couleur saute en arrière.
//! Une fois le minuteur terminé et la cible inchangée, le système rend la main
//! **avant tout `get_mut`** : une partie complète produit quelques dizaines
//! d'`AssetEvent::Modified`, pas soixante par seconde. La frame où le minuteur
//! franchit 1,5 s écrit une dernière fois, à la fraction 1, donc exactement
//! `to`. À l'insertion, `from == to` et le minuteur est déjà terminé : rien
//! n'est écrit au lancement. L'animation continue, elle, vient de
//! `globals.time` côté WGSL.
//!
//! # Interpolation
//!
//! En espace linéaire, sur les cinq champs, par `Mix::mix` de `bevy::color`
//! (`bevy_color-0.19.1/src/color_ops.rs:33`, ré-exporté à la racine) pour les
//! couleurs, et par la même formule à deux termes `a·(1−t) + b·t` pour les
//! deux `f32` : à `t = 1` elle rend exactement `b`, à `t = 0,5` exactement la
//! moyenne, là où `a + (b − a)·t` peut rater `b` d'un ulp.
//!
//! # Paramètres faillibles
//!
//! `BlindContext` n'existe pas hors d'une run, et `State<RunPhase>` non plus,
//! `RunPhase` étant un sous-état de `AppState::InRun`. Un `Res` nu ferait
//! écarter le système en silence au menu principal, et le fond resterait
//! figé sans la moindre erreur. Les deux sont des `Option<Res<…>>` ; phase
//! absente, la cible est `SMALL`, et le système tourne.
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
//! et la CI l'interdit avant lui. Le `expect` sur la conversion est admis ici
//! et seulement ici : les chaînes sont littérales, et le test des quatre
//! palettes les parcourt à chaque exécution ; le volet 1 interdit tout
//! `unwrap` dans `crates/`, prose comprise.
//!
//! `target_palette` est publique, là où le document l'écrit privée : sans
//! appelant avant TASK-87, une fonction privée est du code mort sous
//! `-D warnings`, et la règle du projet est d'exposer, jamais d'`#[allow]`.

use std::sync::LazyLock;

use bevy::color::{LinearRgba, Mix, Srgba};
use bevy::math::Vec2;
use bevy::prelude::*;
use bevy::sprite::Sprite;
use bevy::sprite_render::MeshMaterial2d;
use core_engine::blinds::{BlindContext, BlindDefinition, BlindType};
use game_state::RunPhase;

use super::background::{BackgroundMaterial, BackgroundQuad, BackgroundUniform};

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
    primary: Srgba::hex("#0B1026").expect("littéral sRGB").into(),
    secondary: Srgba::hex("#7B2FF7").expect("littéral sRGB").into(),
    accent: Srgba::hex("#39E1F7").expect("littéral sRGB").into(),
    speed: 0.4,
    swirl_factor: 0.9,
});

/// Grosse Mise : vert émeraude / doré ambré.
pub static BIG: LazyLock<ThemePalette> = LazyLock::new(|| ThemePalette {
    primary: Srgba::hex("#053B2C").expect("littéral sRGB").into(),
    secondary: Srgba::hex("#E0A128").expect("littéral sRGB").into(),
    accent: Srgba::hex("#FFE7A3").expect("littéral sRGB").into(),
    speed: 0.7,
    swirl_factor: 1.2,
});

/// Mise Boss : rubis incandescent / pourpre sombre. Le vortex accélère et la
/// distorsion fait plus que doubler : 2,1 est voulu, ce n'est pas 1,2.
pub static BOSS: LazyLock<ThemePalette> = LazyLock::new(|| ThemePalette {
    primary: Srgba::hex("#2E0618").expect("littéral sRGB").into(),
    secondary: Srgba::hex("#E11D3C").expect("littéral sRGB").into(),
    accent: Srgba::hex("#FF7A18").expect("littéral sRGB").into(),
    speed: 1.3,
    swirl_factor: 2.1,
});

/// Boutique : sépia / bleu pétrole.
pub static SHOP: LazyLock<ThemePalette> = LazyLock::new(|| ThemePalette {
    primary: Srgba::hex("#3A2C1E").expect("littéral sRGB").into(),
    secondary: Srgba::hex("#125A66").expect("littéral sRGB").into(),
    accent: Srgba::hex("#D8C6A0").expect("littéral sRGB").into(),
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

/// La transition de palette en cours, et le matériau qu'elle écrit.
///
/// **`Resource` uniquement** : en 0.19, `Resource` est un sous-trait de
/// `Component`, dériver les deux ne compile pas, et un composant posé par
/// mégarde sur un type déjà inséré en ressource despawnerait des entités.
#[derive(Resource, Debug, Clone)]
pub struct VisualThemeController {
    /// La palette de départ de la transition en cours.
    pub from: ThemePalette,
    /// La palette cible.
    pub to: ThemePalette,
    /// 1,5 s, `TimerMode::Once`.
    pub timer: Timer,
    /// Le matériau du quad de fond, le seul que ce contrôleur écrit.
    pub handle: Handle<BackgroundMaterial>,
}

impl VisualThemeController {
    /// Un contrôleur au repos sur une palette : `from == to`, minuteur
    /// terminé. Sans cela, l'application écrirait le matériau pendant les
    /// 1,5 premières secondes de chaque lancement.
    #[must_use]
    pub fn at_rest(palette: ThemePalette, handle: Handle<BackgroundMaterial>) -> Self {
        let mut timer = Timer::from_seconds(1.5, TimerMode::Once);
        timer.set_elapsed(timer.duration());
        Self {
            from: palette,
            to: palette,
            timer,
            handle,
        }
    }

    /// La palette affichée à l'instant présent : `from` et `to` interpolées à
    /// la fraction du minuteur.
    #[must_use]
    pub fn current(&self) -> ThemePalette {
        blend(self.from, self.to, self.timer.fraction())
    }
}

/// Deux palettes interpolées en espace linéaire sur les cinq champs.
fn blend(from: ThemePalette, to: ThemePalette, t: f32) -> ThemePalette {
    let t = t.clamp(0.0, 1.0);
    let two_terms = |a: f32, b: f32| a * (1.0 - t) + b * t;
    ThemePalette {
        primary: from.primary.mix(&to.primary, t),
        secondary: from.secondary.mix(&to.secondary, t),
        accent: from.accent.mix(&to.accent, t),
        speed: two_terms(from.speed, to.speed),
        swirl_factor: two_terms(from.swirl_factor, to.swirl_factor),
    }
}

/// Insère le contrôleur au repos sur la Petite Mise, avec le handle du quad.
///
/// Chaîné après le spawn du quad : sans quad, rien n'est inséré, et TASK-92
/// gardera la chaîne entière derrière le mode dégradé. Au démarrage, ni
/// phase ni manche n'existent : la cible est `SMALL`.
pub fn init_visual_theme(
    mut commands: Commands,
    quad: Option<Single<&MeshMaterial2d<BackgroundMaterial>, With<BackgroundQuad>>>,
) {
    let Some(material) = quad else {
        return;
    };
    commands.insert_resource(VisualThemeController::at_rest(*SMALL, material.0.clone()));
}

/// Anime la transition de palette et écrit le matériau du fond pendant ses
/// 1,5 s, et seulement pendant.
pub fn animate_visual_theme(
    blind: Option<Res<BlindContext>>,
    phase: Option<Res<State<RunPhase>>>,
    time: Res<Time>,
    controller: Option<ResMut<VisualThemeController>>,
    mut materials: ResMut<Assets<BackgroundMaterial>>,
) {
    // Sans contrôleur, le fond est plat (mode dégradé) : rien à animer.
    let Some(mut controller) = controller else {
        return;
    };
    let target = current_target(phase, blind);
    if target != controller.to {
        controller.from = controller.current();
        controller.to = target;
        controller.timer.reset();
    }
    if controller.timer.is_finished() {
        return;
    }
    controller.timer.tick(time.delta());
    let palette = controller.current();
    if let Some(mut material) = materials.get_mut(&controller.handle) {
        material.params = BackgroundUniform::from(palette);
    }
}

/// La palette cible du moment : celle de la phase et de la manche, `SMALL`
/// hors run.
fn current_target(
    phase: Option<Res<State<RunPhase>>>,
    blind: Option<Res<BlindContext>>,
) -> ThemePalette {
    match phase {
        Some(phase) => target_palette(*phase.get(), blind.as_deref().map(|b| &b.blind)),
        None => *SMALL,
    }
}

/// Le thème du fond plat (TASK-92) : la couleur primaire de la palette cible,
/// sans interpolation, écrite sur le `Sprite` seulement quand elle diffère.
/// Sans fond plat, rien.
pub fn sync_flat_background(
    blind: Option<Res<BlindContext>>,
    phase: Option<Res<State<RunPhase>>>,
    mut flats: Query<&mut Sprite, With<BackgroundQuad>>,
) {
    let target = Color::from(current_target(phase, blind).primary);
    for mut sprite in &mut flats {
        if sprite.color != target {
            sprite.color = target;
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
        let attendu: LinearRgba = Srgba::hex("7B2FF7").expect("littéral sRGB").into();
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

    /// À `t = 0,5`, chaque canal et chaque `f32` valent la moyenne de `from`
    /// et `to` ; à `t = 1`, exactement `to` ; à `t = 0`, exactement `from`.
    #[test]
    fn test_blend_is_linear_and_exact_at_ends() {
        let milieu = blend(*SMALL, *BOSS, 0.5);
        let moyenne = |a: f32, b: f32| (a + b) / 2.0;
        for (x, a, b) in [
            (milieu.primary.red, SMALL.primary.red, BOSS.primary.red),
            (
                milieu.secondary.green,
                SMALL.secondary.green,
                BOSS.secondary.green,
            ),
            (milieu.accent.blue, SMALL.accent.blue, BOSS.accent.blue),
            (milieu.speed, SMALL.speed, BOSS.speed),
            (milieu.swirl_factor, SMALL.swirl_factor, BOSS.swirl_factor),
        ] {
            assert!(
                (x - moyenne(a, b)).abs() <= 1e-6,
                "{x} contre {}",
                moyenne(a, b)
            );
        }
        assert_eq!(blend(*SMALL, *BOSS, 1.0), *BOSS);
        assert_eq!(blend(*SMALL, *BOSS, 0.0), *SMALL);
        assert_eq!(blend(*SMALL, *BOSS, 7.0), *BOSS, "fraction bornée");
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
