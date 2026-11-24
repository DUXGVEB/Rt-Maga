package com.rtmaga.reader

import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.IBinder
import java.io.File

/**
 * Shared storage reclamation helper that removes transient chapter page image files while
 * preserving small manga cover thumbnails (`cache/img/covers`) and SQLite user/per-manga data.
 */
object StorageCleaner {
  fun purgeTransientPageCache(context: Context) {
    try {
      val pagesDir = File(context.filesDir, "cache/img/pages")
      if (pagesDir.exists()) {
        pagesDir.deleteRecursively()
        pagesDir.mkdirs()
      }
      // Also sweep any legacy flat shard directories left outside `covers/` and `pages/`.
      val imgRoot = File(context.filesDir, "cache/img")
      if (imgRoot.isDirectory) {
        imgRoot.listFiles()?.forEach { child ->
          if (child.name != "covers" && child.name != "pages") {
            child.deleteRecursively()
          }
        }
      }
      // Clean temporary WebView HTTP cache files under app cacheDir if present.
      val webViewCache = File(context.cacheDir, "WebView")
      if (webViewCache.exists()) {
        webViewCache.deleteRecursively()
      }
    } catch (_: Exception) {
      // Best-effort cleanup; Rust startup sweep guarantees reconciliation on next launch.
    }
  }
}

/**
 * Lightweight background cleanup service whose `onTaskRemoved` fires immediately when the user
 * swipes RtMaga away from Android Recents ("xóa hẳn đa nhiệm"), reclaiming transient chapter
 * page storage before the process terminates.
 */
class TaskRemovedCleanupService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    return START_NOT_STICKY
  }

  override fun onTaskRemoved(rootIntent: Intent?) {
    StorageCleaner.purgeTransientPageCache(applicationContext)
    stopSelf()
    super.onTaskRemoved(rootIntent)
  }
}
