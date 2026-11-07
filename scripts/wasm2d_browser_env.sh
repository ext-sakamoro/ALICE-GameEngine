#!/usr/bin/env bash
# Prints NAME=value lines for running tests/wasm2d.rs with `cargo test` in headless Chrome:
# the wasm-bindgen test runner of the version this crate resolves to, and the chromedriver
# of the Chrome that is installed. Both are fetched into the cache the first time.
# Progress goes to stderr, so stdout can go to $GITHUB_ENV or `export`.
set -euo pipefail
cd "$(dirname "$0")/.."

cache_root="${XDG_CACHE_HOME:-$HOME/.cache}"

bindgen="$(cargo metadata --format-version 1 --features wasm --filter-platform wasm32-unknown-unknown |
  python3 -c "import json, sys; print(*sorted({p['version'] for p in json.load(sys.stdin)['packages'] if p['name'] == 'wasm-bindgen'}))")"
if [[ -z "$bindgen" || "$bindgen" == *" "* ]]; then
  echo "wasm2d: expected one wasm-bindgen version, got '$bindgen'" >&2
  exit 1
fi
tools="$cache_root/wasm-bindgen/$bindgen"
if [[ ! -x "$tools/bin/wasm-bindgen-test-runner" ]]; then
  cargo install wasm-bindgen-cli --version "$bindgen" --locked --root "$tools" >&2
fi

case "$(uname -s)" in
  Darwin)
    chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    platform=mac-arm64
    ;;
  Linux)
    chrome="$(command -v google-chrome)"
    platform=linux64
    ;;
  *)
    echo "wasm2d: no Chrome location for $(uname -s)" >&2
    exit 1
    ;;
esac
version="$("$chrome" --version | awk '{print $NF}')"
drivers="$cache_root/chromedriver/$version"
driver="$drivers/chromedriver-$platform/chromedriver"
if [[ ! -x "$driver" ]]; then
  mkdir -p "$drivers"
  curl -sSfL -o "$drivers/driver.zip" \
    "https://storage.googleapis.com/chrome-for-testing-public/$version/$platform/chromedriver-$platform.zip"
  unzip -qo "$drivers/driver.zip" -d "$drivers"
  chmod +x "$driver"
fi

echo "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=$tools/bin/wasm-bindgen-test-runner"
echo "CHROMEDRIVER=$driver"
