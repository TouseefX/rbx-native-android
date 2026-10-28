// The Android host of the rbx studio.
//
// It extends [GameActivity] (androidx.games:games-activity, resolved from
// Maven by xbuild's Gradle project — see ../manifest.yaml), whose
// `onCreate` loads `librbx_android.so` and calls `initializeNativeCode`
// there before anything else: that Rust function starts the editor's
// render thread (gpui-pre-mobile's host-driven entry point) and opens no
// window of its own.
//
// What this Activity adds on top of the base class:
//
//  * A second `SurfaceHolder.Callback` beside GameActivity's own, which
//    hands every `Surface` (with the display's density) to the host —
//    GameActivity's native surface callbacks are inert no-ops in Rust, and
//    Kotlin reading the holder is simpler than Rust parsing the callback.
//  * Touch and key events, parsed here and passed as plain values; the
//    base class's own `onTouchEventNative`/`onKeyDownNative` are never
//    reached (and inert if they were).
//  * The IME: a 1x1 [EditText] proxy with a real [InputConnection] — the
//    pattern gpui-pre-mobile's example uses (GpuiInputActivity) — because
//    the software keyboard needs composing text, commits and surrounding
//    deletion, which only an InputConnection provides. Rust asks for it by
//    calling back into this Activity's `gpuiShowKeyboard` & co.
//
// The native methods live in the rbx_android crate: `src/bridge.rs`
// (MainActivity_* — this class) and `src/game_activity.rs`
// (GameActivity_* — the base class). Keep names and signatures in sync.

package com.touseefx.rbxnative

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import android.text.Editable
import android.text.InputType
import android.text.Selection
import android.text.TextWatcher
import android.util.Log
import android.view.KeyEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.ViewGroup
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputConnectionWrapper
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import com.google.androidgamesdk.GameActivity

class MainActivity : GameActivity() {

    /** The software keyboard's proxy, non-null once shown the first time. */
    private var input: InputProxy? = null

    /**
     * How much of a desktop the screen shows, emulator-style: the window's
     * logical size is the physical surface divided by the scale passed with
     * it, so dividing the density by this zoom widens the logical viewport
     * from the phone-native ~411px to ~720px — most of the desktop layout
     * fits on screen at once while touch targets stay large enough to hit.
     * Touches divide by the same scale, so input stays aligned. 1f restores
     * phone-native sizing; larger values zoom out further.
     */
    private val desktopZoom = 1.75f

    /** Hands surfaces to the editor's render thread, beside GameActivity's
     *  own callback (whose Rust side is inert). */
    private val surfaceCallback = object : SurfaceHolder.Callback {
        override fun surfaceCreated(holder: SurfaceHolder) {
            forward(holder)
        }

        override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
            // A repeat of the surface the host already holds is a resize,
            // which is how the host treats it.
            forward(holder)
        }

        override fun surfaceDestroyed(holder: SurfaceHolder) {
            // Blocks until the render thread has let the surface go, which
            // is what the framework expects before returning.
            nativeSurfaceDestroyed()
        }

        private fun forward(holder: SurfaceHolder) {
            val surface: Surface? = holder.surface
            if (surface == null || !surface.isValid) return
            // The density the surface's logical pixels scale by, divided by
            // the desktop zoom above: the host opens GPUI's window scaled by
            // it and expects touches in physical pixels, which onTouchEvent
            // forwards unscaled.
            nativeSurfaceCreated(surface, resources.displayMetrics.density / desktopZoom)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // GameActivity's onCreate has loaded the library, run
        // initializeNativeCode (the render thread is starting), and built
        // its InputEnabledSurfaceView; watch that surface's holder.
        mSurfaceView?.holder?.addCallback(surfaceCallback)
    }

    override fun onDestroy() {
        mSurfaceView?.holder?.removeCallback(surfaceCallback)
        super.onDestroy()
    }

    // ── input ────────────────────────────────────────────────────────────

    override fun onTouchEvent(event: android.view.MotionEvent): Boolean {
        return forward(event)
    }

    override fun onGenericMotionEvent(event: android.view.MotionEvent): Boolean {
        return forward(event)
    }

    private fun forward(event: android.view.MotionEvent): Boolean {
        // Not calling super: GameActivity would only relay to its inert
        // native stub. The return value tells the framework whether the
        // editor consumed the touch (it says false when a platform view
        // should handle it instead).
        val count = event.pointerCount
        if (count == 0) return false
        val ids = IntArray(count)
        val xs = FloatArray(count)
        val ys = FloatArray(count)
        for (i in 0 until count) {
            ids[i] = event.getPointerId(i)
            // getX/getY already are physical pixels relative to this view,
            // which is the surface's coordinate space: pass them through
            // unchanged, the host divides by the density to get GPUI's
            // logical pixels. (Scaling here too landed every touch at
            // density× its logical position — off-window for most of the
            // screen.)
            xs[i] = event.getX(i)
            ys[i] = event.getY(i)
        }
        return nativeMotion(event.actionMasked, event.actionIndex, ids, xs, ys)
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        // Forwarded first, then the super chain, so the system's own
        // handling (Back, volume) still runs.
        nativeKey(keyCode, 0, event.metaState)
        return super.onKeyDown(keyCode, event)
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        nativeKey(keyCode, 1, event.metaState)
        return super.onKeyUp(keyCode, event)
    }

    override fun onResume() {
        super.onResume()
        nativeResumed()
    }

    override fun onPause() {
        nativePaused()
        super.onPause()
    }

    // ── the IME, as gpui-pre-mobile expects it ───────────────────────────
    //
    // Called from Rust (jni::show_keyboard_android & co.) with a session id
    // that every nativeIme call echoes back, so updates from a keyboard
    // that was already dismissed are dropped.

    fun gpuiShowKeyboard(keyboardType: Int, session: Long) {
        // rbx-native diagnostics (temporary): proves the platform asked for
        // the keyboard at all — the focus → text_input_state_changed →
        // show_keyboard_android chain.
        Log.i("rbxstudio", "rbx_android: gpuiShowKeyboard type=$keyboardType session=$session")
        runOnUiThread {
            if (input == null) {
                input = InputProxy()
                input!!.setAlpha(0f)
                input!!.setPadding(0, 0, 0, 0)
                addContentView(input, ViewGroup.LayoutParams(1, 1))
            }
            input!!.reset(session)
            var type = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
            when (keyboardType) {
                1 -> type = InputType.TYPE_CLASS_TEXT or
                    InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS
                2 -> type = InputType.TYPE_CLASS_PHONE
                3 -> type = InputType.TYPE_CLASS_NUMBER
                4 -> type = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
                5 -> type = InputType.TYPE_CLASS_NUMBER or
                    InputType.TYPE_NUMBER_FLAG_DECIMAL
            }
            input!!.inputType = type
            input!!.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI
            input!!.requestFocus()
            val imm = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager
            imm.restartInput(input)
            imm.showSoftInput(input, InputMethodManager.SHOW_IMPLICIT)
        }
    }

    fun gpuiHideKeyboard(session: Long) {
        // rbx-native diagnostics (temporary): the losing-focus counterpart.
        Log.i("rbxstudio", "rbx_android: gpuiHideKeyboard session=$session")
        runOnUiThread {
            val input = this.input ?: return@runOnUiThread
            input.reset(session)
            val imm = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager
            imm.hideSoftInputFromWindow(input.windowToken, 0)
            input.clearFocus()
        }
    }

    fun gpuiResetComposition(session: Long) {
        runOnUiThread {
            val input = this.input ?: return@runOnUiThread
            input.reset(session)
            (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager).restartInput(input)
        }
    }

    /**
     * The software keyboard's stand-in: an invisible EditText whose
     * InputConnection batches the IME's edits (composing text, commits,
     * surrounding deletion) and reports each batch to Rust through
     * [nativeIme], with the session it was started under. Hardware
     * keyboards editing the widget directly are caught by the text
     * watcher.
     */
    private inner class InputProxy : EditText(this@MainActivity) {
        private var session: Long = 0
        private var depth: Int = 0
        private var marked: Boolean = false

        init {
            addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(
                    s: CharSequence, start: Int, count: Int, after: Int
                ) {}

                override fun onTextChanged(
                    s: CharSequence, start: Int, before: Int, count: Int
                ) {}

                override fun afterTextChanged(text: Editable) {
                    // Hardware keyboards edit the widget directly, outside
                    // its InputConnection. IME mutations are batched by the
                    // depth below.
                    if (depth == 0) {
                        depth++
                        endEdit()
                    }
                }
            })
        }

        override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
            if (keyCode == KeyEvent.KEYCODE_DEL && text.isNotEmpty() && !marked) {
                // A delete with nothing in the proxy: the editor's own
                // selection is what it must delete, in code points.
                nativeIme(session, 3, "", 1, 0)
                return true
            }
            return super.onKeyDown(keyCode, event)
        }

        fun reset(nextSession: Long) {
            depth++
            text.clear()
            marked = false
            session = nextSession
            depth--
        }

        private fun endEdit() {
            if (--depth != 0) return
            val text = text
            val composing = BaseInputConnection.getComposingSpanStart(text) >= 0
            if (composing || marked || text.isNotEmpty()) {
                nativeIme(
                    session,
                    if (composing) 0 else 1,
                    text.toString(),
                    maxOf(0, Selection.getSelectionStart(text)),
                    maxOf(0, Selection.getSelectionEnd(text))
                )
                marked = composing
                if (!composing) {
                    depth++
                    text.clear()
                    depth--
                }
            }
        }

        override fun onKeyPreIme(keyCode: Int, event: KeyEvent): Boolean {
            if (keyCode == KeyEvent.KEYCODE_BACK &&
                event.action == KeyEvent.ACTION_UP
            ) {
                // Dismissed with Back: the keyboard is gone, the editor
                // should drop its composition.
                nativeIme(session, 4, "", 0, 0)
            }
            return super.onKeyPreIme(keyCode, event)
        }

        override fun onCreateInputConnection(info: EditorInfo): InputConnection? {
            val connection = super.onCreateInputConnection(info) ?: return null
            val connectionSession = session
            return object : InputConnectionWrapper(connection, false) {
                override fun beginBatchEdit(): Boolean {
                    if (connectionSession != session) return false
                    depth++
                    return super.beginBatchEdit()
                }

                override fun endBatchEdit(): Boolean {
                    if (connectionSession != session) return false
                    val result = super.endBatchEdit()
                    if (depth > 0) endEdit()
                    return result
                }

                override fun setComposingText(text: CharSequence, cursor: Int): Boolean {
                    if (connectionSession != session) return false
                    depth++
                    try {
                        return super.setComposingText(text, cursor)
                    } finally {
                        endEdit()
                    }
                }

                override fun setComposingRegion(start: Int, end: Int): Boolean {
                    if (connectionSession != session) return false
                    depth++
                    try {
                        return super.setComposingRegion(start, end)
                    } finally {
                        endEdit()
                    }
                }

                override fun finishComposingText(): Boolean {
                    if (connectionSession != session) return false
                    depth++
                    try {
                        return super.finishComposingText()
                    } finally {
                        endEdit()
                    }
                }

                override fun commitText(text: CharSequence, cursor: Int): Boolean {
                    if (connectionSession != session) return false
                    depth++
                    try {
                        return super.commitText(text, cursor)
                    } finally {
                        endEdit()
                    }
                }

                override fun setSelection(start: Int, end: Int): Boolean {
                    if (connectionSession != session) return false
                    depth++
                    try {
                        return super.setSelection(start, end)
                    } finally {
                        endEdit()
                    }
                }

                override fun deleteSurroundingText(before: Int, after: Int): Boolean {
                    if (connectionSession != session) return false
                    if (text.isEmpty() && !marked) {
                        // Nothing of the proxy's own to delete: the
                        // editor's selection around the cursor is what the
                        // IME meant.
                        nativeIme(session, 2, "", before, after)
                        return true
                    }
                    depth++
                    try {
                        return super.deleteSurroundingText(before, after)
                    } finally {
                        endEdit()
                    }
                }

                override fun deleteSurroundingTextInCodePoints(
                    before: Int,
                    after: Int
                ): Boolean {
                    if (connectionSession != session) return false
                    if (text.isEmpty() && !marked) {
                        nativeIme(session, 3, "", before, after)
                        return true
                    }
                    depth++
                    try {
                        return super.deleteSurroundingTextInCodePoints(before, after)
                    } finally {
                        endEdit()
                    }
                }

                override fun sendKeyEvent(event: KeyEvent): Boolean {
                    if (connectionSession != session) return false
                    if (event.keyCode == KeyEvent.KEYCODE_DEL) {
                        if (event.action == KeyEvent.ACTION_DOWN) deleteSurroundingText(1, 0)
                        return true
                    }
                    if (event.keyCode == KeyEvent.KEYCODE_ENTER) {
                        if (event.action == KeyEvent.ACTION_DOWN) commitText("\n", 1)
                        return true
                    }
                    return super.sendKeyEvent(event)
                }

                override fun performEditorAction(action: Int): Boolean {
                    if (connectionSession != session) return false
                    if (action == EditorInfo.IME_ACTION_DONE) {
                        finishComposingText()
                        nativeIme(session, 4, "", 0, 0)
                        return true
                    }
                    return commitText("\n", 1)
                }
            }
        }
    }

    // ── the native methods (src/bridge.rs) ───────────────────────────────

    private external fun nativeSurfaceCreated(surface: Surface, density: Float)
    private external fun nativeSurfaceChanged(surface: Surface, density: Float)
    private external fun nativeSurfaceDestroyed()
    private external fun nativeMotion(
        action: Int,
        actionIndex: Int,
        ids: IntArray,
        xs: FloatArray,
        ys: FloatArray
    ): Boolean
    private external fun nativeKey(keyCode: Int, action: Int, metaState: Int)
    private external fun nativeIme(
        session: Long,
        kind: Int,
        text: String,
        start: Int,
        end: Int
    )
    private external fun nativeResumed()
    private external fun nativePaused()

    companion object {
        init {
            // GameActivity loads the library itself in onCreate (from the
            // android.app.lib_name meta-data xbuild writes); loading it here
            // first only makes the native methods of THIS class resolve
            // should any be reached before that.
            System.loadLibrary("rbx_android")
        }

        // The Android clipboard, for the platform layer's JNI bridge (the
        // same contract as GpuiPathPicker.openFiles: a static taking the
        // activity, called from Rust). The system clipboard only allows
        // this app to READ it while it is focused, which is when a paste
        // happens anyway.
        @JvmStatic
        fun readClipboardText(activity: Activity): String? {
            val clipboard =
                activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                ?: return null
            val clip = clipboard.primaryClip ?: return null
            if (clip.itemCount == 0) return null
            val text = clip.getItemAt(0)?.coerceToText(activity) ?: return null
            return text.toString()
        }

        @JvmStatic
        fun writeClipboardText(activity: Activity, text: String) {
            val clipboard =
                activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                ?: return
            clipboard.setPrimaryClip(ClipData.newPlainText("rbxstudio", text))
        }
    }
}
