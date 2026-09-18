#!/usr/bin/env python3
"""Génère les 21 fichiers de `assets/audio/` : dix-sept effets et quatre couches de musique.

Tout est **écrit par le projet** : aucune source tierce, aucun échantillon, que de la synthèse.
C'est ce qui rend la provenance certaine dans `assets/CREDITS.md`. Chaque fichier se remplace un
jour sous le même nom, sans toucher au code du jeu, qui charge par chemin littéral.

Usage :
    PYTHONPATH=<dossier pip> python3 gen_audio.py --out ../../assets/audio
    PYTHONPATH=<dossier pip> python3 gen_audio.py --probe          # table des poids, rien n'est écrit
    PYTHONPATH=<dossier pip> python3 gen_audio.py --preview <dir>  # deux mix d'écoute, hors dépôt

Dépend de `numpy` et de `soundfile` (libsndfile, qui embarque libvorbis), voir le README.
Les fichiers sont commis : personne n'a besoin de relancer ce script pour construire le jeu.

Trois contraintes tiennent la conception.

- **Le poids.** La cible de 25 Mo compte `assets/` : il reste moins de 622 ko pour tout le son
  une fois le backend lié. Les effets sont mono, brefs, sans silence de tête ni de queue ; la
  musique est mono, à `MUSIC_RATE`, sur `MUSIC_BARS` mesures. Ces trois réglages ont été retenus
  sur la mesure (`--probe`), dans l'ordre de sacrifice décidé : stéréo, fréquence, longueur.
- **La boucle.** Les quatre couches sortent du même rendu, sur un tampon **circulaire** : une note,
  un écho ou une réverbération qui dépasse la fin se replie sur le début. Aucun fondu aux bords,
  aucune queue tronquée, même nombre de trames au bit près, et une mesure vaut un nombre entier
  de trames (100 BPM).
- **La hauteur.** Le jeu rejoue certains effets jusqu'à quatre fois plus vite (deux octaves). Leur
  énergie reste sous 5,5 kHz : au plafond elle monte à 22 kHz, sous la moitié de 48 kHz.
"""
import argparse
import os
import tempfile

import numpy as np
import soundfile as sf

SFX_RATE = 48_000
# Retenus sur la mesure, voir `--probe` et le README.
MUSIC_RATE = 32_000
MUSIC_BARS = 8
MUSIC_LEVEL = 0.6
SFX_LEVEL = 0.4

BPM = 100
BEATS_PER_BAR = 4
SWING = 0.58  # place de la croche faible dans le temps
MIX_PEAK = 0.5  # crête des quatre couches sommées à l'unité
CHUNK = 16_384  # libsndfile tombe sur un très grand bloc Vorbis écrit d'un coup


def hz(midi):
    return 440.0 * 2.0 ** ((midi - 69) / 12.0)


# ------------------------------------------------------------------ briques


def seconds(n, rate):
    return np.arange(n) / rate


def harmonic_tone(freq, n, rate, amps, vibrato=0.0):
    """Somme de partiels harmoniques, tronquée sous la moitié de la fréquence d'échantillonnage."""
    t = seconds(n, rate)
    phase = 2 * np.pi * freq * t
    if vibrato:
        phase = phase + vibrato * np.sin(2 * np.pi * 5.0 * t)
    out = np.zeros(n)
    for k, amp in enumerate(amps, start=1):
        if freq * k < 0.45 * rate:
            out += amp * np.sin(k * phase)
    return out


def partials(freqs_amps_taus, n, rate):
    """Partiels libres, chacun avec sa décroissance : cloches, verre, métal."""
    t = seconds(n, rate)
    out = np.zeros(n)
    for freq, amp, tau in freqs_amps_taus:
        out += amp * np.sin(2 * np.pi * freq * t) * np.exp(-t / tau)
    return out


def sweep(f_start, f_end, tau_pitch, n, rate):
    """Sinus dont la fréquence tombe de `f_start` à `f_end` : le corps d'un coup grave."""
    t = seconds(n, rate)
    freq = f_end + (f_start - f_end) * np.exp(-t / tau_pitch)
    return np.sin(2 * np.pi * np.cumsum(freq) / rate)


def band_noise(n, rate, lo, hi, rng):
    """Bruit blanc borné à [lo, hi], par masque spectral à flancs doux."""
    spec = np.fft.rfft(rng.standard_normal(n))
    f = np.fft.rfftfreq(n, 1.0 / rate)
    mask = 1.0 / np.sqrt(1.0 + (lo / np.maximum(f, 1e-9)) ** 8)
    mask *= 1.0 / np.sqrt(1.0 + (f / hi) ** 8)
    out = np.fft.irfft(spec * mask, n)
    return out / (np.max(np.abs(out)) + 1e-12)


def lowpass(sig, rate, cutoff):
    """Passe-bas d'ordre 4 à phase nulle, circulaire : il ne crée pas de bord dans une boucle."""
    spec = np.fft.rfft(sig)
    f = np.fft.rfftfreq(len(sig), 1.0 / rate)
    return np.fft.irfft(spec / np.sqrt(1.0 + (f / cutoff) ** 8), len(sig))


def env_exp(n, rate, attack, tau):
    t = seconds(n, rate)
    return np.minimum(t / attack, 1.0) * np.exp(-t / tau)


def env_hold(n, rate, attack, release):
    t = seconds(n, rate)
    end = n / rate
    return np.minimum(np.minimum(t / attack, 1.0), np.maximum((end - t) / release, 0.0))


def close_edges(sig, rate, attack=0.0005, release=0.004):
    """Ouvre et ferme un effet sans claquer, et sans lui laisser de silence."""
    n = len(sig)
    a = max(1, int(attack * rate))
    r = max(1, int(release * rate))
    out = sig.copy()
    out[:a] *= np.linspace(0.0, 1.0, a)
    out[n - r :] *= np.linspace(1.0, 0.0, r)
    return out


def at_peak(sig, peak):
    return sig * (peak / (np.max(np.abs(sig)) + 1e-12))


def mix_at(total_seconds, rate, *events):
    """Pose des sons à des instants donnés dans un tampon droit : `(instant, signal, gain)`."""
    out = np.zeros(int(total_seconds * rate))
    for when, sig, gain in events:
        start = int(when * rate)
        stop = min(len(out), start + len(sig))
        out[start:stop] += gain * sig[: stop - start]
    return out


# ------------------------------------------------------------------ effets


def dice_roll(seed):
    """Des dés qui rebondissent : impacts de bruit filtré, de plus en plus serrés et faibles."""
    rng = np.random.default_rng(100 + seed)
    rate = SFX_RATE
    count = 3 + seed % 4
    total = 0.25 + 0.04 * (seed % 6)
    gaps = np.sort(rng.uniform(0.35, 1.0, count))[::-1]
    times = np.concatenate(([0.0], np.cumsum(gaps)))[:count]
    times = times / (times[-1] + gaps[-1]) * (total - 0.06)
    events = []
    for index, when in enumerate(times):
        n = int(0.06 * rate)
        centre = rng.uniform(1_800, 3_400)
        knock = band_noise(n, rate, centre * 0.6, centre * 1.5, rng) * env_exp(n, rate, 0.0004, 0.009)
        body = sweep(rng.uniform(320, 420), 190, 0.01, n, rate) * env_exp(n, rate, 0.0004, 0.014)
        gain = 0.85 ** index * rng.uniform(0.8, 1.0)
        events.append((when, knock + 0.7 * body, gain))
    return at_peak(close_edges(mix_at(total, rate, *events), rate), 0.5)


def chip_tick():
    """Le tic de jeton : pincé bref sur la3, trois partiels, propre à quatre fois sa vitesse."""
    n = int(0.09 * SFX_RATE)
    tone = harmonic_tone(hz(69), n, SFX_RATE, [1.0, 0.45, 0.2]) * env_exp(n, SFX_RATE, 0.001, 0.022)
    return at_peak(close_edges(tone, SFX_RATE), 0.5)


def chord(notes, duration, amps, attack, tau, detune=0.0):
    n = int(duration * SFX_RATE)
    out = np.zeros(n)
    for index, note in enumerate(notes):
        freq = hz(note) * (1.0 + detune * (index - len(notes) / 2))
        out += harmonic_tone(freq, n, SFX_RATE, amps)
    return close_edges(out * env_exp(n, SFX_RATE, attack, tau), SFX_RATE, release=0.02)


def relic_chord():
    """L'accord d'une relique : triade de la mineur, douce, qui ponctue."""
    return at_peak(chord([57, 60, 64], 0.30, [1.0, 0.3, 0.1], 0.008, 0.10), 0.45)


def hand_base_chord():
    """L'accord d'ouverture : quinte ouverte et seconde, attaque claire. Il annonce la main."""
    return at_peak(chord([57, 64, 69, 71], 0.45, [1.0, 0.6, 0.35, 0.2, 0.1], 0.002, 0.17, 0.0015), 0.5)


def mult_hit():
    """Le coup multiplicatif : corps grave et claquement, **sec**. La résonance vient du bus."""
    rng = np.random.default_rng(7)
    n = int(0.16 * SFX_RATE)
    body = sweep(170, 58, 0.035, n, SFX_RATE) * env_exp(n, SFX_RATE, 0.001, 0.05)
    snap = band_noise(n, SFX_RATE, 1_500, 5_000, rng) * env_exp(n, SFX_RATE, 0.0004, 0.012)
    return at_peak(close_edges(body + 0.55 * snap, SFX_RATE), 0.55)


def seal_tick():
    """Le tintement d'un sceau : verre, partiels inharmoniques, sous 5 kHz."""
    n = int(0.12 * SFX_RATE)
    glass = partials([(880.0, 1.0, 0.03), (2_429.0, 0.5, 0.02), (4_752.0, 0.25, 0.012)], n, SFX_RATE)
    return at_peak(close_edges(glass, SFX_RATE), 0.45)


def die_lock():
    """Le verrou d'un dé : déclic mécanique en deux temps, le second plus bas."""
    first = partials([(2_500.0, 1.0, 0.003)], int(0.03 * SFX_RATE), SFX_RATE)
    second = partials([(900.0, 1.0, 0.008), (1_850.0, 0.4, 0.004)], int(0.04 * SFX_RATE), SFX_RATE)
    out = mix_at(0.075, SFX_RATE, (0.0, first, 0.7), (0.033, second, 1.0))
    return at_peak(close_edges(out, SFX_RATE), 0.45)


def hand_consumed():
    """La case consommée : loquet sec et bas. Ni le tic de jeton, ni la fanfare."""
    rng = np.random.default_rng(11)
    n = int(0.09 * SFX_RATE)
    thud = sweep(190, 120, 0.02, n, SFX_RATE) * env_exp(n, SFX_RATE, 0.0006, 0.02)
    click = band_noise(n, SFX_RATE, 700, 2_400, rng) * env_exp(n, SFX_RATE, 0.0003, 0.004)
    clunk = sweep(130, 85, 0.02, n, SFX_RATE) * env_exp(n, SFX_RATE, 0.0006, 0.028)
    out = mix_at(0.14, SFX_RATE, (0.0, thud + 0.5 * click, 1.0), (0.045, clunk + 0.25 * click, 0.85))
    return at_peak(close_edges(out, SFX_RATE), 0.5)


def ui_hover():
    n = int(0.03 * SFX_RATE)
    return at_peak(close_edges(harmonic_tone(1_200.0, n, SFX_RATE, [1.0]) * env_exp(n, SFX_RATE, 0.002, 0.008), SFX_RATE), 0.2)


def ui_click():
    n = int(0.05 * SFX_RATE)
    tone = harmonic_tone(800.0, n, SFX_RATE, [1.0, 0.5, 0.25]) * env_exp(n, SFX_RATE, 0.0005, 0.01)
    return at_peak(close_edges(tone, SFX_RATE), 0.3)


def coin():
    """Deux notes montantes, si5 puis mi6, à partiels impairs."""
    odd = [1.0, 0.0, 0.33, 0.0, 0.15]
    n1, n2 = int(0.07 * SFX_RATE), int(0.22 * SFX_RATE)
    first = harmonic_tone(hz(83), n1, SFX_RATE, odd) * env_exp(n1, SFX_RATE, 0.001, 0.05)
    second = harmonic_tone(hz(88), n2, SFX_RATE, odd) * env_exp(n2, SFX_RATE, 0.001, 0.06)
    out = mix_at(0.29, SFX_RATE, (0.0, first, 0.8), (0.07, second, 1.0))
    return at_peak(close_edges(out, SFX_RATE, release=0.01), 0.4)


def victory_fanfare():
    """Arpège montant de do majeur, le relatif, puis l'accord tenu. **Moins de 1,2 s** : il tient
    dans la fenêtre du ducking, et ses notes sont celles de la mineur septième : il ne frotte pas."""
    brass = [1.0, 0.7, 0.5, 0.3, 0.18, 0.1]
    step = 0.15
    events = []
    for index, note in enumerate([72, 76, 79]):
        n = int(0.2 * SFX_RATE)
        events.append((index * step, harmonic_tone(hz(note), n, SFX_RATE, brass) * env_exp(n, SFX_RATE, 0.004, 0.09), 0.8))
    n = int(0.7 * SFX_RATE)
    held = np.zeros(n)
    for note in [72, 76, 79, 84]:
        held += harmonic_tone(hz(note), n, SFX_RATE, brass)
    events.append((3 * step, held * env_exp(n, SFX_RATE, 0.004, 0.24), 0.45))
    return at_peak(close_edges(mix_at(1.15, SFX_RATE, *events), SFX_RATE, release=0.03), 0.5)


def all_sfx():
    sounds = {f"dice_roll_{i:02d}": dice_roll(i) for i in range(1, 7)}
    sounds.update(
        chip_tick=chip_tick(),
        hand_base_chord=hand_base_chord(),
        relic_chord=relic_chord(),
        mult_hit=mult_hit(),
        seal_tick=seal_tick(),
        die_lock=die_lock(),
        hand_consumed=hand_consumed(),
        ui_hover=ui_hover(),
        ui_click=ui_click(),
        coin=coin(),
        victory_fanfare=victory_fanfare(),
    )
    return sounds


# ------------------------------------------------------------------ musique

CHORDS = {
    "Am7": (45, [57, 60, 64, 67]),
    "Dm7": (50, [57, 60, 62, 65]),
    "Fmaj7": (41, [57, 60, 64, 65]),
    "G6": (43, [55, 59, 62, 64]),
    "E7": (40, [56, 59, 62, 64]),
}
PROGRESSION = ["Am7", "Am7", "Dm7", "Dm7", "Fmaj7", "G6", "Am7", "E7"]

# Le chant, par mesure de la grille : (temps, note, durée en temps).
LEAD = [
    [(0.0, 76, 1.0), (1.5, 79, 0.5), (2.0, 81, 1.5), (3.5, 79, 0.5)],
    [(0.0, 76, 2.0), (2.5, 74, 0.5), (3.0, 72, 1.0)],
    [(0.0, 77, 1.5), (1.5, 76, 0.5), (2.0, 74, 2.0)],
    [(0.0, 72, 1.0), (1.0, 74, 1.0), (2.0, 69, 2.0)],
    [(0.5, 72, 0.5), (1.0, 76, 1.0), (2.0, 81, 2.0)],
    [(0.0, 79, 1.5), (1.5, 76, 0.5), (2.0, 74, 1.0), (3.0, 71, 1.0)],
    [(0.0, 72, 3.0), (3.0, 71, 1.0)],
    [(0.0, 80, 2.0), (2.0, 83, 1.0), (3.0, 76, 1.0)],
]
ARP = [0, 1, 2, 3, 4, 5, 6, 7, 6, 5, 4, 3, 2, 1, 2, 3]


class Loop:
    """Un tampon circulaire : ce qui dépasse la fin se replie sur le début."""

    def __init__(self, rate, bars):
        assert rate * 60 % BPM == 0, "un temps doit valoir un nombre entier de trames"
        self.rate = rate
        self.beat = rate * 60 // BPM
        self.bars = bars
        self.frames = self.beat * BEATS_PER_BAR * bars
        self.buf = np.zeros(self.frames)

    def place(self, bar, beat, sig, gain=1.0):
        start = int(round((bar * BEATS_PER_BAR + beat) * self.beat)) % self.frames
        assert len(sig) <= self.frames
        head = min(len(sig), self.frames - start)
        self.buf[start : start + head] += gain * sig[:head]
        self.buf[: len(sig) - head] += gain * sig[head:]

    def echo(self, delay_beats, feedback, taps=4):
        shift = int(round(delay_beats * self.beat))
        out = self.buf.copy()
        for tap in range(1, taps + 1):
            out += feedback**tap * np.roll(self.buf, tap * shift)
        self.buf = out

    def reverb(self, length, mix, rng):
        m = int(length * self.rate)
        ir = np.zeros(self.frames)
        ir[:m] = rng.standard_normal(m) * np.exp(-seconds(m, self.rate) * 6.9 / length)
        ir = lowpass(ir, self.rate, 5_000.0)
        ir /= np.sqrt(np.sum(ir**2))
        wet = np.fft.irfft(np.fft.rfft(self.buf) * np.fft.rfft(ir), self.frames)
        self.buf = self.buf + mix * wet


def chord_of(bar):
    return CHORDS[PROGRESSION[bar % len(PROGRESSION)]]


def kick(rate, punch=1.0):
    n = int(0.22 * rate)
    return sweep(45 + 65 * punch, 45, 0.03, n, rate) * env_exp(n, rate, 0.001, 0.09)


def hat(rate, rng, tau, lo=5_000, hi=9_000):
    n = int(8 * tau * rate)
    return band_noise(n, rate, lo, hi, rng) * env_exp(n, rate, 0.0005, tau)


def render_base(rate, bars):
    """Base feutrée : percussions discrètes, basse ronde qui marche sur la grille."""
    rng = np.random.default_rng(21)
    loop = Loop(rate, bars)
    for bar in range(bars):
        root, _ = chord_of(bar)
        next_root, _ = chord_of(bar + 1)
        for beat in (0, 2):
            loop.place(bar, beat, kick(rate, 0.8), 0.55)
        if bar % 2 == 1:
            loop.place(bar, 3 + SWING, kick(rate, 0.6), 0.3)
        for beat in range(BEATS_PER_BAR):
            loop.place(bar, beat + SWING, hat(rate, rng, 0.012), 0.07)
            if beat in (1, 3):
                n = int(0.05 * rate)
                rim = partials([(1_700.0, 1.0, 0.004), (640.0, 0.5, 0.008)], n, rate)
                loop.place(bar, beat, rim, 0.13)
        walk = [root, root + 7, root + 12, next_root - 1]
        for beat, note in enumerate(walk):
            n = int(0.95 * loop.beat)
            tone = harmonic_tone(hz(note), n, rate, [1.0, 0.35, 0.12]) * env_exp(n, rate, 0.005, 0.32)
            loop.place(bar, beat, close_edges(tone, rate, release=0.02), 0.45)
    return loop.buf


def render_melody(rate, bars):
    """Mélodie : accords planants de synthé rétro, et un chant clairsemé qui laisse de l'air."""
    rng = np.random.default_rng(22)
    pad, lead = Loop(rate, bars), Loop(rate, bars)
    soft_saw = [1.0 / k for k in range(1, 9)]
    triangle = [1.0, 0.0, 0.11, 0.0, 0.04]
    for bar in range(bars):
        _, tones = chord_of(bar)
        n = int(1.12 * BEATS_PER_BAR * loop_beat(rate))
        for index, note in enumerate(tones):
            for detune in (-0.002, 0.002):
                tone = harmonic_tone(hz(note) * (1 + detune), n, rate, soft_saw)
                pad.place(bar, 0.0, tone * env_hold(n, rate, 0.28, 0.5), 0.06 + 0.01 * index)
        # La seconde moitié s'ouvre sur deux mesures sans chant : la grille respire.
        if bars > 8 and bar in (8, 9):
            continue
        for beat, note, length in LEAD[bar % len(LEAD)]:
            m = int(length * loop_beat(rate) * 0.98)
            tone = harmonic_tone(hz(note), m, rate, triangle, vibrato=0.04)
            lead.place(bar, beat, close_edges(tone * env_exp(m, rate, 0.012, 0.6), rate, release=0.03), 0.2)
    pad.buf = lowpass(pad.buf, rate, 2_400.0)
    lead.echo(0.75, 0.35)
    pad.buf += lead.buf
    pad.reverb(1.1, 0.22, rng)
    return pad.buf


def render_tension(rate, bars):
    """Tension : basse saturée en croches, et une nappe qui frotte, mi contre fa."""
    rng = np.random.default_rng(23)
    bass, drone = Loop(rate, bars), Loop(rate, bars)
    saw = [1.0 / k for k in range(1, 13)]
    for bar in range(bars):
        root, _ = chord_of(bar)
        for eighth in range(8):
            n = int(0.46 * loop_beat(rate))
            note = root + (12 if eighth % 4 == 3 else 0)
            tone = harmonic_tone(hz(note), n, rate, saw) * env_exp(n, rate, 0.003, 0.11)
            bass.place(bar, eighth / 2, close_edges(tone, rate, release=0.01), 0.5)
    bass.buf = lowpass(np.tanh(3.0 * bass.buf), rate, 1_300.0)
    t = seconds(drone.frames, rate)
    whole = drone.frames / rate
    for note, amp in ((64, 0.5), (65, 0.45), (81, 0.18)):
        # Un nombre entier de périodes sur la boucle : la nappe n'a pas de couture.
        cycles = round(hz(note) * whole)
        drone.buf += amp * np.sin(2 * np.pi * cycles * t / whole)
    tremolo = 0.75 + 0.25 * np.sin(2 * np.pi * round(3.0 * whole) * t / whole)
    drone.buf *= tremolo
    drone.reverb(1.4, 0.3, rng)
    return 0.34 * bass.buf + 0.16 * drone.buf


def render_climax(rate, bars):
    """Climax : arpèges en doubles croches, caisse claire et charleston serrés."""
    rng = np.random.default_rng(24)
    arp, drums = Loop(rate, bars), Loop(rate, bars)
    pluck = [1.0, 0.6, 0.4, 0.25, 0.15, 0.08]
    for bar in range(bars):
        _, tones = chord_of(bar)
        scale = tones + [note + 12 for note in tones]
        for sixteenth, degree in enumerate(ARP):
            n = int(0.3 * rate)
            tone = harmonic_tone(hz(scale[degree] + 12), n, rate, pluck) * env_exp(n, rate, 0.002, 0.07)
            accent = 1.0 if sixteenth % 4 == 0 else 0.7
            arp.place(bar, sixteenth / 4, close_edges(tone, rate, release=0.01), 0.17 * accent)
            drums.place(bar, sixteenth / 4, hat(rate, rng, 0.006, 6_000, 11_000), 0.05 * accent)
        for beat in (1, 3):
            n = int(0.16 * rate)
            crack = band_noise(n, rate, 1_000, 6_000, rng) * env_exp(n, rate, 0.0005, 0.035)
            body = sweep(230, 180, 0.02, n, rate) * env_exp(n, rate, 0.0005, 0.03)
            drums.place(bar, beat, crack + 0.6 * body, 0.3)
        drums.place(bar, 1.5, kick(rate, 1.0), 0.4)
        drums.place(bar, 3.75, kick(rate, 0.9), 0.32)
    arp.echo(0.75, 0.3, taps=3)
    arp.reverb(0.8, 0.15, rng)
    return arp.buf + drums.buf


def loop_beat(rate):
    return rate * 60 // BPM


STEMS = ["base", "melody", "tension", "climax"]


def all_stems(rate, bars):
    """Les quatre couches d'un **même rendu**, à la même longueur, mises au niveau ensemble."""
    renders = [render_base(rate, bars), render_melody(rate, bars), render_tension(rate, bars), render_climax(rate, bars)]
    assert len({len(r) for r in renders}) == 1
    scale = MIX_PEAK / np.max(np.abs(np.sum(renders, axis=0)))
    return {name: render * scale for name, render in zip(STEMS, renders)}


# ------------------------------------------------------------------ écriture


def write_ogg(path, data, rate, level):
    data = np.asarray(data, dtype=np.float32)
    channels = 1 if data.ndim == 1 else data.shape[1]
    with sf.SoundFile(path, "w", samplerate=rate, channels=channels, format="OGG", subtype="VORBIS", compression_level=level) as out:
        for start in range(0, len(data), CHUNK):
            out.write(data[start : start + CHUNK])
    return os.path.getsize(path)


def decoded_report(path, source):
    """Relit le fichier : longueur décodée, rapport signal sur bruit, saut à la couture."""
    back, _ = sf.read(path, dtype="float64", always_2d=False)
    if back.ndim > 1:
        back, source = back[:, 0], (source if source.ndim == 1 else source[:, 0])
    same = len(back) == len(source)
    n = min(len(back), len(source))
    noise = np.sum((back[:n] - source[:n]) ** 2)
    snr = 10 * np.log10(np.sum(source[:n] ** 2) / (noise + 1e-20))
    seam = abs(back[0] - back[-1])
    usual = np.percentile(np.abs(np.diff(back)), 99.9)
    return same, len(back), snr, seam, usual


def probe(tmp):
    """La table des poids : l'ordre de sacrifice décidé, du plus riche au plus léger."""
    print("| Musique | Niveau | Poids des quatre (octets) | SNR min (dB) |")
    print("| :-- | --: | --: | --: |")
    for label, rate, bars, stereo in (
        ("stéréo 48 kHz, 16 mesures", 48_000, 16, True),
        ("mono 48 kHz, 16 mesures", 48_000, 16, False),
        ("mono 32 kHz, 16 mesures", 32_000, 16, False),
        ("mono 32 kHz, 8 mesures", 32_000, 8, False),
    ):
        stems = all_stems(rate, bars)
        for level in (0.4, 0.6, 0.8, 1.0):
            total, worst = 0, 1e9
            for name, data in stems.items():
                if stereo:
                    # Élargissement de Haas : la voie droite retardée de 15 ms, circulairement.
                    data = np.stack([data, np.roll(data, int(0.015 * rate))], axis=1)
                path = os.path.join(tmp, f"probe_{name}.ogg")
                total += write_ogg(path, data, rate, level)
                worst = min(worst, decoded_report(path, data)[2])
            print(f"| {label} | {level} | {total} | {worst:.1f} |")


def generate(out):
    os.makedirs(out, exist_ok=True)
    print("| Fichier | Octets | Trames | Durée (s) | Crête | SNR (dB) |")
    print("| :-- | --: | --: | --: | --: | --: |")
    total_sfx = total_music = 0
    for name, data in all_sfx().items():
        path = os.path.join(out, f"{name}.ogg")
        size = write_ogg(path, data, SFX_RATE, SFX_LEVEL)
        same, frames, snr, _, _ = decoded_report(path, data)
        assert same, f"{name} : {frames} trames décodées pour {len(data)} écrites"
        total_sfx += size
        print(f"| {name}.ogg | {size} | {frames} | {frames / SFX_RATE:.3f} | {np.max(np.abs(data)):.2f} | {snr:.1f} |")
    for name, data in all_stems(MUSIC_RATE, MUSIC_BARS).items():
        path = os.path.join(out, f"stem_{name}.ogg")
        size = write_ogg(path, data, MUSIC_RATE, MUSIC_LEVEL)
        same, frames, snr, seam, usual = decoded_report(path, data)
        assert same, f"stem_{name} : {frames} trames décodées pour {len(data)} écrites"
        total_music += size
        print(f"| stem_{name}.ogg | {size} | {frames} | {frames / MUSIC_RATE:.3f} | {np.max(np.abs(data)):.2f} | {snr:.1f} |")
        print(f"|   couture : saut {seam:.5f}, pas usuel (99,9e centile) {usual:.5f} | | | | | |")
    print(f"\neffets : {total_sfx} octets ; musique : {total_music} octets ; total : {total_sfx + total_music} octets")


def preview(out):
    """Deux mix d'écoute, **hors dépôt** : la run ordinaire, puis le Boss au Climax."""
    os.makedirs(out, exist_ok=True)
    stems = all_stems(MUSIC_RATE, MUSIC_BARS)
    run = stems["base"] + stems["melody"]
    boss = run + stems["tension"] + stems["climax"]
    sf.write(os.path.join(out, "apercu_run.wav"), np.tile(run, 2).astype(np.float32), MUSIC_RATE)
    sf.write(os.path.join(out, "apercu_boss_climax.wav"), np.tile(boss, 2).astype(np.float32), MUSIC_RATE)
    gap = np.zeros(int(0.35 * SFX_RATE))
    reel = np.concatenate([np.concatenate([data, gap]) for data in all_sfx().values()])
    sf.write(os.path.join(out, "apercu_effets.wav"), reel.astype(np.float32), SFX_RATE)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", help="dossier des 21 fichiers, `assets/audio` du dépôt")
    parser.add_argument("--probe", action="store_true", help="table des poids, dans un dossier temporaire")
    parser.add_argument("--preview", help="dossier des mix d'écoute, hors dépôt")
    args = parser.parse_args()
    if args.probe:
        with tempfile.TemporaryDirectory() as tmp:
            probe(tmp)
    if args.out:
        generate(args.out)
    if args.preview:
        preview(args.preview)
