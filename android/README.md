# ALICE-GameEngine — Android sample app

Minimal Android Studio project that loads the engine's `cdylib`
(`libalice_game_engine.so`) and drives it through the JNI bridge
declared in `src/mobile.rs`:

- `alice_ge_create(width, height)`
- `alice_ge_tick(handle)`
- `alice_ge_touch(handle, id, phase, x, y)`
- `alice_ge_destroy(handle)`

## Build the native library

```bash
cd ..
cargo install cargo-ndk      # one-time setup
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build --release
```

This drops `libalice_game_engine.so` under
`android/app/src/main/jniLibs/arm64-v8a/` where the Java side picks
it up via `System.loadLibrary("alice_game_engine")`.

## Build + install the APK

```bash
cd android
./gradlew assembleDebug
./gradlew installDebug    # connected device or emulator
```

## What the sample does

`MainActivity.onCreate` calls `AliceGameEngine.create(width, height)`,
which loads the native library and returns an opaque `long` handle.
Every Choreographer frame `MainActivity` calls `engine.tick()` and
forwards touch events from `onTouchEvent` to `engine.touch(...)`.
The actual rendering is delegated back to the engine via the
follow-up wgpu-on-Android integration PR; this scaffold proves the
ABI plumbing and lets the Java side stay stable while the Rust side
gains real method bodies.

## Files

```
android/
├── README.md              # this file
├── settings.gradle.kts    # single-project root
├── build.gradle.kts       # AGP version + plugin block
└── app/
    ├── build.gradle.kts   # minSdk, target, jniLibs path
    ├── src/main/
    │   ├── AndroidManifest.xml
    │   └── java/net/alicelaw/alicegame/
    │       ├── AliceGameEngine.java   # JNI wrapper
    │       └── MainActivity.java      # Choreographer + touch
    └── src/main/jniLibs/   # cargo-ndk drops .so files here
```
