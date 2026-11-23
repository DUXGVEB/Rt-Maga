package com.rtmaga.reader

import android.Manifest
import android.app.Activity
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class OrientationLockArgs {
  var mode: String = "auto"
}

@InvokeArg
class ImmersiveArgs {
  var enabled: Boolean = true
}

@InvokeArg
class LocalNotificationArgs {
  var title: String = "RtMaga"
  var body: String = ""
  var notificationId: Int = 1001
}

@TauriPlugin
class DeviceEnvPlugin(private val activity: Activity) : Plugin(activity) {
  companion object {
    const val CHANNEL_ID = "rtmaga_new_chapters"
    const val REQ_POST_NOTIFICATIONS = 4201
  }

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
}
