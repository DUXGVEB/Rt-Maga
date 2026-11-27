package com.rtmaga.reader

import android.Manifest
import android.app.Activity
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ClipboardManager
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.MediaScannerConnection
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.provider.OpenableColumns
import androidx.activity.result.ActivityResult
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.util.UUID
import java.util.concurrent.Executors

@InvokeArg
class OrientationLockArgs {
  var mode: String = "auto"
}

@InvokeArg
class ImmersiveArgs {
  var enabled: Boolean = true
}

@InvokeArg
class KeepScreenOnArgs {
  var enabled: Boolean = false
}

@InvokeArg
class LocalNotificationArgs {
  var title: String = "RtMaga"
  var body: String = ""
  var notificationId: Int = 1001
}

@InvokeArg
class ExportBackupSafArgs {
  var tempPath: String = ""
  var fileName: String = "rtmaga_backup.rtmaga"
  var mimeType: String = "application/vnd.rtmaga.backup"
}

@InvokeArg
class SaveImageToGalleryArgs {
  var tempPath: String = ""
  var fileName: String = "rtmaga_page.jpg"
  var mimeType: String = "image/jpeg"
}

@InvokeArg
class OpenExternalUrlArgs {
  var url: String = ""
}

@TauriPlugin
class DeviceEnvPlugin(private val activity: Activity) : Plugin(activity) {
  companion object {
    const val CHANNEL_ID = "rtmaga_new_chapters"
    const val REQ_POST_NOTIFICATIONS = 4201
    const val MAX_BACKUP_BYTES = 32L * 1024L * 1024L + 24L
  }

  private val ioExecutor = Executors.newSingleThreadExecutor()

  @Volatile
  private var pendingExportTempPath: String? = null

  private fun mainActivity(): MainActivity? = activity as? MainActivity

  private fun ensureChannelInternal() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      val mgr = activity.getSystemService(Context.NOTIFICATION_SERVICE) as? NotificationManager
        ?: return
      val existing = mgr.getNotificationChannel(CHANNEL_ID)
      if (existing == null) {
        val channel = NotificationChannel(
          CHANNEL_ID,
          "New Chapters",
          NotificationManager.IMPORTANCE_DEFAULT
        ).apply {
          description = "Notifications when tracked manga release new chapters"
        }
        mgr.createNotificationChannel(channel)
      }
    }
  }

  @Command
  fun getDeviceEnv(invoke: Invoke) {
    try {
      ensureChannelInternal()
      val json = mainActivity()?.buildDeviceEnvJson()
      if (json != null) {
        invoke.resolve(JSObject(json.toString()))
      } else {
        val fallback = JSObject()
        fallback.put("platform", "android")
        fallback.put("sdkInt", Build.VERSION.SDK_INT)
        fallback.put("realDeviceName", Build.MODEL ?: "Android Device")
        invoke.resolve(fallback)
      }
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun setOrientationLock(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(OrientationLockArgs::class.java)
      val ok = mainActivity()?.setOrientationLockMode(args.mode) ?: false
      val res = JSObject()
      res.put("ok", ok)
      res.put("mode", mainActivity()?.getOrientationLockMode() ?: "auto")
      invoke.resolve(res)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun setImmersive(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ImmersiveArgs::class.java)
      mainActivity()?.setImmersiveEnabled(args.enabled)
      val res = JSObject()
      res.put("enabled", args.enabled)
      invoke.resolve(res)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun setKeepScreenOn(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(KeepScreenOnArgs::class.java)
      mainActivity()?.setKeepScreenOnEnabled(args.enabled)
      val res = JSObject()
      res.put("enabled", args.enabled)
      invoke.resolve(res)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun checkNotificationPermission(invoke: Invoke) {
    try {
      ensureChannelInternal()
      val granted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        ContextCompat.checkSelfPermission(
          activity,
          Manifest.permission.POST_NOTIFICATIONS
        ) == PackageManager.PERMISSION_GRANTED
      } else {
        NotificationManagerCompat.from(activity).areNotificationsEnabled()
      }
      val res = JSObject()
      res.put("granted", granted)
      res.put("canRequest", Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU)
      invoke.resolve(res)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun requestNotificationPermission(invoke: Invoke) {
    try {
      ensureChannelInternal()
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        val granted = ContextCompat.checkSelfPermission(
          activity,
          Manifest.permission.POST_NOTIFICATIONS
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) {
          ActivityCompat.requestPermissions(
            activity,
            arrayOf(Manifest.permission.POST_NOTIFICATIONS),
            REQ_POST_NOTIFICATIONS
          )
        }
        val res = JSObject()
        res.put("granted", granted)
        res.put("requested", !granted)
        invoke.resolve(res)
      } else {
        val res = JSObject()
        res.put("granted", NotificationManagerCompat.from(activity).areNotificationsEnabled())
        res.put("requested", false)
        invoke.resolve(res)
      }
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun sendLocalNotification(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(LocalNotificationArgs::class.java)
      ensureChannelInternal()
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
          ContextCompat.checkSelfPermission(
            activity,
            Manifest.permission.POST_NOTIFICATIONS
          ) != PackageManager.PERMISSION_GRANTED
      ) {
        val res = JSObject()
        res.put("sent", false)
        invoke.resolve(res)
        return
      }

      val launchIntent = Intent(activity, MainActivity::class.java).apply {
        flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
      }
      val pendingFlags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
      } else {
        PendingIntent.FLAG_UPDATE_CURRENT
      }
      val pending = PendingIntent.getActivity(activity, 0, launchIntent, pendingFlags)

      val builder = NotificationCompat.Builder(activity, CHANNEL_ID)
        .setSmallIcon(R.mipmap.ic_launcher)
        .setContentTitle(args.title)
        .setContentText(args.body)
        .setStyle(NotificationCompat.BigTextStyle().bigText(args.body))
        .setPriority(NotificationCompat.PRIORITY_DEFAULT)
        .setAutoCancel(true)
        .setContentIntent(pending)

      NotificationManagerCompat.from(activity).notify(args.notificationId, builder.build())
      val res = JSObject()
      res.put("sent", true)
      invoke.resolve(res)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun readClipboardText(invoke: Invoke) {
    activity.runOnUiThread {
      try {
        val clipboard = activity.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
        val clip = clipboard?.primaryClip
        val raw = if (clip != null && clip.itemCount > 0) {
          clip.getItemAt(0)?.coerceToText(activity)?.toString()?.trim()
        } else {
          null
        }
        val res = JSObject()
        res.put("text", raw ?: "")
        invoke.resolve(res)
      } catch (ex: Exception) {
        invoke.reject(ex.message ?: ex.toString())
      }
    }
  }

  private fun isInsideCacheDir(file: File): Boolean {
    val cacheRoot = activity.cacheDir.canonicalFile
    val canonical = file.canonicalFile
    return canonical.path.startsWith(cacheRoot.path + File.separator)
  }

  private fun queryDisplayName(uri: Uri): String? {
    return try {
      activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
        ?.use { cursor ->
          if (cursor.moveToFirst()) {
            val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (idx >= 0) cursor.getString(idx) else null
          } else {
            null
          }
        }
    } catch (_: Exception) {
      null
    }
  }

  @Command
  fun exportBackupSaf(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ExportBackupSafArgs::class.java)
      val srcFile = File(args.tempPath)
      if (!srcFile.exists() || !isInsideCacheDir(srcFile)) {
        invoke.reject("Backup temp file is missing or outside app cache directory")
        return
      }
      pendingExportTempPath = srcFile.canonicalPath
      val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        // Use application/octet-stream so Android SAF preserves the `.rtmaga` extension reliably.
        type = "application/octet-stream"
        putExtra(Intent.EXTRA_TITLE, args.fileName.ifBlank { "rtmaga_backup.rtmaga" })
      }
      startActivityForResult(invoke, intent, "onExportBackupResult")
    } catch (ex: Exception) {
      pendingExportTempPath = null
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @ActivityCallback
  fun onExportBackupResult(invoke: Invoke, result: ActivityResult) {
    val srcPath = pendingExportTempPath
    pendingExportTempPath = null
    val uri = result.data?.data
    if (result.resultCode != Activity.RESULT_OK || uri == null || srcPath.isNullOrBlank()) {
      val res = JSObject()
      res.put("cancelled", true)
      res.put("bytes", 0L)
      invoke.resolve(res)
      return
    }

    ioExecutor.execute {
      try {
        val srcFile = File(srcPath)
        var copied = 0L
        val buffer = ByteArray(64 * 1024)
        activity.contentResolver.openOutputStream(uri)?.use { outStream ->
          FileInputStream(srcFile).use { inStream ->
            while (true) {
              val read = inStream.read(buffer)
              if (read <= 0) break
              outStream.write(buffer, 0, read)
              copied += read
            }
            outStream.flush()
          }
        } ?: throw IllegalStateException("Android SAF returned null output stream")

        val res = JSObject()
        res.put("cancelled", false)
        res.put("uri", uri.toString())
        res.put("bytes", copied)
        invoke.resolve(res)
      } catch (ex: Exception) {
        invoke.reject(ex.message ?: ex.toString())
      }
    }
  }

  @Command
  fun pickBackupSaf(invoke: Invoke) {
    try {
      val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        // Accept */* so Android OS never grays out `.rtmaga` backup files.
        type = "*/*"
      }
      startActivityForResult(invoke, intent, "onPickBackupResult")
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @ActivityCallback
  fun onPickBackupResult(invoke: Invoke, result: ActivityResult) {
    val uri = result.data?.data
    if (result.resultCode != Activity.RESULT_OK || uri == null) {
      val res = JSObject()
      res.put("cancelled", true)
      invoke.resolve(res)
      return
    }

    ioExecutor.execute {
      var outFile: File? = null
      try {
        val displayName = queryDisplayName(uri) ?: "backup.rtmaga"
        val dest = File(activity.cacheDir, "rtmaga_import_${UUID.randomUUID()}.rtmaga")
        outFile = dest
        var total = 0L
        val buffer = ByteArray(64 * 1024)
        activity.contentResolver.openInputStream(uri)?.use { inStream ->
          FileOutputStream(dest).use { outStream ->
            while (true) {
              val read = inStream.read(buffer)
              if (read <= 0) break
              total += read
              if (total > MAX_BACKUP_BYTES) {
                throw IllegalArgumentException("Selected file exceeds maximum .rtmaga backup size (32 MB)")
              }
              outStream.write(buffer, 0, read)
            }
            outStream.flush()
          }
        } ?: throw IllegalStateException("Android SAF returned null input stream")

        val res = JSObject()
        res.put("cancelled", false)
        res.put("path", dest.absolutePath)
        res.put("displayName", displayName)
        invoke.resolve(res)
      } catch (ex: Exception) {
        outFile?.delete()
        invoke.reject(ex.message ?: ex.toString())
      }
    }
  }

  @Command
  fun saveImageToGallery(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SaveImageToGalleryArgs::class.java)
      val srcFile = File(args.tempPath)
      if (!srcFile.exists() || !isInsideCacheDir(srcFile)) {
        invoke.reject("Source image temp file is missing or outside app cache directory")
        return
      }
      val safeName = args.fileName.replace(Regex("[^A-Za-z0-9._-]"), "_").ifBlank { "rtmaga_page.jpg" }
      val mime = args.mimeType.ifBlank { "image/jpeg" }

      ioExecutor.execute {
        try {
          if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val values = ContentValues().apply {
              put(MediaStore.Images.Media.DISPLAY_NAME, safeName)
              put(MediaStore.Images.Media.MIME_TYPE, mime)
              put(MediaStore.Images.Media.RELATIVE_PATH, "${Environment.DIRECTORY_PICTURES}/RtMaga")
              put(MediaStore.Images.Media.IS_PENDING, 1)
            }
            val resolver = activity.contentResolver
            val itemUri = resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)
              ?: throw IllegalStateException("Failed to create MediaStore gallery entry")
            try {
              resolver.openOutputStream(itemUri)?.use { out ->
                FileInputStream(srcFile).use { input ->
                  input.copyTo(out, 64 * 1024)
                  out.flush()
                }
              } ?: throw IllegalStateException("Failed to open MediaStore output stream")
              val finishValues = ContentValues().apply {
                put(MediaStore.Images.Media.IS_PENDING, 0)
              }
              resolver.update(itemUri, finishValues, null, null)
            } catch (writeEx: Exception) {
              resolver.delete(itemUri, null, null)
              throw writeEx
            }
            val res = JSObject()
            res.put("savedPath", "Pictures/RtMaga/$safeName")
            res.put("uri", itemUri.toString())
            invoke.resolve(res)
          } else {
            @Suppress("DEPRECATION")
            val picsDir = File(
              Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_PICTURES),
              "RtMaga"
            )
            if (!picsDir.exists()) {
              picsDir.mkdirs()
            }
            val dest = File(picsDir, safeName)
            FileInputStream(srcFile).use { input ->
              FileOutputStream(dest).use { out ->
                input.copyTo(out, 64 * 1024)
                out.flush()
              }
            }
            MediaScannerConnection.scanFile(activity, arrayOf(dest.absolutePath), arrayOf(mime), null)
            val res = JSObject()
            res.put("savedPath", dest.absolutePath)
            res.put("uri", Uri.fromFile(dest).toString())
            invoke.resolve(res)
          }
        } catch (ex: Exception) {
          invoke.reject(ex.message ?: ex.toString())
        }
      }
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun openExternalUrl(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(OpenExternalUrlArgs::class.java)
      val raw = args.url.trim()
      if (!raw.startsWith("http://", ignoreCase = true) && !raw.startsWith("https://", ignoreCase = true)) {
        invoke.reject("Only http:// and https:// URLs may be opened externally")
        return
      }
      val intent = Intent(Intent.ACTION_VIEW, Uri.parse(raw)).apply {
        addCategory(Intent.CATEGORY_BROWSABLE)
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      }
      activity.runOnUiThread {
        try {
          activity.startActivity(intent)
          val res = JSObject()
          res.put("opened", true)
          invoke.resolve(res)
        } catch (ex: Exception) {
          invoke.reject(ex.message ?: ex.toString())
        }
      }
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }
}
