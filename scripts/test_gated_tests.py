#!/usr/bin/env python3
"""Tests of scripts/gated_tests.py on hand-written test files, ci.yml and preflight.sh."""

import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).with_name("gated_tests.py")

GPU = '#[cfg(feature = "gpu")]\nmod gpu {\n    #[test]\n    #[ignore = "needs a working GPU adapter"]\n    fn a() {}\n    #[test]\n    #[ignore = "needs a working GPU adapter"]\n    fn b() {}\n}\n'
PHYS = '#![cfg(feature = "physics")]\n#[test]\nfn a() {}\n'
PLAIN = "#[test]\nfn a() {}\n"
GPU_OK = "cargo test --features gpu --test g -- --include-ignored 2>&1 | python3 scripts/min_passed.py --no-ignored g=2"
PHYS_OK = "cargo test --features physics 2>&1 | python3 scripts/min_passed.py lib=1 p=1"


def run(files, ci, pre):
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        (root / "tests").mkdir()
        (root / ".github" / "workflows").mkdir(parents=True)
        (root / "scripts").mkdir()
        for name, text in files.items():
            (root / "tests" / f"{name}.rs").write_text(text)
        if ci is not None:
            (root / ".github" / "workflows" / "ci.yml").write_text(ci)
        if pre is not None:
            (root / "scripts" / "preflight.sh").write_text(pre)
        r = subprocess.run([sys.executable, str(SCRIPT), "--root", d], capture_output=True, text=True)
        return r.returncode


BOTH = GPU_OK + "\n" + PHYS_OK + "\n"
FILES = {"g": GPU, "p": PHYS, "plain": PLAIN}

CASES = [
    # (description, files, ci.yml, preflight.sh, expected exit code)
    ("every gated file run and counted in both", FILES, BOTH, BOTH, 0),
    ("a gated file missing from ci.yml", FILES, PHYS_OK, BOTH, 1),
    ("a gated file missing from preflight.sh", FILES, BOTH, GPU_OK, 1),
    ("a floor below the test count", FILES, BOTH.replace("g=2", "g=1"), BOTH, 1),
    ("a floor above the test count", FILES, BOTH.replace("g=2", "g=3"), BOTH, 1),
    ("the feature not enabled", FILES, BOTH.replace("--features gpu", "--features physics"), BOTH, 1),
    ("--features full enables it", FILES, BOTH.replace("--features gpu", "--features full"), BOTH, 0),
    ("named but filtered out by --test", FILES, BOTH.replace("--test g", "--test other"), BOTH, 1),
    ("ignored tests without --include-ignored", FILES, BOTH.replace(" -- --include-ignored", ""), BOTH, 1),
    ("ignored tests without --no-ignored", FILES, BOTH.replace("--no-ignored ", ""), BOTH, 1),
    ("a command over lines with backslashes", FILES, BOTH.replace("--test g", "\\\n    --test g"), BOTH, 0),
    ("an ungated file needs no entry", {"plain": PLAIN, "g": GPU, "p": PHYS}, BOTH, BOTH, 0),
    ("no gated file at all", {"plain": PLAIN}, BOTH, BOTH, 1),
    ("no min_passed command in ci.yml", FILES, "cargo test\n", BOTH, 1),
    ("no preflight.sh", FILES, BOTH, None, 1),
]


def main() -> int:
    bad = 0
    for what, files, ci, pre, want in CASES:
        got = run(files, ci, pre)
        if got != want:
            print(f"test_gated_tests: {what}: exit {got}, expected {want}", file=sys.stderr)
            bad += 1
    print(f"test_gated_tests: {len(CASES) - bad}/{len(CASES)} cases", file=sys.stderr)
    return 1 if bad or not CASES else 0


if __name__ == "__main__":
    sys.exit(main())
