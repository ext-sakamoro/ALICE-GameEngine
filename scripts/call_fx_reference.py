#!/usr/bin/env python3
"""Independent reference for `call_fx` (src/call_fx.rs): the degraded video-call look on an RGBA8
image, in integer arithmetic only, written from the definition below and not from the Rust code.

Definition (applied in this order; alpha is kept as it is):
1. block: each block x block tile (the last ones may be smaller) takes, per channel, the floor of
   the mean of its pixels
2. chroma shift s: the red of pixel x is the red of x - s, the blue of pixel x is the blue of x + s
   (clamped to the row); green stays
3. color bits b: each channel keeps its top b bits: v & (0xFF << (8 - b))
4. scanlines every k (k > 0): rows with y % k == k - 1 are multiplied by dim / 256 (floor)
5. noise a (a > 0): a xorshift64 generator seeded with seed (or 0x9E3779B97F4A7C15 when seed is 0)
   gives one 64-bit value per pixel in row-major order; channel c (0..3, not alpha) adds
   ((value >> (16 * c)) & 0xFFFF) % (2a + 1) - a, clamped to 0..=255

    python3 scripts/call_fx_reference.py            write tests/data/call_fx/expected.txt
    python3 scripts/call_fx_reference.py --check    fail when the file is not what this writes
"""
import hashlib
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "tests" / "data" / "call_fx" / "expected.txt"
M64 = (1 << 64) - 1


def xorshift64(x):
    x ^= (x << 13) & M64
    x ^= x >> 7
    x ^= (x << 17) & M64
    return x


def apply(img, w, h, block, color_bits, scan_every, scan_dim, chroma, noise, seed):
    px = [list(img[i * 4:i * 4 + 4]) for i in range(w * h)]
    if block > 1:
        for by in range(0, h, block):
            for bx in range(0, w, block):
                cells = [(x, y) for y in range(by, min(by + block, h)) for x in range(bx, min(bx + block, w))]
                for c in range(3):
                    m = sum(px[y * w + x][c] for x, y in cells) // len(cells)
                    for x, y in cells:
                        px[y * w + x][c] = m
    if chroma:
        new = [p[:] for p in px]
        for y in range(h):
            for x in range(w):
                rx = min(max(x - chroma, 0), w - 1)
                bx = min(max(x + chroma, 0), w - 1)
                new[y * w + x][0] = px[y * w + rx][0]
                new[y * w + x][2] = px[y * w + bx][2]
        px = new
    mask = (0xFF << (8 - color_bits)) & 0xFF
    for p in px:
        for c in range(3):
            p[c] &= mask
    if scan_every > 0:
        for y in range(h):
            if y % scan_every == scan_every - 1:
                for x in range(w):
                    for c in range(3):
                        px[y * w + x][c] = px[y * w + x][c] * scan_dim // 256
    if noise > 0:
        state = seed if seed else 0x9E3779B97F4A7C15
        for p in px:
            state = xorshift64(state)
            for c in range(3):
                d = ((state >> (16 * c)) & 0xFFFF) % (2 * noise + 1) - noise
                p[c] = min(max(p[c] + d, 0), 255)
    return bytes(v for p in px for v in p)


def image(w, h):
    """A fixed test image: channel values from integer arithmetic only"""
    return bytes(v for y in range(h) for x in range(w)
                 for v in ((x * 37 + y * 11) % 256, (x * 5 + y * 53) % 256, (x * y * 7 + 13) % 256, 255))


CASES = [
    # name, w, h, block, bits, scan_every, scan_dim, chroma, noise, seed
    ("identity", 8, 6, 1, 8, 0, 256, 0, 0, 0),
    ("block3", 8, 6, 3, 8, 0, 256, 0, 0, 0),
    ("chroma2", 8, 6, 1, 8, 0, 256, 2, 0, 0),
    ("chroma_neg", 8, 6, 1, 8, 0, 256, -3, 0, 0),
    ("bits3", 8, 6, 1, 3, 0, 256, 0, 0, 0),
    ("scan2", 8, 6, 1, 8, 2, 160, 0, 0, 0),
    ("noise", 8, 6, 1, 8, 0, 256, 0, 12, 42),
    ("noise_seed0", 8, 6, 1, 8, 0, 256, 0, 5, 0),
    ("all", 16, 9, 4, 4, 3, 96, 1, 20, 7),
]


def lines():
    out = []
    for name, w, h, *p in CASES:
        res = apply(image(w, h), w, h, *p)
        out.append(f"{name} {w} {h} {' '.join(map(str, p))} {hashlib.sha256(res).hexdigest()}")
    return out


def main(argv):
    text = "# scripts/call_fx_reference.py: name w h block bits scan_every scan_dim chroma noise seed sha256(result)\n" + "\n".join(lines()) + "\n"
    if argv[1:] == ["--check"]:
        ok = OUT.read_text(encoding="utf-8") == text
        print("call_fx_reference: " + ("file is what this writes" if ok else "file differs"))
        return 0 if ok else 1
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(text, encoding="utf-8")
    print(f"call_fx_reference: wrote {len(CASES)} cases")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
