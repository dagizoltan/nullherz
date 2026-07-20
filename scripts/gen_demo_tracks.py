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


def pad(freqs, dur: float, attack=0.8, release=1.2, detune=1.004) -> list:
    """Slow-attack detuned-saw chord pad."""
    n = int(dur * SR)
    a = int(attack * SR)
    r = int(release * SR)
    phases = [0.0] * (len(freqs) * 2)
    out = []
    for i in range(n):
        s = 0.0
        for k, f in enumerate(freqs):
            for d, ph_idx in ((1.0, k * 2), (detune, k * 2 + 1)):
                phases[ph_idx] = (phases[ph_idx] + f * d / SR) % 1.0
                s += (phases[ph_idx] * 2 - 1) / (len(freqs) * 2)
        e = min(1.0, i / max(1, a), (n - i) / max(1, r))
        out.append(s * e * 0.6)
    return out


def acid(root: float, dur: float, res_sweep_hz: float, rng) -> list:
    """Single resonant-ish saw line: crude 303 flavour via a swept one-pole
    plus an octave-jumping pattern baked into the phase."""
    n = int(dur * SR)
    out = []
    p = lp = 0.0
    for i in range(n):
        t = i / SR
        f = root * (2.0 if (int(t * 8) % 4 == 3) else 1.0)
        p = (p + f / SR) % 1.0
        raw = p * 2 - 1
        cutoff = 180 + 1400 * (0.5 + 0.5 * math.sin(2 * math.pi * res_sweep_hz * t + rng.random() * 0.1))
        a = min(0.99, 2 * math.pi * cutoff / SR)
        lp += a * (raw - lp)
        edge = min(1.0, i / 100, (n - i) / 300)
        out.append((lp * 1.4 + raw * 0.1) * edge)
    return out


def sub808(freq: float, dur: float, glide_to: float = 0.0) -> list:
    """808-style sub with a click and optional pitch glide."""
    n = int(dur * SR)
    out = []
    ph = 0.0
    for i in range(n):
        t = i / n
        f = freq if glide_to <= 0.0 else freq * (1 - t) + glide_to * t
        ph += 2 * math.pi * f / SR
        e = math.exp(-i / (0.45 * SR))
        click = 0.4 * math.exp(-i / (0.003 * SR))
        out.append((math.sin(ph) + click) * e)
    return out


def make_techno(path: str):
    """132 BPM techno: rumbling four-on-the-floor, offbeat metallic hats,
    acid line rising over 8 bars."""
    rng = random.Random(0x132)
    bpm = 132.0
    beat = 60.0 / bpm
    bars = 8
    song = Song(bars * 4 * beat + 0.5)
    for bar in range(bars):
        t0 = bar * 4 * beat
        for b in range(4):
            song.add(t0 + b * beat, kick(punch=100, tail=45, dur=0.32), gain=1.0)
            song.add(t0 + (b + 0.5) * beat, hat(rng, open_=(b == 3 and bar % 2)), pan=0.25, gain=0.35)
            song.add(t0 + b * beat + 0.02, sub(41.2, 0.5 * beat), gain=0.5)  # rumble tail
        if bar % 2 == 1:
            song.add(t0 + 1 * beat, snare(rng, dur=0.1), pan=-0.2, gain=0.35)  # rim-ish
        song.add(t0, acid(55.0, 4 * beat, 0.25 + bar * 0.08, rng), pan=-0.1, gain=0.34)
    song.write(path)


def make_trance(path: str):
    """138 BPM trance: rolling offbeat bass, 1/16 arp stabs, snare lift each 4."""
    rng = random.Random(0x138)
    bpm = 138.0
    beat = 60.0 / bpm
    bars = 8
    song = Song(bars * 4 * beat + 0.6)
    arp = [220.0, 277.2, 329.6, 440.0]  # A minor-ish
    for bar in range(bars):
        t0 = bar * 4 * beat
        for b in range(4):
            song.add(t0 + b * beat, kick(punch=115, tail=50, dur=0.26), gain=0.95)
            song.add(t0 + (b + 0.5) * beat, sub(55.0, 0.3 * beat), gain=0.7)  # offbeat bass
        for s16 in range(16):
            song.add(t0 + s16 * beat / 4, stab(arp[s16 % 4] * (2 if s16 % 8 >= 4 else 1), dur=0.1),
                     pan=(-0.4 + 0.8 * ((s16 % 3) / 2.0)), gain=0.22)
        if bar % 4 == 3:  # lift
            for r in range(8):
                song.add(t0 + (2 + r * 0.25) * beat, snare(rng, dur=0.09), gain=0.25 + r * 0.06)
    song.write(path)


def make_jungle(path: str):
    """160 BPM jungle: chopped-break feel from kicks/snares/hats, deep sub."""
    rng = random.Random(0x160)
    bpm = 160.0
    beat = 60.0 / bpm
    bars = 8
    song = Song(bars * 4 * beat + 0.5)
    # 1/16 step masks for a chopped two-bar break (k=kick, s=snare, h=hat)
    kicks_a = [0, 10]
    snares_a = [4, 7, 12, 15]
    kicks_b = [0, 6, 11]
    snares_b = [4, 9, 12, 14]
    for bar in range(bars):
        t0 = bar * 4 * beat
        kicks, snares = (kicks_a, snares_a) if bar % 2 == 0 else (kicks_b, snares_b)
        for s16 in kicks:
            song.add(t0 + s16 * beat / 4, kick(punch=120, tail=48, dur=0.2), gain=0.9)
        for s16 in snares:
            song.add(t0 + s16 * beat / 4, snare(rng, dur=0.11), pan=0.15 if s16 % 2 else -0.15, gain=0.7)
        for s16 in range(16):
            if rng.random() < 0.7:
                song.add(t0 + s16 * beat / 4, hat(rng, dur=0.03), pan=0.3, gain=0.2)
        root = (36.7, 41.2, 32.7, 41.2)[bar % 4]
        song.add(t0 + 0.02, sub(root, 1.5 * beat), gain=0.8)
        song.add(t0 + 2 * beat, sub(root, 1.2 * beat), gain=0.7)
    song.write(path)


def make_trap(path: str):
    """140 BPM trap (half-time feel): 808 glides, hat rolls, clap on 3."""
    rng = random.Random(0x140 + 1)
    bpm = 140.0
    beat = 60.0 / bpm
    bars = 8
    song = Song(bars * 4 * beat + 0.7)
    roots = [55.0, 55.0, 65.4, 49.0]
    for bar in range(bars):
        t0 = bar * 4 * beat
        song.add(t0, kick(punch=95, tail=48, dur=0.3), gain=0.95)
        if bar % 2 == 1:
            song.add(t0 + 3.5 * beat, kick(punch=95, tail=48, dur=0.2), gain=0.7)
        song.add(t0 + 2 * beat, snare(rng, dur=0.16), gain=0.85)  # clap-ish on 3
        root = roots[bar % 4]
        song.add(t0 + 0.01, sub808(root, 1.8 * beat, glide_to=root * (0.75 if bar % 4 == 2 else 1.0)), gain=0.85)
        # hats: 1/8 base with occasional 1/32 rolls
        s = 0.0
        while s < 4.0:
            if rng.random() < 0.15:
                for r in range(4):  # roll
                    song.add(t0 + (s + r * 0.125 / 2) * beat, hat(rng, dur=0.02), pan=0.2, gain=0.22)
                s += 0.5
            else:
                song.add(t0 + s * beat, hat(rng, dur=0.03), pan=0.2, gain=0.3)
                s += 0.5
    song.write(path)


def make_ambient(path: str):
    """80 BPM ambient: slow pads and a soft pulse — exercises the analyzers'
    low-transient path (sparse onsets, sustained spectrum)."""
    rng = random.Random(0x80)
    bpm = 80.0
    beat = 60.0 / bpm
    bars = 8
    song = Song(bars * 4 * beat + 1.5)
    chords = [
        (110.0, 164.8, 220.0),   # A min-ish
        (98.0, 146.8, 196.0),    # G
        (87.3, 130.8, 174.6),    # F
        (98.0, 155.6, 196.0),    # G sus-ish
    ]
    for bar in range(bars):
        t0 = bar * 4 * beat
        song.add(t0, pad(chords[bar % 4], 4.2 * beat), pan=-0.15 if bar % 2 else 0.15, gain=0.5)
        song.add(t0, kick(punch=70, tail=40, dur=0.4), gain=0.35)  # soft pulse
        if bar % 2 == 1:
            song.add(t0 + 2 * beat, hat(rng, open_=True), pan=0.4, gain=0.12)
        song.add(t0 + rng.random() * 3 * beat, stab(440.0 * (1.5 if bar % 4 == 2 else 1.0), dur=0.5),
                 pan=rng.uniform(-0.5, 0.5), gain=0.1)
    song.write(path)


def make_dub(path: str):
    """75 BPM dub: one-drop kick, offbeat skank stabs with echoes, deep sub."""
    rng = random.Random(0x75)
    bpm = 75.0
    beat = 60.0 / bpm
    bars = 8
    song = Song(bars * 4 * beat + 1.0)
    for bar in range(bars):
        t0 = bar * 4 * beat
        song.add(t0 + 2 * beat, kick(punch=90, tail=50, dur=0.3), gain=0.9)  # one drop
        song.add(t0 + 2 * beat, snare(rng, dur=0.14), gain=0.5)
        for b in (1, 3):  # skank + echo tail
            for echo in range(3):
                song.add(t0 + (b + echo * 0.75) * beat, stab(261.6, dur=0.12),
                         pan=(-0.3, 0.2, 0.5)[echo], gain=0.3 * (0.55 ** echo))
        root = (55.0, 49.0)[bar % 2]
        song.add(t0 + 0.5 * beat, sub(root, 1.2 * beat), gain=0.8)
        song.add(t0 + 2.5 * beat, sub(root, 0.9 * beat), gain=0.7)
        if rng.random() < 0.5:
            song.add(t0 + 3.5 * beat, hat(rng, open_=True), pan=0.35, gain=0.15)
    song.write(path)


if __name__ == "__main__":
    make_neuro("tracks/track_a.wav")
    make_house("tracks/track_b.wav")
    make_halftime("tracks/track_c.wav")
    make_boombap("tracks/track_d.wav")
    make_techno("tracks/track_e.wav")
    make_trance("tracks/track_f.wav")
    make_jungle("tracks/track_g.wav")
    make_trap("tracks/track_h.wav")
    make_ambient("tracks/track_i.wav")
    make_dub("tracks/track_j.wav")
