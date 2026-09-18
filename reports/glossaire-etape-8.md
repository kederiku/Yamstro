# Section Étape 8 — à porter au glossaire

> **Ce fichier n'est pas le glossaire.** C'est le bloc porté à la v2.2 de
> `00 — Glossaire & Conventions de Nommage`, qui vit sur le corpus et prime sur
> tous les autres documents. Il est livré ici pour que le texte soit rédigé,
> relu et versionné avec le code qu'il décrit, et pour que l'audit de clôture
> (`crates/audio_system/tests/step8_audit.rs`) vérifie, **dans les deux sens**,
> que chaque identifiant qu'il nomme est déclaré une fois dans le code, et que
> chaque déclaration publique de la crate audio y figure.

**Deux opérations sur le glossaire, faites à la v2.2 du 18 septembre 2026 :**

1. **au registre § 2.6**, les lignes de l'Étape 8 ci-dessous s'ajoutent après
   `PitchScaleTracker`, dont le rôle est complété ;
2. **au § 7**, la table v1 → v2, trois lignes s'ajoutent.

Le § 3 ne bouge pas : `SoundEffectBank`, `AdaptiveMusicManager` et
`AudioBusVolumes` y figuraient déjà. **Un registre, un endroit** : tout ce que
l'étape déclare d'autre entre au § 2.6, comme TASK-104 l'a fait pour
`PitchScaleTracker`.

---

## § 2.6 — Types déclarés par les Étapes (registre) : lignes de l'Étape 8

| Identifiant | Forme | Étape | Rôle |
| :-: | :-: | :-: | :-- |
| `GameAudioPlugin` | Plugin | 8 | Monte tout le son du jeu, dans `audio_system`, seule crate de fonctionnalité qui dépende d'un backend audio. Trois constructeurs : `new()` (backend réel, muet sans sortie audio), `headless()` (backend nul journalisant, tests), `offline()` (backend réel sans plateforme, tests sur le son rendu). Son `build` **exige**, par assertion, `AssetPlugin`, la machine à états et `JuicePlugin` déjà montés : en 0.19 un système dont une ressource manque panique, il n'est pas écarté. |
| `AudioBackend` | trait | 8 | **La façade, seule frontière avec le backend**, dans `backend.rs` : `load_clip`, `play`, `load_layer`, `set_layer_gain`, `set_bus_gain`. Tout `f32` qui la traverse est une **amplitude linéaire**. Le backend retenu est `bevy_seedling` `=0.8.0` sur Firewheel (branche A de l'addendum de l'ADR-006). |
| `AudioBackendHandle` | Resource | 8 | Porte le backend (`Box<dyn AudioBackend>`). Insérée par le plugin et par personne d'autre ; `null()` rend le journal quand le backend monté est le nul. |
| `AudioClip` | newtype de façade | 8 | Identifiant **opaque** d'un son chargé, défini dans `backend.rs`. Jamais un `Handle<T>` du backend ; `Copy`, jamais cloné. |
| `LayerHandle` | newtype de façade | 8 | Identifiant **opaque** d'une couche musicale, index 0 à 3 : Base, Mélodie, Tension, Climax. |
| `Bus` | enum | 8 | `Master`, `Music`, `Sfx`, `SfxReverb`. Le dernier est un **routage**, sans curseur : la réverbération y est montée en parallèle du bus SFX, aucun DSP n'est écrit à la main. |
| `BackendKind` | enum | 8 | `Real`, `Null`, `Offline` : le discriminant que le plugin porte, jamais le backend. `resolve_kind()` replie `Real` sur `Null` quand la machine n'a pas de sortie audio. |
| `resolve_kind()` | fn | 8 | Voir `BackendKind`. Pure. |
| `NullBackend` | struct | 8 | Backend nul **journalisant** : aucun périphérique, aucun son, tout est consigné (`played()`, `loaded()`, `layer_loads()`, `bus_gain_writes()`…). Le journal n'est ni optionnel ni désactivable : les tests de décision de l'étape lisent des décisions, pas du son. C'est aussi le backend d'une machine sans sortie audio. |
| `PlayedSound` | struct | 8 | Une ligne du journal : clip, bus, volume, hauteur. |
| `SeedlingBackend` | struct | 8 | Backend réel. Un chargement échoué y est remplacé par un bip **produit par le code**, dit une fois avec le chemin ; un son demandé avant que son clip soit résolu attend dans la file ; un stem manquant se dit et n'est pas remplacé. |
| `LAYER_COUNT` | const | 8 | 4. |
| `sfx_volume()` / `music_gain()` | fn | 8 | Les deux formules **entendues** : `sfx × master × volume local`, `music × master × poids × ducking`, bornées à `0.0 ..= 1.0`. Pures. **Elles ne se remettent jamais à la façade** : les curseurs s'appliquent sur les bus. |
| `local_gain()` / `layer_gain()` | fn | 8 | Ce qui se remet à la façade : la part locale d'un son, et `poids × ducking` d'une couche. Pures. |
| `target_gains()` | fn | 8 | Gains cibles des quatre couches, **fonction pure de l'état** (`AppState`, `RunPhase`, `BlindContext`). Aucun événement, aucune hystérésis. |
| `blind_in_play()` | fn | 8 | Filtre le contexte de blind périmé : `BlindContext` n'est jamais retiré du monde, il ne compte qu'en run et hors boutique. Pure. |
| `CLIMAX_THRESHOLD_PERCENT` | const | 8 | 75, en `u128`. **Seule écriture du seuil dans le dépôt** ; comparaison entière. |
| `STEM_PATHS` | const | 8 | Les quatre chemins de couche, dans l'ordre des index. |
| `DEFAULT_FADE_PER_SECOND` / `DUCK_DURATION` / `FANFARE_DUCK_GAIN` | const | 8 | 2,0 par seconde ; 1 200 ms ; `0.501_187_2`, −6 dB en amplitude, écrit une fois. |
| `step_clip()` / `step_bus()` / `step_pitch()` / `step_intensity()` / `seal_tint()` | fn | 8 | Ce que le lecteur de `ScoreStepPlayed` dérive d'un palier : le timbre vient de la source, l'intensité de l'action, la hauteur suit la source, seul le coup multiplicatif part sur le routage réverbéré. Pures, `match` exhaustifs sans bras générique. |
| `bus_plugin()` / `music_plugin()` / `sfx_plugin()` / `pitch_plugin()` | fn | 8 | Les quatre points d'enregistrement de la crate, appelés par `GameAudioPlugin::build`. `lib.rs` n'enregistre aucun système. |
| `blind_is_beaten()` | fn | 8 | `core_engine::blinds`. **La seule définition de « blind battue » du dépôt** : `current_score >= target_score`. Quatre lecteurs l'appellent : l'arbitre de fin de manche, la victoire finale, le harnais de simulation, l'audio. |
| `add_game_plugins()` | fn | 8 | `crates/wasm_size`, outillage. La liste des quatre plugins de jeu telle que la cible de mesure la monte, partagée avec le test de démarrage. Un substitut daté du binaire de l'Étape 11. |

`PitchScaleTracker` (déjà au registre) : compteur de demi-tons du décompte en
cours, `base_pitch × 2^(n / 12)`, **seul modèle de hauteur du projet**, plafonné
à 24 dans `advance()`, remis à zéro à l'**entrée** de `RunPhase::Scoring`.

Aucun type de l'Étape 8 ne dérive à la fois `Component` et `Resource`. L'audio
ne crée ni n'émet aucun événement, et ne consomme jamais un flux de `RunRng` :
le générateur de la banque est un `SmallRng` privé, non sérialisé.

## § 7 — Table de correspondance v1 → v2 : lignes de l'Étape 8

| Ancien nom (v1) | Nom canonique (v2) |
| :-: | :-: |
| « +0,05 linéaire » par palier (Étape 4) | `PitchScaleTracker`, demi-ton tempéré (Étape 8) ; l'Étape 4 ne parle plus de hauteur |
| quatre sinks `bevy_audio` lancés « au même instant » | `AudioBackend` sur `bevy_seedling`, couches créées en lecture sous un même horodatage |
| `STREAM_AUDIO`, tirage audio sur `RunRng` | générateur local de `SoundEffectBank`, nom proscrit |
