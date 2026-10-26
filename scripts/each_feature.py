#!/usr/bin/env python3
"""Check that the library builds with each feature of Cargo.toml on its own.

`cargo build --features full` builds the features together, so a module gated on one
feature that needs another (a `use crate::ui` under `window` alone) passes CI while a
caller who enables that one feature cannot build. This reads the `[features]` table of
Cargo.toml (no list kept here, so a new feature is checked without editing this file)
and runs `cargo check --lib --no-default-features --features <f>` for each one not in
SKIP. It fails when a check fails, or when nothing was checked.

    python3 scripts/each_feature.py           check every feature
    python3 scripts/each_feature.py --list    print the features it would check
"""
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# features not checked here, with the reason
SKIP = {
    "default": "the empty set, checked by every plain cargo command",
    "full": "the union, checked by the full-features clippy and test steps",
    "python": "builds a Python extension (pyo3) that needs a Python install to link",
    "godot": "the same FFI surface as ffi, which is checked",
}


def features() -> list[str]:
    with open(ROOT / "Cargo.toml", "rb") as f:
        table = tomllib.load(f).get("features", {})
    return [name for name in table if name not in SKIP]


def main(argv: list[str]) -> int:
    names = features()
    if argv[1:] == ["--list"]:
        print("\n".join(names))
        return 0 if names else 1
    if not names:
        print("each_feature: no feature to check (compared nothing)", file=sys.stderr)
        return 1
    failed = []
    for name in names:
        cmd = ["cargo", "check", "--lib", "--no-default-features", "--features", name]
        print(f"each_feature: {' '.join(cmd)}", flush=True)
        if subprocess.run(cmd, cwd=ROOT).returncode != 0:
            failed.append(name)
    print(f"each_feature: {len(names)} feature(s) checked on their own, {len(failed)} failed")
    if failed:
        print(f"each_feature: failed alone: {', '.join(failed)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
