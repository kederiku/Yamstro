"""Génère les assets d'essai du banc audio (TASK-95), sans dépendance : module `wave` seul.

Quatre stems WAV mono 48 kHz de durée strictement identique (4 s, 192 000 trames). Chaque stem
porte une note continue, en nombre entier de périodes (le bouclage est donc sans couture par
construction), et une impulsion à la trame k * 480 : décalées de 10 ms, les quatre impulsions
se séparent dans la somme et donnent la phase de chaque stem à l'échantillon près.
`mult_hit.wav` est un coup de bruit de 120 ms à fin sèche, pour lire la queue de réverbération.

Usage : python3 gen_assets.py [répertoire]   (défaut : assets)
L'OGG de la pièce « décodeur » s'encode à part, voir le README du banc.
"""
import math
import os
import random
import struct
import sys
import wave

RATE = 48_000
STEM_SECONDS = 4
STEM_FRAMES = RATE * STEM_SECONDS
NOTES_HZ = [110.0, 220.0, 330.0, 440.0]
NOTE_AMPLITUDE = 0.08
IMPULSE_AMPLITUDE = 0.6
IMPULSE_SPACING = 480
HIT_SECONDS = 0.12


def write_wav(path, frames):
    with wave.open(path, "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(RATE)
        out.writeframes(b"".join(struct.pack("<h", int(round(v * 32767))) for v in frames))


def stem(index):
    freq = NOTES_HZ[index]
    frames = [NOTE_AMPLITUDE * math.sin(2 * math.pi * freq * n / RATE) for n in range(STEM_FRAMES)]
    frames[index * IMPULSE_SPACING] = IMPULSE_AMPLITUDE
    return frames


def mult_hit():
    rng = random.Random(7)
    count = int(RATE * HIT_SECONDS)
    return [0.9 * (rng.random() * 2 - 1) * math.exp(-8.0 * n / count) for n in range(count)]


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "assets"
    os.makedirs(out, exist_ok=True)
    for index in range(len(NOTES_HZ)):
        write_wav(os.path.join(out, f"stem_{index}.wav"), stem(index))
    write_wav(os.path.join(out, "mult_hit.wav"), mult_hit())


if __name__ == "__main__":
    main()
