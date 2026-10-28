// Ported from the gpui-pre-mobile example's
// example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiPathPicker.java,
// because xbuild only assembles Gradle's Kotlin source set (a .java beside
// MainActivity.kt is not compiled). Behavior is unchanged: GPUI's file
// prompts import SAF documents into cache before releasing URI access, and
// `openFiles` is called by a Rust worker through JNI — never the UI or GPUI
// event thread.

package dev.gpui.mobile

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Looper
import android.provider.OpenableColumns
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.util.UUID
import java.util.concurrent.CountDownLatch

/** GPUI file prompts import SAF documents into cache before releasing URI access. */
class GpuiPathPicker : Activity() {

    private class Request(val multiple: Boolean) {
        val done = CountDownLatch(1)
        var paths: Array<String>? = null
        var error: Exception? = null

        fun complete(paths: Array<String>?, error: Exception?) {
            synchronized(this) {
                if (done.count == 0L) return
                this.paths = paths
                this.error = error
            }
            done.countDown()
        }
    }

    /** The request this activity instance serves, taken from [companion]'s pending one. */
    private var request: Request? = null

    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        synchronized(GpuiPathPicker::class.java) { request = pending }
        // After process death the original GPUI request no longer exists.
        val request = this.request
        if (request == null) {
            finish()
            return
        }
        if (state != null) return
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT)
        intent.addCategory(Intent.CATEGORY_OPENABLE)
        intent.type = "*/*"
        intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, request.multiple)
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, PICK_FILES)
        } catch (error: Exception) {
            request.complete(null, error)
            finish()
        }
    }

    @Suppress("DEPRECATION")
    override fun onActivityResult(code: Int, result: Int, data: Intent?) {
        super.onActivityResult(code, result, data)
        val request = this.request ?: return
        if (code != PICK_FILES) return
        if (result != RESULT_OK || data == null) {
            request.complete(null, null)
            finish()
            return
        }
        val uris = ArrayList<Uri>()
        val clip = data.clipData
        if (clip != null) {
            for (i in 0 until clip.itemCount) {
                val uri = clip.getItemAt(i).uri
                if (uri != null) uris.add(uri)
            }
        } else {
            data.data?.let { uris.add(it) }
        }
        // Keep this Activity (and its temporary URI grants) alive while
        // reading. Files from remote providers can be large, so copying
        // cannot run on UI.
        Thread(
            {
                val imported = ArrayList<File>()
                try {
                    for (uri in uris) imported.add(importFile(uri))
                    request.complete(imported.map { it.absolutePath }.toTypedArray(), null)
                } catch (error: Exception) {
                    for (file in imported) {
                        file.delete()
                        file.parentFile?.delete()
                    }
                    request.complete(null, error)
                } finally {
                    runOnUiThread { finish() }
                }
            },
            "gpui-file-import",
        ).start()
    }

    private fun importFile(uri: Uri): File {
        var name: String? = null
        contentResolver
            .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor -> if (cursor.moveToFirst()) name = cursor.getString(0) }
        // Keep the original extension and Unicode filename for attachment
        // checks, but never allow a provider's display name to escape the
        // import directory.
        if (name.isNullOrEmpty()) name = uri.lastPathSegment
        if (name.isNullOrEmpty() || name == "." || name == "..") name = "attachment"
        name = name!!
            .replace('/', '_')
            .replace('\\', '_')
            .replace('\u0000', '_')
        val directory = File(cacheDir, "gpui-import-" + UUID.randomUUID())
        if (!directory.mkdir()) throw IOException("Could not create import directory")
        val file = File(directory, name)
        try {
            val input = contentResolver.openInputStream(uri)
                ?: throw IOException("Could not read selected document")
            input.use { stream ->
                FileOutputStream(file).use { output ->
                    val buffer = ByteArray(64 * 1024)
                    var count = stream.read(buffer)
                    while (count != -1) {
                        output.write(buffer, 0, count)
                        count = stream.read(buffer)
                    }
                }
            }
        } catch (error: Exception) {
            file.delete()
            directory.delete()
            throw error
        }
        return file
    }

    override fun onDestroy() {
        if (isFinishing) request?.complete(null, null)
        super.onDestroy()
    }

    companion object {
        private const val PICK_FILES = 9010
        private var pending: Request? = null

        /** Called by a Rust worker, never the UI or GPUI event thread. */
        @JvmStatic
        fun openFiles(activity: Activity, multiple: Boolean): Array<String>? {
            check(Looper.myLooper() != Looper.getMainLooper()) {
                "File prompts must run on a worker thread"
            }
            val current = Request(multiple)
            synchronized(this) {
                check(pending == null) { "A file picker is already open" }
                pending = current
            }
            try {
                activity.runOnUiThread {
                    try {
                        activity.startActivity(Intent(activity, GpuiPathPicker::class.java))
                    } catch (error: Exception) {
                        current.complete(null, error)
                    }
                }
                current.done.await()
                current.error?.let { error -> throw error }
                return current.paths
            } finally {
                synchronized(this) {
                    if (pending === current) pending = null
                }
            }
        }
    }
}
