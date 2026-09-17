"""Régénère les fichiers OGG des tests du backend réel (`tests/real_backend.rs`).

Quatre couches d'une seconde pile (48 000 trames à 48 kHz), chacune une note en nombre entier
de périodes et une impulsion à la trame k * 480 : décalées de 10 ms, les quatre impulsions se
séparent dans la somme et donnent la phase de chaque couche. `hit.ogg` est un coup de bruit de
120 ms à fin sèche, pour lire volume, hauteur et queue de réverbération.

Dépend de `soundfile` pour l'encodage Vorbis : `pip install --target <dir> soundfile`, puis
`PYTHONPATH=<dir> python3 gen_fixtures.py`. Les fichiers sont commis : personne n'a besoin de
relancer ce script pour faire tourner les tests.
"""
import math
import os
import random
import struct

import soundfile as sf

RATE = 48_000
LAYER_FRAMES = RATE
NOTES_HZ = [110.0, 220.0, 330.0, 440.0]
NOTE_AMPLITUDE = 0.02
IMPULSE_AMPLITUDE = 0.6
IMPULSE_SPACING = 480
HIT_FRAMES = int(RATE * 0.12)


def write_ogg(path, frames):
    with sf.SoundFile(path, "w", samplerate=RATE, channels=1, format="OGG", subtype="VORBIS") as out:
        out.buffer_write(struct.pack("<%df" % len(frames), *frames), dtype="float32")


def layer(index):
    freq = NOTES_HZ[index]
    frames = [NOTE_AMPLITUDE * math.sin(2 * math.pi * freq * n / RATE) for n in range(LAYER_FRAMES)]
    frames[index * IMPULSE_SPACING] = IMPULSE_AMPLITUDE
    return frames


def hit():
    rng = random.Random(7)
    return [0.9 * (rng.random() * 2 - 1) * math.exp(-8.0 * n / HIT_FRAMES) for n in range(HIT_FRAMES)]


here = os.path.dirname(os.path.abspath(__file__))
for index in range(4):
    write_ogg(os.path.join(here, f"layer_{index}.ogg"), layer(index))
write_ogg(os.path.join(here, "hit.ogg"), hit())
