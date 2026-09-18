# ADR-006 — Addendum : la tâche 0 a mesuré `bevy_seedling`, la branche A est retenue

**Statut :** Accepté · **Date :** 17 septembre 2026 · **Ticket :** TASK-95 · **Complète :** ADR-006

Exemplaire du dépôt. Le journal des ADR (v2.2) porte la même branche et les mêmes valeurs sous
l'ADR-006, et renvoie ici. Le banc qui a produit ces chiffres est `tools/audio_probe/`, détaché
du workspace ; il se relance par `cargo run --release -- --measure`.

**Branche retenue : A, `bevy_seedling` `=0.8.0`**, sur Firewheel 0.12. Les trois critères passent.
Le repli pré-mixé n'est pas employé. 0.8.0 est la seule version de la crate qui cible Bevy 0.19.

## Les trois critères

Méthode commune : rendu **hors ligne**, sans périphérique. Le banc active le contexte Firewheel
lui-même et appelle `FirewheelProcessor::process` bloc par bloc (le patron de `platform/mock.rs`
de la crate, sans fil), à 48 000 Hz, par blocs de 128 trames, 768 trames par image ; il garde
toute la sortie. Le graphe part du gabarit `Empty` : le gabarit par défaut pose un limiteur après
`MainBus`, qui masquerait un clic. Le signal attendu est relu dans les WAV, octet par octet, sans
passer par le décodeur mesuré. L'instrument est éprouvé à chaque lancement : un stem retardé de
50 trames est mesuré à 50 trames, un clic injecté porte le rapport des pas de 0,9993 à 7,2785.

| # | Critère mesuré | Valeur obtenue | Verdict |
| :-: | :-- | :-- | :-- |
| 1 | Bouclage sans couture de 4 stems : dérive ≤ 1 ms, soit 48 trames, après 10 minutes | Dérive maximale entre stems : **0 trame (0,0000 ms)** sur 151 bouclages et 604 impulsions, 10 minutes rendues. Départ à l'échantillon visé, écart 0. Résidu entre la sortie et les quatre stems tuilés : nul au bit près (−inf dBFS), aux coutures comme ailleurs ; gain de chaîne 1,000000000. La dérive se lit sur le signal : chaque stem porte une impulsion à la trame k × 480. | **passe** |
| 2 | Fondu d'une couche de 0 à 1 en 1,5 s sans clic ; trois bus Master, Music, SFX indépendants | Rapport entre le pas maximal du signal pendant le fondu et à l'unité : **1,0000** (seuil 1,01), pour le fondu programmé (`VolumeFade::fade_at`) et pour le fondu écrit image par image ; 99 % du gain atteint à 1,5053 s et à 1,5009 s. Bus, par superposition de trois rendus : résidus de **−106,0**, **−106,0**, **−77,4** et **−126,0 dBFS** (unité, Music à −20 dB, SFX à −20 dB, Master à −20 dB ; seuil −60). Bus Music pris seul : rapport des gains 0,100000. | **passe** |
| 3 | Une réverbération sur le bus SFX, entendue sur `mult_hit`, sans DSP maison | Énergie dans les 300 ms qui suivent la fin du coup sec : **−22,9 dBFS** avec le `FreeverbNode` de Firewheel, silence numérique (−inf dBFS) sans lui ; seuils : plus de −40 avec, moins de −80 sans. Le banc n'implémente aucun nœud audio. | **passe** |

## Ce que la mesure rectifie dans l'ADR-006

- **Rectification 1.** « backend Kira » est faux. `bevy_seedling` est l'intégration de **Firewheel** ;
  Kira n'apparaît nulle part dans son arbre de dépendances. La ligne « Vertical layering et bus
  audio via `bevy_seedling` (backend Kira …) » se lit « (moteur Firewheel …) ».
- **Rectification 2.** « Le backend OGG reste `lewton` (feature `vorbis` par défaut) » est remplacée.
  En branche A, `bevy_seedling` décode seul, par `symphonium` sur `symphonia` (ses features `ogg`
  et `wav`) : `stem_0.ogg` est relevé à 192 000 trames, 1 voie, 48 000 Hz, comme son WAV. Les
  features Bevy `bevy_audio` et `vorbis` n'ont plus d'objet, et `bevy_audio` doit rester
  désactivée : la crate la refuse pour collision de noms (`src/lib.rs`, lignes 16 à 18). La feature
  Bevy `symphonia-vorbis` reste proscrite et n'est pas activée ; le contrôle porte sur la feature,
  jamais sur le nom des crates `symphonia-*`, qui sont ici attendues.
- **Rectification 3.** « 4 stems pré-mixés en un seul fichier » décrit mal le repli : le § 3 du
  document de l'Étape 8 retient quatre mix complets M0 à M3, un seul jouant à la fois. Sans objet
  en branche A, consigné pour que personne ne recopie la formule.

## Ce que TASK-96 applique

| Ligne du manifeste d'`audio_system` | Branche A |
| :-- | :-- |
| `bevy_seedling` | `{ version = "=0.8.0", default-features = false, features = ["ogg", "cpal"] }` : le décodeur OGG et la sortie audio, **sans les défauts de la crate** |
| `firewheel` | `{ version = "0.12", default-features = false, features = ["freeverb_node"] }` : le seul nœud de réverbération, que TASK-97 enregistre par `register_node::<FreeverbNode>()` |
| feature Bevy `bevy_audio` | non |
| feature Bevy `vorbis` | non |
| feature Bevy `symphonia-vorbis` | jamais |
| cible `wasm32` | rien à ajouter : `game_state` active déjà `getrandom` avec `wasm_js` |

Révisé à TASK-96, le 17 septembre 2026, après pesée de quatre jeux de features par la chaîne WASM
du dépôt. La première version de cette table écrivait `features = ["effects"]` avec les défauts de
la crate, la ligne la plus lourde. Le jeu retenu a été rejoué dans le banc : queue de réverbération
à −22,9 dBFS, bus indépendants, à l'identique. Il n'apporte aucune licence nouvelle.

| Jeu de features de `bevy_seedling` | Backend seul, en octets | Marge restante |
| :-- | --: | --: |
| `ogg`, `cpal`, sans réverbération | 2 363 412 | 753 701 |
| **`ogg`, `cpal`, et `freeverb_node` pris dans `firewheel`** (retenu) | **2 495 750** | **621 363** |
| `ogg`, `cpal`, `effects`, qui lie tous les nœuds | 2 737 135 | 379 978 |
| le précédent plus `wav`, `rand`, `reflect`, `diagnostics` | 2 914 887 | 202 226 |

Le WAV n'a pas d'usage (les assets sont en OGG, le bip de secours de TASK-102 se synthétise en
mémoire), ni `rand` (la banque de sons tient son propre générateur), ni `reflect`, ni `diagnostics`.

**Licences.** Neuf crates de l'arbre sont sous MPL-2.0, copyleft faible à l'échelle du fichier :
`symphonia`, `symphonia-codec-pcm`, `symphonia-codec-vorbis`, `symphonia-common`,
`symphonia-core`, `symphonia-format-ogg`, `symphonia-format-riff`, `symphonia-metadata` et
`triple_buffer` (tirée par `firewheel-graph`, donc présente même sans décodeur). Décision du
17 septembre 2026 : neuf **exceptions nommées** dans `deny.toml`, la liste globale restant
permissive. Aucun copyleft fort dans l'arbre. `cargo deny check` passe dans le banc.

## Ce que le banc a appris, à l'usage des tickets suivants

1. **Départ en phase (TASK-100).** Créer les lecteurs **en lecture**, tous porteurs du même
   `DiffTimestamp`, avec `play_at(None, time.now(), …)` : c'est la forme de l'exemple
   `precise_scheduling`, et c'est elle qui donne la dérive nulle ci-dessus. Le piège : un lecteur
   créé en pause laisse fuir une image de son à la création (143 trames non nulles relevées), et
   `play_at(None, t)` le fait **reprendre** à `t` depuis la position de la fuite, donc décalé
   (225 trames relevées). `play_at(Some(PlayFrom::BEGINNING), t)` tombe bien sur l'échantillon
   visé, écarts `[0, 0, 0, 0]`, mais la fuite subsiste.
2. **Réverbération (TASK-97, TASK-105).** Le `FreeverbNode` ne rend que le signal humide (`dry`
   vaut zéro dans `freeverb.rs` et rien ne le règle). Chaîné en série comme dans l'exemple
   `buses_and_pools`, il supprime le son sec : il se monte **en parallèle**, le bus sortant vers
   `MainBus` et vers la réverbération.
3. **Fondus (TASK-101).** Le lisseur du `VolumeNode` se déclare établi à moins de 1 % de son
   étendue de 2,0, soit 0,02 de gain, et saute alors à la cible ; `VolumeNode` n'expose pas ce
   réglage. Un fondu sort donc en marches : 40 marches, saut maximal 0,0198 de gain, 35,0 dB
   sous la note pour le fondu programmé ; 62 marches, 0,0088, 42,4 dB sous la note pour le fondu
   écrit image par image, qui est le moins marqué. Le critère convenu est tenu ; le mode
   `--listen` du banc (touche `2`) laisse l'oreille juger. `Volume::Linear(v)` vaut `v × v` en
   amplitude : `fade_to` depuis le silence suit une courbe quadratique, 0,243 à mi-course.
4. **Graphe (TASK-97).** Partir du gabarit `Empty` et poser soi-même `MainBus`, les bus et les
   pools : le gabarit `Game` impose un limiteur et des pools dont le jeu n'a pas l'usage.
5. **Position de lecture.** `Sampler::try_playhead_frames` est un instantané par bloc, avec une
   gigue d'un bloc : elle ne sert pas à synchroniser.
6. **Tests sans périphérique (TASK-97).** Le patron du pilote hors ligne du banc donne des tests
   sur le son réel, si le backend nul journalisant ne suffit pas.

## Poids sur le Web, à titre d'information

Même application avec et sans backend, `wasm-bindgen` 0.2.128 puis `wasm-opt -Oz`, voie `cpal`
avec `wasm-bindgen` (la voie par défaut de la crate sur `wasm32`) ; `web_audio` est écartée, elle
exige nightly, `atomics` et des en-têtes COOP et COEP.

| Application | Octets optimisés | Backend seul |
| :-- | --: | --: |
| témoin, sans audio | 2 554 950 | |
| branche A, `bevy_seedling` 0.8.0 avec `effects` | 5 497 260 | **2 942 310** |
| branche B, `bevy_audio` avec `vorbis` | 4 598 293 | 2 043 343 |

La marge du projet sous les 26 214 400 octets est de 3 117 113 octets, assets compris. Ces trois
lignes pèsent le banc tel qu'il a mesuré les critères, avec `effects` et les défauts de la crate.
TASK-96 a pesé des jeux plus étroits et retenu 2 495 750 octets, voir plus haut : il reste 621 363
octets avant le premier fichier son. La porte de +5 % (24 242 759) sera franchie quand la cible de
mesure liera le backend : 23 088 342 + 2 495 750 = 25 584 092. TASK-108 réécrit la référence à la
main et tient le budget des stems dans ce qui reste.

## La mesure liée (TASK-108, 18 septembre 2026)

La prévision ci-dessus pesait le backend dans le banc. TASK-108 l'a **lié au jeu** : la cible de
mesure est devenue une crate binaire à elle, `crates/wasm_size/`, qui dépend de tout, et elle est
construite deux fois, sans le son puis avec lui. Après `wasm-opt -Oz` :

| Grandeur | Octets |
| :-- | --: |
| le jeu sans le son, quatre plugins | 23 088 790 |
| ancienne référence, même jeu mesuré depuis la boutique | 23 088 342 |
| **le jeu, son compris : nouvelle référence** | **25 350 138** |
| coût du son, par différence | 2 261 348 |
| `assets/`, dont 462 939 de son | 478 485 |
| total comparé à la cible | 25 828 623 |
| **marge restante sous 26 214 400** | **385 777** |

Le banc surestimait le backend de 234 402 octets (2 495 750 prévus). Le déménagement de la cible
pèse 448 octets : l'ancienne montait déjà les quatre plugins depuis TASK-81, il n'y avait rien à
rattraper. Le saut de référence, +9,8 %, est donc **le son, et lui seul**.

**Il reste 385 777 octets pour les Étapes 10, 9 et 11**, polices et traductions comprises. Tant
que cette marge est inférieure à 5 % de la référence, le plafond de +5 % (26 617 644) passe
au-dessus de la cible absolue : c'est elle qui mord la première, et c'est la marge que la porte
de taille publie désormais.

**Un levier est mesuré, et il n'est pas tiré.** Le profil `release` est à `opt-level = 3`, et
il sert aussi au desktop. Le même jeu, son compris, par la seule variable
`CARGO_PROFILE_RELEASE_OPT_LEVEL` : 19 284 488 octets à `"s"` (−6,07 Mo), 16 165 602 à `"z"`
(−9,18 Mo). Le coût en fluidité dans un navigateur n'est pas mesuré : aucun build jouable
n'existe sur cette branche pour le chronométrer. Un `[profile.wasm-release]` qui hérite de
`release` est la forme attendue ; la décision revient à un ticket de réduction, que l'audit de
clôture de l'étape reçoit comme point ouvert. TASK-108 ne l'a pas prise : changer le profil et
lier le son dans le même commit aurait fait bouger la référence pour deux causes à la fois.
