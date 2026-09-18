# audio_probe — banc d'essai de TASK-95

Mesure les trois critères de la tâche 0 de l'Étape 8 sur `bevy_seedling` 0.8.0 (Firewheel 0.12).
**Paquet détaché du workspace** : la table `[workspace]` vide de `Cargo.toml` en fait sa propre
racine, `cargo metadata` à la racine du dépôt ne le voit pas, et `cargo test --workspace` l'ignore.
Ce qui lui survit, c'est `docs/adr/ADR-006-addendum.md`.

## Lancer

Depuis ce répertoire :

```bash
cargo run --release -- --measure
```

Rend le son hors ligne, sans périphérique, et imprime les valeurs de l'addendum (`clé = valeur`).
`--measure --short` saute les dix minutes du critère 1.

```bash
cargo run --release -- --listen
```

Joue sur la sortie audio. Une touche puis Entrée : `1` quatre stems en phase, `2` fondu de la
couche 3 en 1,5 s, `3` `mult_hit` avec sa réverbération, `m` `u` `s` pour -20 dB sur Master, Music,
SFX, `q` pour quitter.

```bash
cargo deny check
```

La politique de licences du dépôt, plus neuf exceptions MPL-2.0 nommées (`deny.toml`).

## Les assets

`python3 gen_assets.py` régénère les quatre stems et `mult_hit.wav`, sans dépendance. `stem_0.ogg`
est la pièce du décodeur : le premier stem encodé en Vorbis par `soundfile`
(`pip install --target <dir> soundfile`, puis `SoundFile(..., format="OGG", subtype="VORBIS")`).

## Ce que le banc a appris, en trois lignes

- Le départ en phase s'obtient en créant les lecteurs **en lecture** avec un même `DiffTimestamp`.
  Créés en pause puis lancés par `play_at`, ils laissent fuir une image de son à la création.
- Le `FreeverbNode` de Firewheel ne rend que le signal humide : il se monte **en parallèle** du bus.
- Le lisseur du `VolumeNode` saute à la cible dès qu'il en est à moins de 0,02 : un fondu sort en
  marches de 0,02 au plus, 35 dB sous la note dans la mesure.
