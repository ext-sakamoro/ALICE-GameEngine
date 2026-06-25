package net.alicelaw.alicegame;

import android.app.Activity;
import android.os.Bundle;
import android.util.Log;
import android.view.Choreographer;
import android.view.MotionEvent;
import android.view.View;
import android.widget.TextView;

/**
 * Minimal Activity that owns one {@link AliceGameEngine} for the
 * lifetime of the window, drives one tick per Choreographer frame,
 * and forwards touch input. Real rendering is delegated to the
 * native engine in a follow-up PR.
 */
public final class MainActivity extends Activity {
    private static final String TAG = "AliceGameEngine";

    private AliceGameEngine engine;
    private TextView statusView;
    private final Choreographer.FrameCallback frameCallback = new Choreographer.FrameCallback() {
        @Override
        public void doFrame(long frameTimeNanos) {
            if (engine != null) {
                long frame = engine.tick();
                statusView.setText("frame " + frame);
            }
            Choreographer.getInstance().postFrameCallback(this);
        }
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        statusView = new TextView(this);
        statusView.setText("starting…");
        setContentView(statusView);
        statusView.setOnTouchListener(this::handleTouch);

        View root = statusView.getRootView();
        int width = root.getWidth() > 0 ? root.getWidth() : 1080;
        int height = root.getHeight() > 0 ? root.getHeight() : 1920;

        try {
            engine = new AliceGameEngine(width, height);
            Log.i(TAG, "engine created " + width + "x" + height);
        } catch (UnsatisfiedLinkError | IllegalStateException e) {
            Log.e(TAG, "failed to load native engine", e);
            statusView.setText("native engine missing — run cargo-ndk first");
        }

        Choreographer.getInstance().postFrameCallback(frameCallback);
    }

    private boolean handleTouch(View v, MotionEvent ev) {
        if (engine == null) return false;
        int phase;
        switch (ev.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_POINTER_DOWN:
                phase = AliceGameEngine.PHASE_BEGAN;
                break;
            case MotionEvent.ACTION_MOVE:
                phase = AliceGameEngine.PHASE_MOVED;
                break;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_POINTER_UP:
                phase = AliceGameEngine.PHASE_ENDED;
                break;
            default:
                phase = AliceGameEngine.PHASE_CANCELLED;
        }
        engine.touch(ev.getPointerId(0), phase, ev.getX(), ev.getY());
        return true;
    }

    @Override
    protected void onDestroy() {
        Choreographer.getInstance().removeFrameCallback(frameCallback);
        if (engine != null) {
            engine.close();
            engine = null;
        }
        super.onDestroy();
    }
}
