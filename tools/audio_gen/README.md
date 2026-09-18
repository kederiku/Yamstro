# Générateur des assets audio

`gen_audio.py` produit les 21 fichiers de `assets/audio/` : dix-sept effets et les quatre couches
de la musique. Tout est de la synthèse, **écrit par le projet** : aucune source tierce, et c'est
ce que dit `assets/CREDITS.md`. Les fichiers sont commis ; ce script ne tourne ni en CI ni à la
construction du jeu.

## Relancer

```bash
python3 -m pip install --target <dossier> "soundfile==0.13.1" "numpy==2.0.2"
PYTHONPATH=<dossier> python3 gen_audio.py --out ../../assets/audio
PYTHONPATH=<dossier> python3 gen_audio.py --probe
PYTHONPATH=<dossier> python3 gen_audio.py --preview <dossier hors dépôt>
```

Versions de la livraison (TASK-107, 18 septembre 2026) : Python 3.9.6, `soundfile` 0.13.1
(libsndfile 1.2.2, qui embarque libvorbis), `numpy` 2.0.2. **Deux générations donnent le même
son, pas forcément les mêmes octets** : libsndfile tire au hasard le numéro de série du flux Ogg.

## Ce qui tient la conception

- **Le poids.** `ci/check-wasm-size.sh` compare le `.wasm` optimisé **plus `assets/`** à 25 Mo :
  il reste 621 363 octets pour tout le son une fois le backend lié (addendum de l'ADR-006).
  Enveloppe retenue : 500 000 octets pour les 21 fichiers, tenue par
  `test_audio_fits_the_wasm_budget`. La consigne du ticket (512 ko d'effets, 5,5 Mo de musique)
  valait dix fois ce reste.
- **La boucle.** Les quatre couches sortent du même rendu, sur un tampon circulaire : ce qui
  dépasse la fin se replie sur le début. 100 BPM : un temps vaut 19 200 trames à 32 kHz, huit
  mesures 614 400 trames, 19,2 s.
- **La hauteur.** Les effets que le jeu rejoue jusqu'à quatre fois plus vite gardent leur énergie
  sous 5,5 kHz.
- **L'accord.** Tout est accordé sur la mineur ; la fanfare monte sur do majeur, le relatif, et
  dure 1,15 s, sous la fenêtre de 1,2 s du ducking.

## La mesure qui a fixé la musique

Ordre de sacrifice décidé : la stéréo, puis la fréquence, puis la longueur. Poids des quatre
couches et plus mauvais rapport signal sur bruit après décodage, par niveau de compression de
libsndfile (0 = meilleur, 1 = plus léger). Plafond de la musique : 400 000 octets.

| Musique | 0,4 | 0,6 | 0,8 | 1,0 |
| :-- | --: | --: | --: | --: |
| stéréo 48 kHz, 16 mesures | 1 937 683 (26,8 dB) | 1 475 817 (22,1) | 1 202 123 (18,1) | 848 908 (13,3) |
| mono 48 kHz, 16 mesures | 935 085 (27,8) | 771 572 (23,8) | 681 707 (21,1) | 526 707 (17,0) |
| mono 32 kHz, 16 mesures | 851 512 (28,3) | 690 074 (24,1) | 595 186 (21,5) | 445 542 (16,4) |
| **mono 32 kHz, 8 mesures** | 434 352 (28,6) | **353 236 (24,4)** | 304 616 (21,5) | 230 126 (16,6) |

Seule la dernière ligne tient, et le niveau 0,6 y garde 24 dB. Les effets sont au niveau 0,4 :
109 703 octets, dont près de 73 000 d'en-têtes Vorbis (environ 4,3 ko par fichier, incompressibles),
23 dB au plus mauvais. Total : 462 939 octets.

Le chargeur du jeu rééchantillonne au chargement vers la fréquence du flux : des couches à 32 kHz
se lisent juste, et quatre fichiers de même longueur le restent.
