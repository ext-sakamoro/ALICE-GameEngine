#!/usr/bin/env python3
"""Check that every feature-gated test file is run, and counted, by CI and by preflight.

usage: python3 scripts/gated_tests.py [--root DIR]

A test file is gated when it holds `cfg(feature = "<f>")` (the file, or a module in it). Its
tests run only in a command with that feature, and its GPU tests (ignored without an adapter)
only with --include-ignored, so a file that no command names is silently never run. For each
gated file, in each of .github/workflows/ci.yml and scripts/preflight.sh, there must be a
`cargo test` command piped into `scripts/min_passed.py` that
  - enables the file's feature (`--features <f>` or `--features full`),
  - runs the file: no `--test` / `--lib` filter, or `--test <file>` among its filters,
  - names the file to min_passed with a floor equal to the file's `#[test]` count (a floor
    below the count lets a new test go uncounted),
  - and, when the file has an ignored test, passes --include-ignored to cargo and --no-ignored
    to min_passed.
Fails when no gated file or no min_passed command is found (a check that compared nothing).
"""

import re
import sys
from pathlib import Path

GATE = re.compile(r'cfg\((?:all\()?feature\s*=\s*"([A-Za-z0-9_-]+)"')
COMMAND = re.compile(r"cargo test(?P<cargo>[^|\n]*)\|\s*python3 scripts/min_passed\.py(?P<mp>[^\n]*)")
FLOOR = re.compile(r"\b([A-Za-z0-9_]+)=(\d+)\b")


def gated_files(root: Path):
    """name -> (feature, #[test] count, has an ignored test)"""
    out = {}
    for f in sorted((root / "tests").glob("*.rs")):
        text = f.read_text(encoding="utf-8")
        m = GATE.search(text)
        if m:
            out[f.stem] = (m.group(1), text.count("#[test]"), "#[ignore" in text)
    return out


def commands(text: str):
    """(cargo arguments, min_passed arguments) of each `cargo test ... | min_passed` command"""
    # a command continued over lines with a trailing backslash is one command
    text = re.sub(r"\\\n\s*", " ", text)
    return [(m.group("cargo"), m.group("mp")) for m in COMMAND.finditer(text)]


def covers(cargo: str, mp: str, name: str, feature: str, count: int, ignored: bool):
    """None when the command runs and counts the file as required, else why not"""
    floors = dict((n, int(v)) for n, v in FLOOR.findall(mp))
    if name not in floors:
        return "not named to min_passed"
    feats = re.findall(r"--features[ =]([A-Za-z0-9_,-]+)", cargo)
    enabled = {x for f in feats for x in f.split(",")}
    if feature not in enabled and "full" not in enabled:
        return f"feature {feature} not enabled"
    filters = re.findall(r"--(test|lib)\b\s*([A-Za-z0-9_]*)", cargo.split(" -- ")[0])
    if filters and ("test", name) not in filters:
        return "not among the --test filters"
    if floors[name] != count:
        return f"floor {floors[name]}, the file has {count} tests"
    if ignored and ("--include-ignored" not in cargo or "--no-ignored" not in mp):
        return "has ignored tests but the command does not run them (--include-ignored / --no-ignored)"
    return None


def check(root: Path):
    """the problems found, and how many (file, list) pairs were compared"""
    files = gated_files(root)
    sources = {
        "ci.yml": root / ".github" / "workflows" / "ci.yml",
        "preflight.sh": root / "scripts" / "preflight.sh",
    }
    problems = []
    compared = 0
    if not files:
        problems.append("no feature-gated test file found under tests/")
    for label, path in sources.items():
        cmds = commands(path.read_text(encoding="utf-8")) if path.exists() else []
        if not cmds:
            problems.append(f"{label}: no `cargo test ... | python3 scripts/min_passed.py` command found")
            continue
        for name, (feature, count, ignored) in files.items():
            compared += 1
            why = [covers(c, m, name, feature, count, ignored) for c, m in cmds]
            if all(w is not None for w in why):
                # the reason from the command that names the file, if one does
                named = [w for w in why if w != "not named to min_passed"]
                problems.append(
                    f"{label}: tests/{name}.rs (feature {feature}, {count} tests): "
                    + (named[0] if named else "no min_passed command names it")
                )
    return problems, compared


def main() -> int:
    args = sys.argv[1:]
    root = Path(args[args.index("--root") + 1]) if "--root" in args else Path(__file__).resolve().parent.parent
    problems, compared = check(root)
    print(f"gated_tests: {compared} (file, list) pairs compared", file=sys.stderr)
    for p in problems:
        print(f"gated_tests: {p}", file=sys.stderr)
    return 1 if problems or compared == 0 else 0


if __name__ == "__main__":
    sys.exit(main())
