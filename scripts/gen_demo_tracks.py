#!/usr/bin/env python3
"""Generate the demo tracks (stdlib only, deterministic).

    python3 scripts/gen_demo_tracks.py

Writes tracks/track_a.wav (174 BPM neurofunk loop: kick/snare/hats + reese
bass with filter movement) and tracks/track_b.wav (128 BPM house loop:
four-on-the-floor, offbeat hats, clap, sub bassline, stab). Real transients
and dynamics so waveform rendering, BPM detection, and transient analysis
have something to work with. After regenerating, delete library.redb so the
analyzer re-scans (it caches per path).
"""
import math
import random
import struct
import wave

SR = 44100


def soft_clip(x: float) -> float:
    return math.tanh(x)


class Song:
    def __init__(self, seconds: float):
        self.n = int(seconds * SR)
        self.left = [0.0] * self.n
        self.right = [0.0] * self.n

    def add(self, at: float, mono, pan: float = 0.0, gain: float = 1.0):
        """Mix a mono sample list in at time `at` seconds, pan -1..1."""
        start = int(at * SR)
        lg = gain * min(1.0, 1.0 - pan)
        rg = gain * min(1.0, 1.0 + pan)
        for i, s in enumerate(mono):
            j = start + i
            if 0 <= j < self.n:
                self.left[j] += s * lg
                self.right[j] += s * rg

    def write(self, path: str, level: float = 0.85):
        peak = max(1e-9, max(max(map(abs, self.left)), max(map(abs, self.right))))
        k = level / peak
        with wave.open(path, "w") as w:
            w.setnchannels(2)
            w.setsampwidth(2)
            w.setframerate(SR)
            frames = bytearray()
            for l, r in zip(self.left, self.right):
                frames += struct.pack(
                    "<hh",
                    int(soft_clip(l * k) * 32767),
                    int(soft_clip(r * k) * 32767),
                )
            w.writeframes(bytes(frames))
        print("wrote", path)


def env(n: int, attack: float, decay: float) -> list:
    a = max(1, int(attack * SR))
    out = []
    for i in range(n):
        if i < a:
            out.append(i / a)
        else:
            out.append(math.exp(-(i - a) / (decay * SR)))
    return out


def kick(punch=150.0, tail=50.0, dur=0.28) -> list:
    n = int(dur * SR)
    e = env(n, 0.001, 0.09)
    out = []
    phase = 0.0
    for i in range(n):
        t = i / n
        f = punch * (1 - t) ** 2 + tail
        phase += 2 * math.pi * f / SR
        click = 0.6 * math.exp(-i / (0.004 * SR))
        out.append((math.sin(phase) + click) * e[i])
    return out


def snare(rng, dur=0.18) -> list:
    n = int(dur * SR)
    e = env(n, 0.001, 0.05)
    body_phase = 0.0
    out = []
    for i in range(n):
        body_phase += 2 * math.pi * 195 / SR
        noise = rng.uniform(-1, 1)
        out.append((0.5 * math.sin(body_phase) + 0.8 * noise) * e[i])
    return out


def hat(rng, dur=0.05, open_=False) -> list:
    n = int((0.22 if open_ else dur) * SR)
    e = env(n, 0.0005, 0.09 if open_ else 0.014)
    # crude metallic noise: sum of detuned squares
    phases = [0.0] * 5
    freqs = [5217.0, 6733.0, 8121.0, 9210.0, 10583.0]
    out = []
    for i in range(n):
        s = 0.0
        for k in range(5):
            phases[k] += 2 * math.pi * freqs[k] / SR
            s += 1.0 if math.sin(phases[k]) > 0 else -1.0
        s = s / 5 * 0.7 + rng.uniform(-1, 1) * 0.3
        out.append(s * e[i])
    return out


def reese(freq: float, dur: float, cutoff_lfo_hz: float, detune=1.012) -> list:
    """Two detuned saws through a moving one-pole low-pass — the neuro growl."""
    n = int(dur * SR)
    out = []
    p1 = p2 = lp = 0.0
    for i in range(n):
        t = i / SR
        p1 = (p1 + freq / SR) % 1.0
        p2 = (p2 + freq * detune / SR) % 1.0
        raw = (p1 * 2 - 1) * 0.5 + (p2 * 2 - 1) * 0.5
        cutoff = 250 + 900 * (0.5 + 0.5 * math.sin(2 * math.pi * cutoff_lfo_hz * t))
        a = min(0.99, 2 * math.pi * cutoff / SR)
        lp += a * (raw - lp)
        edge = 1.0
        if i < 200:
            edge = i / 200
        if i > n - 400:
            edge = (n - i) / 400
        out.append(lp * edge)
    return out


def sub(freq: float, dur: float) -> list:
    n = int(dur * SR)
    out = []
    ph = 0.0
    for i in range(n):
        ph += 2 * math.pi * freq / SR
        edge = min(1.0, i / 150, (n - i) / 400)
        out.append(math.sin(ph) * edge)
    return out


def stab(root: float, dur=0.22) -> list:
    n = int(dur * SR)
    e = env(n, 0.002, 0.07)
    phases = [0.0] * 3
    freqs = [root, root * 1.26, root * 1.5]  # rough minor-ish stab
    out = []
    for i in range(n):
        s = 0.0
        for k in range(3):
            phases[k] += 2 * math.pi * freqs[k] / SR
            s += (2 * ((phases[k] / (2 * math.pi)) % 1.0) - 1) / 3
        out.append(s * e[i])
    return out


def make_neuro(path: str):
    rng = random.Random(0x174)
    bpm = 174.0
    beat = 60.0 / bpm
    bars = 12
    song = Song(bars * 4 * beat + 0.5)

    # E1 root with simple movement every 2 bars
    bassline = [41.2, 41.2, 49.0, 36.7, 41.2, 41.2, 55.0, 49.0]
    for bar in range(bars):
        t0 = bar * 4 * beat
        # amen-ish two-step: kick 1 and 2.5, snare 2 and 4
        song.add(t0 + 0 * beat, kick(), gain=1.0)
        song.add(t0 + 2.5 * beat, kick(punch=130), gain=0.9)
        song.add(t0 + 1 * beat, snare(rng), gain=0.85)
        song.add(t0 + 3 * beat, snare(rng), gain=0.9)
        if bar % 4 == 3:  # fill
            song.add(t0 + 3.5 * beat, snare(rng, dur=0.1), gain=0.5)
            song.add(t0 + 3.75 * beat, snare(rng, dur=0.1), gain=0.6)
        # 1/16 hats, velocity-humanized, alternating pan
        for s16 in range(16):
            g = 0.28 + 0.18 * rng.random() + (0.12 if s16 % 4 == 2 else 0)
            song.add(t0 + s16 * beat / 4, hat(rng, open_=(s16 == 14 and bar % 2)),
                     pan=0.35 if s16 % 2 else -0.35, gain=g)
        # reese: two-bar phrases, half-bar notes with rests
        if bar % 2 == 0:
            root = bassline[(bar // 2) % len(bassline)]
            for half in range(4):
                if half == 3 and bar % 4 == 2:
                    continue  # breathe
                song.add(t0 + half * 2 * beat, reese(root, 1.6 * beat, cutoff_lfo_hz=1.3 + 0.4 * half),
                         gain=0.55)
                song.add(t0 + half * 2 * beat, sub(root, 1.7 * beat), gain=0.5)
    song.write(path)


def make_house(path: str):
    rng = random.Random(0x128)
    bpm = 128.0
    beat = 60.0 / bpm
    bars = 8
    song = Song(bars * 4 * beat + 0.5)

    bassline = [55.0, 55.0, 65.4, 49.0]  # A1 pattern
    for bar in range(bars):
        t0 = bar * 4 * beat
        for b in range(4):
            song.add(t0 + b * beat, kick(punch=120, tail=48, dur=0.24), gain=1.0)
            song.add(t0 + (b + 0.5) * beat, hat(rng, open_=True), pan=0.2, gain=0.4)
        song.add(t0 + 1 * beat, snare(rng, dur=0.13), gain=0.6)  # clap-ish
        song.add(t0 + 3 * beat, snare(rng, dur=0.13), gain=0.6)
        root = bassline[bar % 4]
        for b in range(4):
            song.add(t0 + (b + 0.55) * beat, sub(root, 0.35 * beat), gain=0.75)
        if bar % 2 == 1:
            for echo in range(3):
                song.add(t0 + 2 * beat + echo * 0.75 * beat, stab(220.0),
                         pan=(-0.4, 0.1, 0.5)[echo], gain=0.35 * (0.6 ** echo))
    song.write(path)


def make_halftime(path: str):
    """140 BPM halftime: sparse heavy kicks, snare on the 3, reese swells."""
    rng = random.Random(0x140)
    bpm = 140.0
    beat = 60.0 / bpm
    bars = 8
    song = Song(bars * 4 * beat + 0.6)

    roots = [41.2, 41.2, 49.0, 36.7]  # E1 / G1 / D1 movement
    for bar in range(bars):
        t0 = bar * 4 * beat
        song.add(t0, kick(punch=140, tail=42, dur=0.3), gain=1.0)
        if bar % 2 == 1:
            song.add(t0 + 3.5 * beat, kick(punch=140, tail=42, dur=0.22), gain=0.8)
        song.add(t0 + 2 * beat, snare(rng, dur=0.22), gain=0.85)
        for h in range(8):
            if h % 2 == 0 or rng.random() < 0.35:
                song.add(t0 + h * 0.5 * beat, hat(rng), pan=(-0.3 if h % 4 else 0.3), gain=0.28)
        root = roots[bar % 4]
        song.add(t0 + 0.02, reese(root, 1.6 * beat, 0.7), gain=0.5)
        song.add(t0 + 2.5 * beat, reese(root * 1.5, 1.2 * beat, 1.3), pan=0.25, gain=0.35)
    song.write(path)


def make_boombap(path: str):
    """92 BPM boom bap: swung kicks, cracking snare on 2 & 4, dusty stabs."""
    rng = random.Random(0x92)
    bpm = 92.0
    beat = 60.0 / bpm
    bars = 8
    swing = 0.09 * beat
    song = Song(bars * 4 * beat + 0.6)

    for bar in range(bars):
        t0 = bar * 4 * beat
        song.add(t0, kick(punch=110, tail=55, dur=0.26), gain=0.95)
        song.add(t0 + 1.75 * beat + swing, kick(punch=110, tail=55, dur=0.2), gain=0.7)
        if bar % 2 == 1:
            song.add(t0 + 3.25 * beat + swing, kick(punch=110, tail=55, dur=0.2), gain=0.6)
        song.add(t0 + 1 * beat, snare(rng, dur=0.19), gain=0.9)
        song.add(t0 + 3 * beat, snare(rng, dur=0.19), gain=0.9)
        for h in range(8):
            off = swing if h % 2 else 0.0
            song.add(t0 + h * 0.5 * beat + off, hat(rng, dur=0.04), pan=0.25, gain=0.3)
        song.add(t0 + 0.5 * beat, sub(55.0, 0.4 * beat), gain=0.6)
        song.add(t0 + 2.5 * beat + swing, sub(61.7, 0.4 * beat), gain=0.55)
        if bar % 4 == 2:
            song.add(t0 + 2 * beat, stab(196.0, dur=0.3), pan=-0.35, gain=0.4)
    song.write(path)


if __name__ == "__main__":
    make_neuro("tracks/track_a.wav")
    make_house("tracks/track_b.wav")
    make_halftime("tracks/track_c.wav")
    make_boombap("tracks/track_d.wav")
