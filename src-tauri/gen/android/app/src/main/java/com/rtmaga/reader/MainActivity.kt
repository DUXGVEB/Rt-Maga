package com.rtmaga.reader

import android.content.ComponentCallbacks2
import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.WindowManager
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import org.json.JSONObject
import kotlin.math.roundToInt

class MainActivity : TauriActivity() {
  override val handleBackNavigation: Boolean = false

  private val mainHandler = Handler(Looper.getMainLooper())
  private var webViewRef: WebView? = null
  private var immersiveEnabled: Boolean = true
  private var imeVisible: Boolean = false
  private var currentOrientationLock: String = "auto"

  @Volatile
  var lastCutoutTopDp: Int = 0
  @Volatile
  var lastCutoutBottomDp: Int = 0
  @Volatile
  var lastCutoutLeftDp: Int = 0
  @Volatile
  var lastCutoutRightDp: Int = 0
  @Volatile
  var lastGestureLeftDp: Int = 0
  @Volatile
  var lastGestureRightDp: Int = 0
  @Volatile
  var lastImeBottomDp: Int = 0

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT),
    )
    super.onCreate(savedInstanceState)

    configureCutoutAndWindow()
    attachInsetsListener()
    registerBackHandler()
    applyImmersiveMode()
    try {
      startService(Intent(this, TaskRemovedCleanupService::class.java))
    } catch (_: Exception) {
      // Safe fallback if OS restricts background service startup; startup & onDestroy sweeps still run.
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    webViewRef = webView
    webView.isVerticalScrollBarEnabled = false
    webView.isHorizontalScrollBarEnabled = false
    // Preserve WebView HTTP disk cache across launches: chapter pages already send
    // `Cache-Control: no-store`, so only small persistent covers reside in WebView cache
    // and render instantly (<16ms) instead of round-tripping through img:// on every launch.
    mainHandler.postDelayed({
      applyImmersiveMode()
      pushEnvToWebView(orientationChanged = false)
    }, 150)
  }

  override fun onStop() {
    super.onStop()
    if (isFinishing) {
      StorageCleaner.purgeTransientPageCache(applicationContext)
    }
  }

  override fun onResume() {
    super.onResume()
    applyImmersiveMode()
    mainHandler.postDelayed({
      applyImmersiveMode()
      pushEnvToWebView(orientationChanged = false)
    }, 120)
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus && !imeVisible) {
      applyImmersiveMode()
    }
  }

  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    // Xiaomi HyperOS / MIUI and ColorOS sometimes fail to re-measure the WebView
    // fixed layer or re-show system bars briefly after Landscape <-> Portrait rotation.
    configureCutoutAndWindow()
    applyImmersiveMode()
    val wv = webViewRef
    wv?.requestLayout()
    window.decorView.requestApplyInsets()

    mainHandler.postDelayed({
      applyImmersiveMode()
      wv?.requestLayout()
      window.decorView.requestApplyInsets()
      pushEnvToWebView(orientationChanged = true)
    }, 120)

    mainHandler.postDelayed({
      if (!imeVisible) {
        applyImmersiveMode()
      }
      pushEnvToWebView(orientationChanged = true)
    }, 340)
  }

  override fun onDestroy() {
    mainHandler.removeCallbacksAndMessages(null)
    AiRuntimePlugin.hardUnloadActiveInstance()
    StorageCleaner.purgeTransientPageCache(applicationContext)
    restoreSystemBars()
    super.onDestroy()
  }

  override fun onTrimMemory(level: Int) {
    super.onTrimMemory(level)
    @Suppress("DEPRECATION")
    if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_CRITICAL) {
      AiRuntimePlugin.hardUnloadActiveInstance()
      webViewRef?.clearCache(false)
    }
  }

  override fun onLowMemory() {
    super.onLowMemory()
    AiRuntimePlugin.hardUnloadActiveInstance()
  }

  private fun configureCutoutAndWindow() {
    WindowCompat.setDecorFitsSystemWindows(window, false)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      val lp = window.attributes
      lp.layoutInDisplayCutoutMode =
        WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
      window.attributes = lp
    } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
      val lp = window.attributes
      lp.layoutInDisplayCutoutMode =
        WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
      window.attributes = lp
    }
  }

  fun setImmersiveEnabled(enabled: Boolean) {
    immersiveEnabled = enabled
    runOnUiThread {
      if (enabled) {
        applyImmersiveMode()
      } else {
        restoreSystemBars()
      }
    }
  }

  fun applyImmersiveMode() {
    if (!immersiveEnabled || imeVisible) return
    val controller = WindowCompat.getInsetsController(window, window.decorView)
    controller.systemBarsBehavior =
      WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    controller.hide(WindowInsetsCompat.Type.systemBars())
  }

  private fun restoreSystemBars() {
    val controller = WindowCompat.getInsetsController(window, window.decorView)
    controller.show(WindowInsetsCompat.Type.systemBars())
  }

  fun setOrientationLockMode(mode: String): Boolean {
    val target = when (mode) {
      "portrait" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT
      "landscape" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
      else -> ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
    }
    currentOrientationLock = when (mode) {
      "portrait", "landscape" -> mode
      else -> "auto"
    }
    return try {
      runOnUiThread {
        requestedOrientation = target
        mainHandler.postDelayed({
          applyImmersiveMode()
          webViewRef?.requestLayout()
          window.decorView.requestApplyInsets()
          pushEnvToWebView(orientationChanged = true)
        }, 160)
      }
      true
    } catch (_: Exception) {
      false
    }
  }

  fun getOrientationLockMode(): String = currentOrientationLock

  private fun attachInsetsListener() {
    ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { view, insets ->
      val density = resources.displayMetrics.density.coerceAtLeast(1f)
      val cutout = insets.getInsets(WindowInsetsCompat.Type.displayCutout())
      val statusBars = insets.getInsets(WindowInsetsCompat.Type.statusBars())
      val navBars = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
      val gestures = insets.getInsetsIgnoringVisibility(WindowInsetsCompat.Type.systemGestures())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())

      val wasImeVisible = imeVisible
      imeVisible = ime.bottom > 0

      val safeTopPx = maxOf(cutout.top, statusBars.top)
      val safeBottomPx = maxOf(cutout.bottom, navBars.bottom)
      val safeLeftPx = maxOf(cutout.left, navBars.left)
      val safeRightPx = maxOf(cutout.right, navBars.right)

      lastCutoutTopDp = (safeTopPx / density).roundToInt()
      lastCutoutBottomDp = (safeBottomPx / density).roundToInt()
      lastCutoutLeftDp = (safeLeftPx / density).roundToInt()
      lastCutoutRightDp = (safeRightPx / density).roundToInt()
      lastGestureLeftDp = (gestures.left / density).roundToInt().coerceAtMost(36)
      lastGestureRightDp = (gestures.right / density).roundToInt().coerceAtMost(36)
      lastImeBottomDp = (ime.bottom / density).roundToInt()

      if (wasImeVisible && !imeVisible && immersiveEnabled) {
        mainHandler.postDelayed({ applyImmersiveMode() }, 100)
      }
      if (wasImeVisible != imeVisible) {
        webViewRef?.requestLayout()
      }

      pushEnvToWebView(orientationChanged = false)
      ViewCompat.onApplyWindowInsets(view, insets)
    }
  }

  private fun registerBackHandler() {
    onBackPressedDispatcher.addCallback(
      this,
      object : OnBackPressedCallback(true) {
        override fun handleOnBackPressed() {
          val wv = webViewRef
          if (wv == null) {
            moveTaskToBack(true)
            return
          }
          wv.evaluateJavascript(
            "(function(){try{return window.__rtmagaBackPress ? window.__rtmagaBackPress() : 'exit';}catch(e){return 'exit';}})()"
          ) { result ->
            val cleaned = result?.trim('"') ?: "exit"
            if (cleaned == "exit") {
              runOnUiThread {
                restoreSystemBars()
                moveTaskToBack(true)
              }
            }
          }
        }
      }
    )
  }

  fun detectRealDeviceName(): String {
    val fromGlobal = try {
      Settings.Global.getString(contentResolver, Settings.Global.DEVICE_NAME)
    } catch (_: Exception) {
      null
    }
    if (!fromGlobal.isNullOrBlank()) {
      return fromGlobal.trim()
    }
    val fromBt = try {
      Settings.Secure.getString(contentResolver, "bluetooth_name")
    } catch (_: Exception) {
      null
    }
    if (!fromBt.isNullOrBlank()) {
      return fromBt.trim()
    }
    val manu = (Build.MANUFACTURER ?: "").trim()
    val model = (Build.MODEL ?: "").trim()
    return when {
      model.isBlank() -> "Android Device"
      manu.isBlank() || model.startsWith(manu, ignoreCase = true) -> model
      else -> "${manu.replaceFirstChar { it.uppercaseChar() }} $model"
    }
  }

  fun setKeepScreenOnEnabled(enabled: Boolean) {
    runOnUiThread {
      if (enabled) {
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
      } else {
        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
      }
    }
  }

  fun detectAndroidSkin(): String {
    val manu = (Build.MANUFACTURER ?: "").lowercase()
    val brand = (Build.BRAND ?: "").lowercase()
    val miuiOs = getSystemProperty("ro.mi.os.version.name")
    if (!miuiOs.isNullOrBlank()) return "hyperos"
    val miuiVer = getSystemProperty("ro.miui.ui.version.name")
    if (!miuiVer.isNullOrBlank() || manu.contains("xiaomi") || brand.contains("redmi") || brand.contains("poco")) {
      return "miui"
    }
    if (manu.contains("samsung") || !getSystemProperty("ro.build.version.oneui").isNullOrBlank()) {
      return "oneui"
    }
    if (!getSystemProperty("ro.build.version.oplusrom").isNullOrBlank() || manu.contains("oppo") || brand.contains("realme") || brand.contains("oneplus")) {
      return "coloros"
    }
    if (!getSystemProperty("ro.vivo.os.version").isNullOrBlank() || manu.contains("vivo") || brand.contains("iqoo")) {
      return "originos"
    }
    return "aosp"
  }

  private fun getSystemProperty(key: String): String? {
    return try {
      val cls = Class.forName("android.os.SystemProperties")
      val getMethod = cls.getMethod("get", String::class.java, String::class.java)
      (getMethod.invoke(null, key, "") as? String)?.takeIf { it.isNotBlank() }
    } catch (_: Exception) {
      null
    }
  }

  fun buildDeviceEnvJson(): JSONObject {
    val dm = resources.displayMetrics
    val density = dm.density.coerceAtLeast(1f)
    val orientation = if (resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE) {
      "landscape"
    } else {
      "portrait"
    }

    val insetsObj = JSONObject().apply {
      put("cutoutTop", lastCutoutTopDp)
      put("cutoutBottom", lastCutoutBottomDp)
      put("cutoutLeft", lastCutoutLeftDp)
      put("cutoutRight", lastCutoutRightDp)
      put("gestureLeft", lastGestureLeftDp)
      put("gestureRight", lastGestureRightDp)
      put("imeBottom", lastImeBottomDp)
    }

    return JSONObject().apply {
      put("platform", "android")
      put("sdkInt", Build.VERSION.SDK_INT)
      put("manufacturer", Build.MANUFACTURER ?: "")
      put("model", Build.MODEL ?: "")
      put("skin", detectAndroidSkin())
      put("realDeviceName", detectRealDeviceName())
      put("orientation", orientation)
      put("orientationLock", currentOrientationLock)
      put("viewportWidthDp", (dm.widthPixels / density).roundToInt())
      put("viewportHeightDp", (dm.heightPixels / density).roundToInt())
      put("immersive", immersiveEnabled)
      put("insets", insetsObj)
    }
  }

  private fun pushEnvToWebView(orientationChanged: Boolean) {
    val wv = webViewRef ?: return
    val json = buildDeviceEnvJson().toString()
    val eventName = if (orientationChanged) "rtmaga:orientation-changed" else "rtmaga:env-changed"
    val js = """
      (function(){
        try {
          var env = $json;
          window.__rtmagaDeviceEnv = env;
          var root = document.documentElement;
          root.style.setProperty('--safe-top', env.insets.cutoutTop + 'px');
          root.style.setProperty('--safe-bottom', env.insets.cutoutBottom + 'px');
          root.style.setProperty('--safe-left', env.insets.cutoutLeft + 'px');
          root.style.setProperty('--safe-right', env.insets.cutoutRight + 'px');
          root.style.setProperty('--gesture-left', env.insets.gestureLeft + 'px');
          root.style.setProperty('--gesture-right', env.insets.gestureRight + 'px');
          root.style.setProperty('--ime-bottom', env.insets.imeBottom + 'px');
          window.dispatchEvent(new CustomEvent('$eventName', { detail: env }));
        } catch (e) {}
      })();
    """.trimIndent()
    runOnUiThread {
      wv.evaluateJavascript(js, null)
    }
  }
}
