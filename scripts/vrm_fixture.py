#!/usr/bin/env python3
"""A tiny VRM 1.0 file written from the glTF 2.0 and VRMC_vrm 1.0 specifications, and the
values a reader must recover from it.

It is the independent side of the VRM reader's tests: the bytes are laid out here by hand
(standard library only), not by the Rust code. One mesh (a triangle) with POSITION, NORMAL,
JOINTS_0 and WEIGHTS_0, indices, two morph targets, one skin of two joints with inverse bind
matrices, three nodes, and a VRMC_vrm extension with the full 1.0 meta (licence fields),
humanoid bones and expressions (presets with several morph target binds, a custom one).

    python3 scripts/vrm_fixture.py            write tests/data/vrm/tiny.vrm and expected.txt
    python3 scripts/vrm_fixture.py --check    fail when the files are not what this writes
"""
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "data" / "vrm"

POSITIONS = [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)]
NORMALS = [(0.0, 0.0, 1.0)] * 3
JOINTS = [(0, 1, 0, 0), (0, 1, 0, 0), (1, 0, 0, 0)]
WEIGHTS = [(0.75, 0.25, 0.0, 0.0), (0.5, 0.5, 0.0, 0.0), (1.0, 0.0, 0.0, 0.0)]
INDICES = [0, 1, 2]
# morph target position deltas: 0 = "mouth open" moves vertex 2 down, 1 = "smile" moves 0 and 1
TARGETS = [
    [(0.0, 0.0, 0.0), (0.0, 0.0, 0.0), (0.0, -0.25, 0.0)],
    [(-0.125, 0.0625, 0.0), (0.125, 0.0625, 0.0), (0.0, 0.0, 0.0)],
]
# morph target 0 also turns vertex 2's normal by (0, 0.5, 0); target 1 has no normal deltas
TARGET0_NORMALS = [(0.0, 0.0, 0.0), (0.0, 0.0, 0.0), (0.0, 0.5, 0.0)]
# column-major 4x4 inverse bind matrices: identity, and a translation by -1 on y
IBM = [
    [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, -1, 0, 1],
]

META = {
    "name": "Tiny",
    "version": "1",
    "authors": ["Author A", "Author B"],
    "copyrightInformation": "test fixture",
    "licenseUrl": "https://vrm.dev/licenses/1.0/",
    # every licence value is the one the specification does NOT default to, so a reader that
    # ignores a key and returns the default reads another value than this file states
    "avatarPermission": "everyone",
    "allowExcessivelyViolentUsage": True,
    "allowExcessivelySexualUsage": True,
    "commercialUsage": "corporation",
    "allowPoliticalOrReligiousUsage": True,
    "allowAntisocialOrHateUsage": True,
    "creditNotation": "unnecessary",
    "allowRedistribution": True,
    "modification": "allowModificationRedistribution",
}


def f32s(rows):
    return b"".join(struct.pack("<" + "f" * len(r), *r) for r in rows)


def pad4(b, fill=b"\x00"):
    return b + fill * (-len(b) % 4)


# a second primitive: POSITION interleaved in one buffer view with byteStride 16 (4 bytes of
# filler after each vertex), WEIGHTS_0 as normalized unsigned bytes, indices as unsigned bytes
P2_POSITIONS = [(2.0, 0.0, 0.0), (3.0, 0.0, 0.0), (2.0, 1.0, 0.0)]
P2_WEIGHTS_U8 = [(255, 0, 0, 0), (191, 64, 0, 0), (128, 127, 0, 0)]
P2_INDICES = [2, 1, 0]


def build():
    blobs = [
        ("POSITION", f32s(POSITIONS), 5126, "VEC3", 3),
        ("NORMAL", f32s(NORMALS), 5126, "VEC3", 3),
        ("JOINTS_0", b"".join(struct.pack("<4B", *j) for j in JOINTS), 5121, "VEC4", 3),
        ("WEIGHTS_0", f32s(WEIGHTS), 5126, "VEC4", 3),
        ("indices", struct.pack("<3H", *INDICES), 5123, "SCALAR", 3),
        ("target0", f32s(TARGETS[0]), 5126, "VEC3", 3),
        ("target1", f32s(TARGETS[1]), 5126, "VEC3", 3),
        ("ibm", f32s(IBM), 5126, "MAT4", 2),
        ("target0n", f32s(TARGET0_NORMALS), 5126, "VEC3", 3),
        ("p2pos", b"".join(struct.pack("<3f", *p) + b"\xAA" * 4 for p in P2_POSITIONS), 5126, "VEC3", 3),
        ("p2w", b"".join(struct.pack("<4B", *w) for w in P2_WEIGHTS_U8), 5121, "VEC4", 3),
        ("p2idx", struct.pack("<3B", *P2_INDICES), 5121, "SCALAR", 3),
    ]
    bin_ = b""
    views, accessors = [], []
    for i, (name, data, ctype, typ, count) in enumerate(blobs):
        bin_ = pad4(bin_)
        view = {"buffer": 0, "byteOffset": len(bin_), "byteLength": len(data)}
        if name == "p2pos":
            view["byteStride"] = 16
        views.append(view)
        acc = {"bufferView": i, "componentType": ctype, "count": count, "type": typ}
        if name == "p2w":
            acc["normalized"] = True
        if name == "POSITION":
            acc["min"], acc["max"] = [0.0, 0.0, 0.0], [1.0, 1.0, 0.0]
        accessors.append(acc)
        bin_ += data
    bin_ = pad4(bin_)
    doc = {
        "asset": {"version": "2.0", "generator": "vrm_fixture.py"},
        "extensionsUsed": ["VRMC_vrm", "VRMC_materials_mtoon"],
        "buffers": [{"byteLength": len(bin_)}],
        "bufferViews": views,
        "accessors": accessors,
        "meshes": [{
            "primitives": [{
                "attributes": {"POSITION": 0, "NORMAL": 1, "JOINTS_0": 2, "WEIGHTS_0": 3},
                "indices": 4,
                "targets": [{"POSITION": 5, "NORMAL": 8}, {"POSITION": 6}],
                "material": 0,
            }, {
                "attributes": {"POSITION": 9, "JOINTS_0": 2, "WEIGHTS_0": 10},
                "indices": 11,
            }],
            "weights": [0.0, 0.0],
        }],
        "skins": [{"joints": [1, 2], "inverseBindMatrices": 7}],
        # one MToon material on the first primitive; the second primitive has none
        "materials": [{
            "name": "skin",
            "pbrMetallicRoughness": {"baseColorFactor": [1.0, 0.5, 0.25, 1.0]},
            "extensions": {"VRMC_materials_mtoon": {
                "specVersion": "1.0",
                "shadeColorFactor": [0.25, 0.125, 0.5],
                "shadingShiftFactor": -0.25,
                "shadingToonyFactor": 0.75,
            }},
        }],
        "nodes": [
            {"name": "body", "mesh": 0, "skin": 0},
            {"name": "hips", "children": [2]},
            {"name": "head", "translation": [0.0, 1.0, 0.0]},
        ],
        "scenes": [{"nodes": [0, 1]}],
        "scene": 0,
        "extensions": {"VRMC_vrm": {
            "specVersion": "1.0",
            "meta": META,
            "humanoid": {"humanBones": {"hips": {"node": 1}, "head": {"node": 2}}},
            # an expression-type look at: 30 degrees left maps to lookLeft at 0.5 of the way to the
            # 60-degree input maximum times the output scale 1.0; up maps 20 of 40 degrees by 0.5
            "lookAt": {
                "type": "expression",
                "offsetFromHeadBone": [0.0, 0.0625, 0.125],
                "rangeMapHorizontalInner": {"inputMaxValue": 60.0, "outputScale": 1.0},
                "rangeMapHorizontalOuter": {"inputMaxValue": 60.0, "outputScale": 1.0},
                "rangeMapVerticalDown": {"inputMaxValue": 40.0, "outputScale": 0.5},
                "rangeMapVerticalUp": {"inputMaxValue": 40.0, "outputScale": 0.5},
            },
            "expressions": {
                "preset": {
                    "aa": {"morphTargetBinds": [{"node": 0, "index": 0, "weight": 1.0}]},
                    "happy": {"morphTargetBinds": [{"node": 0, "index": 1, "weight": 1.0},
                                                   {"node": 0, "index": 0, "weight": 0.5}],
                              "isBinary": False},
                    "blink": {"morphTargetBinds": [], "isBinary": True},
                    "lookLeft": {"morphTargetBinds": [{"node": 0, "index": 1, "weight": 1.0}]},
                    "lookUp": {"morphTargetBinds": [{"node": 0, "index": 0, "weight": 1.0}]},
                },
                "custom": {"smirk": {"morphTargetBinds": [{"node": 0, "index": 1, "weight": 0.25}]}},
            },
        }},
    }
    js = pad4(json.dumps(doc, separators=(",", ":"), sort_keys=True).encode(), b" ")
    body = struct.pack("<II", len(js), 0x4E4F534A) + js + struct.pack("<II", len(bin_), 0x004E4942) + bin_
    glb = struct.pack("<III", 0x46546C67, 2, 12 + len(body)) + body
    return glb


def expected():
    rows = [f"meta.{k} {json.dumps(v)}" for k, v in sorted(META.items())]
    rows += [f"position {i} {' '.join(repr(x) for x in p)}" for i, p in enumerate(POSITIONS)]
    rows += [f"joints {i} {' '.join(map(str, j))}" for i, j in enumerate(JOINTS)]
    rows += [f"weights {i} {' '.join(repr(x) for x in w)}" for i, w in enumerate(WEIGHTS)]
    rows += [f"indices {' '.join(map(str, INDICES))}"]
    rows += [f"target {t} {i} {' '.join(repr(x) for x in d)}" for t, ds in enumerate(TARGETS) for i, d in enumerate(ds)]
    rows += [f"target_normal 0 {i} {' '.join(repr(x) for x in d)}" for i, d in enumerate(TARGET0_NORMALS)]
    rows += [f"skin joints 1 2", f"ibm 1 {' '.join(repr(float(x)) for x in IBM[1])}"]
    rows += [f"p2position {i} {' '.join(repr(x) for x in p)}" for i, p in enumerate(P2_POSITIONS)]
    rows += [f"p2weights {i} {' '.join(repr(x / 255.0) for x in w)}" for i, w in enumerate(P2_WEIGHTS_U8)]
    rows += [f"p2indices {' '.join(map(str, P2_INDICES))}"]
    rows += ["bone head 2", "bone hips 1"]
    rows += ["expression preset aa 0:0:1.0", "expression preset blink binary",
             "expression preset happy 0:1:1.0 0:0:0.5", "expression preset lookLeft 0:1:1.0",
             "expression preset lookUp 0:0:1.0", "expression custom smirk 0:1:0.25"]
    rows += ["material 0 skin base 1.0 0.5 0.25 1.0 shade 0.25 0.125 0.5 shift -0.25 toony 0.75",
             "primitive 0 material 0", "primitive 1 material none"]
    rows += ["lookat type expression", "lookat offset 0.0 0.0625 0.125",
             "lookat horizontalInner 60.0 1.0", "lookat horizontalOuter 60.0 1.0",
             "lookat verticalDown 40.0 0.5", "lookat verticalUp 40.0 0.5"]
    return "\n".join(rows) + "\n"


def main(argv):
    glb, exp = build(), expected()
    if argv[1:] == ["--check"]:
        ok = (OUT / "tiny.vrm").read_bytes() == glb and (OUT / "expected.txt").read_text(encoding="utf-8") == exp
        print("vrm_fixture: " + ("files are what this writes" if ok else "files differ from what this writes"))
        return 0 if ok else 1
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "tiny.vrm").write_bytes(glb)
    (OUT / "expected.txt").write_text(exp, encoding="utf-8")
    print(f"vrm_fixture: wrote {len(glb)} bytes and {exp.count(chr(10))} expected rows")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
