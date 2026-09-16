# Section Étape 7 — à porter au glossaire

> **Ce fichier n'est pas le glossaire.** C'est le bloc porté à la v2.1 de
> `00 — Glossaire & Conventions de Nommage`, qui vit sur le corpus et prime sur
> tous les autres documents. Il est livré ici pour que le texte soit rédigé,
> relu et versionné avec le code qu'il décrit, et pour que l'audit de clôture
> (`tests/step7_audit.rs`) vérifie que chaque type qu'il nomme est déclaré une
> fois, et une seule, dans `ui_and_juice`.

**Trois opérations sur le glossaire, faites à la v2.1 du 16 septembre 2026 :**

1. **au registre § 2.6**, la ligne groupée `CrtSettings` (« Intensité du filtre
   CRT + SafeMode ») est **scindée** en deux lignes, `CrtSettings` et
   `SafeMode`, et la fonction `render_plugin` s'y ajoute ;
2. **au § 3**, la couche Bevy, les onze identifiants ci-dessous s'ajoutent ;
3. **au § 7**, la table v1 → v2, deux lignes s'ajoutent.

---

## § 2.6 — Types déclarés par les Étapes (registre) : lignes de l'Étape 7

| Identifiant | Forme | Étape | Rôle |
| :-: | :-: | :-: | :-- |
| `CrtSettings` | Resource | 7 | Réglages du filtre cathodique : `enabled`, `intensity` (`0.0 ..= 1.0`, slider exposé à l'Étape 11), quatre intensités unitaires. Prédicat pur `is_active(&SafeMode)`, seule lecture du drapeau ; `graphics/` ne le lit jamais. |
| `SafeMode` | Resource | 7 | Mode dégradé : `is_engaged`, `engage`, `requested_by` (argument `--safe-mode`). Replis (fond plat, aucune passe cathodique, dés plats) et bascule automatique sur un shader en échec. À exposer dans les options à l'Étape 11, à persister à l'Étape 10. |
| `render_plugin()` | fn | 7 | La configuration de rendu du jeu, écrite une fois : `RenderPlugin` à priorité `WgpuSettingsPriority::Functionality`, les limites réelles de l'adaptateur, montée par `DefaultPlugins.set(render_plugin())`. `WebGL2` et `WebGPU` sont proscrites, mesurées. |

## § 3 — Types canoniques, couche Bevy : lignes de l'Étape 7

| Identifiant canonique | Forme | Remplace |
| :-: | :-: | :-: |
| `VisualEffectsPlugin` | Plugin | — |
| `BackgroundMaterial` | Asset, `Material2d` | — |
| `BackgroundUniform` | `ShaderType`, 64 octets | — |
| `BackgroundQuad` | Component marqueur | — |
| `ThemePalette` | struct | — |
| `VisualThemeController` | Resource | — |
| `CrtMaterial` | Component, `FullscreenMaterial` (l'exception mesurée à TASK-82 : le trait l'exige, ce n'est ni un `Asset` ni une `Resource`) | le nœud de graphe de rendu de la v1 (nom proscrit, voir § 7) |
| `CrtUniform` | `ShaderType`, 16 octets pile | — |
| `HoloOutlineMaterial` | Asset, `Material2d` | — |
| `HoloUniform` | `ShaderType`, 32 octets | — |
| `HoloMaterials` | Resource | — |

Aucun de ces types ne dérive à la fois `Component` et `Resource`. Aucun
uniforme ne porte de champ `time` : l'animation continue vient de
`globals.time` côté WGSL, et le filtre cathodique est statique.

## § 7 — Table de correspondance v1 → v2 : lignes de l'Étape 7

| Ancien nom (v1) | Nom canonique (v2) |
| :-: | :-: |
| le nœud cathodique du graphe de rendu de la v1, `RenderGraph`, `ViewNode` (noms proscrits dans le dépôt, épelés dans le glossaire seul) | `CrtMaterial` (`FullscreenMaterial`), ordonné par `schedule_configs` |
| l'ancienne variante `Compatibility` de `WgpuSettingsPriority` (proscrite) | `WgpuSettingsPriority::Functionality` (mesuré à TASK-93 : `WebGL2`, l'héritière de `Compatibility`, casse à facteur d'échelle 2) |
