#!/usr/bin/env python3
"""Generate the game's placeholder audio (music loops + SFX) as OGG files.

All output is synthesized from scratch with numpy — no samples, no external
recordings — and is dedicated to the public domain (CC0 1.0). See
`assets/CREDITS.md`.

Usage:
    python3 -m venv .venv && .venv/bin/pip install numpy soundfile
    .venv/bin/python scripts/generate-audio.py

Requires: numpy, soundfile (libsndfile with Vorbis support).
"""

from __future__ import annotations

import math
import pathlib

import numpy as np
import soundfile as sf

SR = 44100
OUT = pathlib.Path(__file__).resolve().parent.parent / "assets" / "audio"

# ── helpers ──────────────────────────────────────────────────────────────────


def t(seconds: float) -> np.ndarray:
    return np.arange(int(SR * seconds)) / SR


def env(n: int, attack: float, release: float) -> np.ndarray:
    """Linear attack / exponential release envelope of n samples."""
    e = np.ones(n)
    a = max(1, int(SR * attack))
    e[:a] = np.linspace(0.0, 1.0, a)
    r = max(1, int(SR * release))
    tail = np.exp(-np.linspace(0.0, 6.0, min(r, n)))
    e[-min(r, n):] *= tail
    return e


def tone(freq: float, dur: float, vibrato: float = 0.0) -> np.ndarray:
    """A folk-ish reedy tone: fundamental + odd harmonics, slight vibrato."""
    x = t(dur)
    f = freq * (1.0 + vibrato * np.sin(2 * math.pi * 5.5 * x))
    phase = 2 * math.pi * np.cumsum(f) / SR
    return (
        np.sin(phase)
        + 0.35 * np.sin(2 * phase)
        + 0.20 * np.sin(3 * phase)
        + 0.08 * np.sin(5 * phase)
    ) / 1.63


def noise(dur: float) -> np.ndarray:
    rng = np.random.default_rng(24)  # fixed seed: deterministic output
    return rng.standard_normal(int(SR * dur))


def lowpass(sig: np.ndarray, cutoff: float) -> np.ndarray:
    """One-pole lowpass, good enough for percussion colouring."""
    alpha = 1.0 - math.exp(-2 * math.pi * cutoff / SR)
    out = np.empty_like(sig)
    acc = 0.0
    for i, s in enumerate(sig):
        acc += alpha * (s - acc)
        out[i] = acc
    return out


def normalize(sig: np.ndarray, peak: float = 0.85) -> np.ndarray:
    m = np.max(np.abs(sig))
    return sig * (peak / m) if m > 0 else sig


def write(name: str, sig: np.ndarray) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / name
    sf.write(path, normalize(sig).astype(np.float32), SR, format="OGG", subtype="VORBIS")
    print(f"wrote {path} ({path.stat().st_size / 1024:.0f} KiB)")


# ── music ────────────────────────────────────────────────────────────────────

# D natural-minor-ish modal pitches (Hz), folk flavour.
D3, E3, F3, G3, A3, Bb3, C4, D4, E4, F4, G4, A4 = (
    146.83, 164.81, 174.61, 196.00, 220.00, 233.08, 261.63, 293.66,
    329.63, 349.23, 392.00, 440.00,
)


def melody(notes: list[tuple[float, float]], vibrato: float = 0.008) -> np.ndarray:
    parts = []
    for freq, dur in notes:
        if freq == 0.0:
            parts.append(np.zeros(int(SR * dur)))
            continue
        seg = tone(freq, dur, vibrato)
        parts.append(seg * env(len(seg), 0.02, dur * 0.5))
    return np.concatenate(parts)


def drone(freq: float, dur: float) -> np.ndarray:
    x = t(dur)
    d = np.sin(2 * math.pi * freq * x) + 0.5 * np.sin(2 * math.pi * freq * 1.5 * x)
    # Slow swell so the loop point is not a hard edge.
    return d * (0.8 + 0.2 * np.sin(2 * math.pi * x / dur))


def drum(dur: float, pitch: float = 80.0) -> np.ndarray:
    x = t(dur)
    body = np.sin(2 * math.pi * pitch * np.exp(-3 * x) * x * 8)
    thump = lowpass(noise(dur), 200)
    sig = 0.8 * body + 0.6 * thump
    return sig * env(len(sig), 0.002, dur)


def place(canvas: np.ndarray, sig: np.ndarray, at: float, gain: float = 1.0) -> None:
    i = int(at * SR)
    j = min(len(canvas), i + len(sig))
    canvas[i:j] += gain * sig[: j - i]


def menu_theme() -> np.ndarray:
    """Slow doina-like: drone in D + free-ish ornamented melody, 16 s loop."""
    dur = 16.0
    canvas = np.zeros(int(SR * dur))
    canvas += 0.28 * drone(D3, dur)
    line = melody(
        [
            (A4, 1.5), (G4, 0.5), (F4, 1.0), (E4, 1.0), (D4, 2.0),
            (F4, 0.75), (E4, 0.25), (D4, 1.0), (C4, 1.0), (D4, 3.0),
            (0.0, 0.5), (A4, 0.5), (Bb3 * 2, 1.0), (A4, 1.0), (G4, 2.0),
        ],
        vibrato=0.012,
    )
    place(canvas, line, 0.0, 0.5)
    return canvas


def arena_theme() -> np.ndarray:
    """Upbeat hora-like: 2/4 drum pulse + brisk stepwise tune, 12.8 s loop."""
    bpm = 150
    beat = 60.0 / bpm
    bars = 16
    dur = bars * 2 * beat
    canvas = np.zeros(int(SR * dur))
    for b in range(bars * 2):
        place(canvas, drum(0.18, 90 if b % 2 == 0 else 60), b * beat, 0.55)
    eighth = beat / 2
    phrase = [
        D4, E4, F4, G4, A4, G4, F4, E4,
        D4, F4, A4, F4, G4, E4, D4, D4,
        A4, A4, G4, F4, E4, F4, G4, E4,
        F4, D4, E4, C4, D4, D4, D4, 0.0,
    ]
    line = melody([(f, eighth) for f in phrase] * (bars // 8), vibrato=0.006)
    place(canvas, line, 0.0, 0.45)
    canvas += 0.15 * drone(D3, dur)
    return canvas


def boss_theme() -> np.ndarray:
    """Ominous variant: low fifth drone, slow minor line, heavier drums, 12.8 s."""
    bpm = 100
    beat = 60.0 / bpm
    bars = 8
    dur = bars * 4 * beat  # 4/4
    canvas = np.zeros(int(SR * dur))
    canvas += 0.30 * drone(D3 / 2, dur) + 0.18 * drone(A3 / 2, dur)
    for b in range(bars * 4):
        gain = 0.7 if b % 4 == 0 else 0.35
        place(canvas, drum(0.30, 55), b * beat, gain)
    line = melody(
        [
            (D4, 1.2), (Bb3, 1.2), (A3 * 2, 2.4),
            (F4, 1.2), (E4, 1.2), (D4, 2.4),
            (Bb3, 1.2), (C4, 1.2), (A3 * 2, 2.4),
            (E4, 1.2), (F4, 1.2), (D4, 2.4),
        ],
        vibrato=0.01,
    )
    place(canvas, line, 0.0, 0.4)
    return canvas


# ── SFX ──────────────────────────────────────────────────────────────────────


def sfx_hit() -> np.ndarray:
    thud = lowpass(noise(0.15), 1200) * env(int(SR * 0.15), 0.001, 0.14)
    return _pad_add(thud, 0.5 * drum(0.12, 120))


def sfx_crit() -> np.ndarray:
    # A hit plus a bright metallic ring.
    ring = tone(880, 0.35, 0.02) * env(int(SR * 0.35), 0.001, 0.3)
    return _pad_add(sfx_hit(), 0.4 * ring)


def _pad_add(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    n = max(len(a), len(b))
    out = np.zeros(n)
    out[: len(a)] += a
    out[: len(b)] += b
    return out


def sfx_block() -> np.ndarray:
    # Wood thunk: short pitched knock, heavily damped.
    knock = tone(220, 0.09) * env(int(SR * 0.09), 0.001, 0.08)
    return _pad_add(lowpass(noise(0.06), 800) * env(int(SR * 0.06), 0.001, 0.05), knock)


def sfx_whoosh() -> np.ndarray:
    n = noise(0.25)
    x = t(0.25)
    sweep = lowpass(n, 600) * np.sin(math.pi * x / 0.25) ** 2
    return sweep


def sfx_rest() -> np.ndarray:
    # Breath: soft filtered noise swell.
    n = lowpass(noise(0.6), 900)
    x = t(0.6)
    return n * np.sin(math.pi * x / 0.6)


def sfx_fail() -> np.ndarray:
    # Dull "no stamina" thud: low descending buzz.
    x = t(0.25)
    f = 160 * np.exp(-2 * x)
    sig = np.sin(2 * math.pi * np.cumsum(f) / SR)
    return sig * env(len(sig), 0.002, 0.2)


def sfx_defeated() -> np.ndarray:
    return _pad_add(drum(0.5, 45) * 1.2, lowpass(noise(0.4), 500) * env(int(SR * 0.4), 0.002, 0.35))


def sfx_click() -> np.ndarray:
    tick = tone(1400, 0.03) * env(int(SR * 0.03), 0.001, 0.025)
    return tick * 0.8


def sfx_coin() -> np.ndarray:
    a = tone(1567.98, 0.12, 0.0) * env(int(SR * 0.12), 0.001, 0.1)
    b = tone(2093.0, 0.25, 0.0) * env(int(SR * 0.25), 0.001, 0.22)
    return _pad_add(a, np.concatenate([np.zeros(int(SR * 0.07)), b]))


def sting(freqs: list[float], dur_each: float, vibrato: float = 0.006) -> np.ndarray:
    return melody([(f, dur_each) for f in freqs], vibrato)


# ── main ─────────────────────────────────────────────────────────────────────


def main() -> None:
    write("music_menu.ogg", menu_theme())
    write("music_arena.ogg", arena_theme())
    write("music_boss.ogg", boss_theme())
    write("sfx_hit.ogg", sfx_hit())
    write("sfx_crit.ogg", sfx_crit())
    write("sfx_block.ogg", sfx_block())
    write("sfx_whoosh.ogg", sfx_whoosh())
    write("sfx_rest.ogg", sfx_rest())
    write("sfx_fail.ogg", sfx_fail())
    write("sfx_defeated.ogg", sfx_defeated())
    write("sfx_click.ogg", sfx_click())
    write("sfx_coin.ogg", sfx_coin())
    write("sting_victory.ogg", sting([D4, F4, A4, D4 * 2], 0.22))
    write("sting_defeat.ogg", sting([A3, G3, F3, D3], 0.4, 0.012))


if __name__ == "__main__":
    main()
