package net.alicelaw.alicegame;

/**
 * Java wrapper around the four C-ABI functions declared in
 * {@code src/mobile.rs} ({@code alice_ge_create}, {@code _tick},
 * {@code _touch}, {@code _destroy}). The wrapper owns the opaque
 * native handle as a {@code long}; downstream Java/Kotlin code never
 * touches the raw pointer.
 */
public final class AliceGameEngine implements AutoCloseable {
    static {
        System.loadLibrary("alice_game_engine");
    }

    /** Touch lifecycle constants — must match {@code mobile::TouchPhase}. */
    public static final int PHASE_BEGAN = 0;
    public static final int PHASE_MOVED = 1;
    public static final int PHASE_ENDED = 2;
    public static final int PHASE_CANCELLED = 3;

    private long handle;

    public AliceGameEngine(int width, int height) {
        this.handle = nativeCreate(width, height);
        if (this.handle == 0) {
            throw new IllegalStateException("alice_ge_create returned null");
        }
    }

    /** Advance one frame. Returns the new frame counter. */
    public long tick() {
        if (handle == 0) throw new IllegalStateException("engine already destroyed");
        return nativeTick(handle);
    }

    /** Forward one touch sample to the engine. */
    public int touch(int id, int phase, float x, float y) {
        if (handle == 0) throw new IllegalStateException("engine already destroyed");
        return nativeTouch(handle, id, phase, x, y);
    }

    @Override
    public void close() {
        if (handle != 0) {
            nativeDestroy(handle);
            handle = 0;
        }
    }

    private static native long nativeCreate(int width, int height);
    private static native long nativeTick(long handle);
    private static native int  nativeTouch(long handle, int id, int phase, float x, float y);
    private static native void nativeDestroy(long handle);
}
