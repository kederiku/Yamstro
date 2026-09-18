# Crédits et licences des assets

Ce registre couvre **tout `assets/`**, fichier par fichier : une ligne par fichier, cinq colonnes,
aucune entrée groupée. Il ne se couvre pas lui-même : `assets/CREDITS.md` est le seul fichier de
`assets/` sans ligne. Il naît avec les premiers assets du projet parce que le seul instant où la
provenance d'un fichier est gratuite est celui où il entre au dépôt.

## Politique

Pendant de `deny.toml`, qui ne gouverne que les crates : permissif, distribuable commercialement,
jamais de copyleft.

**Licences admises**, et elles seules, dans la colonne Licence :

| Valeur de la colonne | Condition |
| :-- | :-- |
| `CC0-1.0` | aucune |
| `CC-BY-4.0` | le crédit est porté nommément, par cette table et par l'écran de crédits du jeu |
| `Commerciale : <nom exact de la licence>` | libre de redevances, facture archivée |
| `Propriétaire, Projet Yamstro (tous droits réservés)` | fichier **écrit par le projet** |

**Refusées, sans discussion** : toute mention NC (non commerciale) ; toute mention ND (pas de
dérivés : réencoder en OGG et découper une boucle sont des dérivés) ; toute mention SA, GPL ou
AGPL ; « usage personnel » ; un crédit exigé qu'on ne peut pas porter nommément ; et tout fichier
dont la licence n'est pas identifiable avec certitude. **Un asset dont la licence est incertaine
n'entre pas au dépôt**, pas même le temps d'un essai : il resterait dans l'historique.

**La preuve.** Une page de licence est un lien mort à trois ans. Pour tout fichier qui n'est ni
`CC0-1.0` ni écrit par le projet, le texte de la licence ou la facture est archivé sous
`docs/licences-assets/`, dans un fichier qui porte le nom de l'asset, et la colonne « Ajouté le »
vaut aussi date de consultation. Hors de `assets/`, qui est expédié avec le jeu.

**Fichiers écrits par le projet.** Source : le chemin de ce qui les produit, ou « écrit à la
main ». Auteur : Projet Yamstro. Le dépôt ne porte pas encore de licence : ces fichiers sont
propriétaires.

**Aucun fichier de repli.** Le bip qui remplace un son manquant est produit par le code, jamais
par un fichier : un fichier de repli passerait tous les contrôles et ferait taire l'avertissement
qui signale l'asset manquant.

## Registre

| Fichier | Source | Auteur | Licence | Ajouté le |
| :-- | :-- | :-- | :-- | :-- |
| `assets/audio/chip_tick.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/coin.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/dice_roll_01.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/dice_roll_02.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/dice_roll_03.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/dice_roll_04.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/dice_roll_05.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/dice_roll_06.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/die_lock.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/hand_base_chord.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/hand_consumed.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/mult_hit.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/relic_chord.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/seal_tick.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/stem_base.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/stem_climax.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/stem_melody.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/stem_tension.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/ui_click.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/ui_hover.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/audio/victory_fanfare.ogg` | `tools/audio_gen/gen_audio.py` (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-18 |
| `assets/shaders/crt_postprocess.wgsl` | écrit à la main (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-15 |
| `assets/shaders/holo_card.wgsl` | écrit à la main (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-15 |
| `assets/shaders/psyche_background.wgsl` | écrit à la main (écrit par le projet) | Projet Yamstro | Propriétaire, Projet Yamstro (tous droits réservés) | 2026-09-15 |
