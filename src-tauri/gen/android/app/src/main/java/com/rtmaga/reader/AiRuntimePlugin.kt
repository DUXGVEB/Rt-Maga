package com.rtmaga.reader

import ai.onnxruntime.OnnxTensor
import ai.onnxruntime.OrtEnvironment
import ai.onnxruntime.OrtSession
import android.app.Activity
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import androidx.annotation.Keep
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.android.gms.tasks.Task
import com.google.mlkit.common.model.DownloadConditions
import com.google.mlkit.common.model.RemoteModelManager
import com.google.mlkit.nl.translate.TranslateLanguage
import com.google.mlkit.nl.translate.TranslateRemoteModel
import com.google.mlkit.nl.translate.Translation
import com.google.mlkit.nl.translate.Translator
import com.google.mlkit.nl.translate.TranslatorOptions
import java.io.File
import java.io.FileOutputStream
import java.lang.ref.WeakReference
import java.nio.FloatBuffer
import java.util.ArrayDeque
import java.util.Locale
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

@Keep
@InvokeArg
class RunTier1Args {
  var imagePath: String = ""
  var targetLang: String = "en"
  var detectOnly: Boolean = false
}

@Keep
@InvokeArg
class RunTier2Args {
  var imagePath: String = ""
  var modelPath: String = ""
  var mmprojPath: String = ""
  var targetLang: String = "en"
  var prompt: String = ""
}

@Keep
@InvokeArg
class MlKitModelArgs {
  var lang: String = ""
}

@Keep
internal object LlamaMtmdBridge {
  @Volatile
  private var libraryLoaded: Boolean = false

  init {
    libraryLoaded = try {
      System.loadLibrary("llama_mtmd_jni")
      true
    } catch (_: Throwable) {
      false
    }
  }

  fun isLibraryLoaded(): Boolean = libraryLoaded

  fun isRuntimeAvailable(): Boolean =
    libraryLoaded && runCatching { nativeIsRuntimeAvailable() }.getOrDefault(false)

  fun isVulkanSupported(): Boolean =
    libraryLoaded && runCatching { nativeIsVulkanSupported() }.getOrDefault(false)

  fun abortActive() {
    if (libraryLoaded) {
      runCatching { nativeAbortActive() }
    }
  }

  fun hardUnload() {
    if (libraryLoaded) {
      runCatching { nativeHardUnload() }
    }
  }

  fun inferPage(
    imagePath: String,
    modelPath: String,
    mmprojPath: String,
    targetLang: String,
    prompt: String
  ): String {
    if (!libraryLoaded) {
      throw IllegalStateException("Native libllama_mtmd_jni.so failed to load on this device")
    }
    return nativeInferPage(imagePath, modelPath, mmprojPath, targetLang, prompt)
  }

  private external fun nativeIsRuntimeAvailable(): Boolean
  private external fun nativeIsVulkanSupported(): Boolean
  private external fun nativeAbortActive()
  private external fun nativeHardUnload()
  private external fun nativeInferPage(
    imagePath: String,
    modelPath: String,
    mmprojPath: String,
    targetLang: String,
    prompt: String
  ): String
}

private data class NormalizedRegion(
  val text: String,
  val sourceText: String,
  val x1: Int,
  val y1: Int,
  val x2: Int,
  val y2: Int
)

private data class Tier1PipelineOutput(
  val regions: List<NormalizedRegion>,
  val missingLanguages: List<String> = emptyList(),
  val error: String? = null
)

@Keep
@TauriPlugin
class AiRuntimePlugin(private val activity: Activity) : Plugin(activity) {
  companion object {
    private const val MAX_INFER_SIDE = 1024
    private const val DET_MAX_SIDE = 960
    private const val REC_HEIGHT = 48
    private const val REC_MAX_WIDTH = 640
    private const val MAX_REGIONS_PER_PAGE = 28

    @Volatile
    private var activeInstance: WeakReference<AiRuntimePlugin>? = null

    @JvmStatic
    fun hardUnloadActiveInstance() {
      activeInstance?.get()?.hardUnloadInternal(triggerGc = true)
    }

    @JvmStatic
    fun cancelActiveInstanceJobs() {
      activeInstance?.get()?.cancelJobsInternal()
    }
  }

  private val aiExecutor = Executors.newSingleThreadExecutor { runnable ->
    Thread(runnable, "rtmaga-ai-worker").apply { isDaemon = true }
  }

  private val runtimeLock = Any()
  private val cancelFlag = AtomicBoolean(false)
  private val jobGeneration = AtomicLong(0L)

  @Volatile
  private var ortEnv: OrtEnvironment? = null
  private var detSession: OrtSession? = null
  private var recSession: OrtSession? = null
  private var recCharDict: List<String>? = null
  private val translators = HashMap<String, Translator>()

  init {
    activeInstance = WeakReference(this)
  }

  private fun isCancelled(myGen: Long): Boolean =
    cancelFlag.get() || jobGeneration.get() != myGen

  private fun resolveAiStorageDir(): File {
    val dir = activity.getExternalFilesDir("ai_models")
      ?: File(activity.filesDir, "ai_models")
    if (!dir.exists()) {
      dir.mkdirs()
    }
    return dir
  }

  private fun resolveAiTmpDir(): File {
    val dir = File(activity.cacheDir, "ai_tmp")
    if (!dir.exists()) {
      dir.mkdirs()
    }
    return dir
  }

  @Command
  fun getAiStorageDir(invoke: Invoke) {
    try {
      val dir = resolveAiStorageDir()
      val res = JSObject()
      res.put("path", dir.absolutePath)
      invoke.resolve(res)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun getRuntimeCapabilities(invoke: Invoke) {
    aiExecutor.execute {
      try {
        val onnxReady = runCatching {
          OrtEnvironment.getEnvironment() != null
        }.getOrDefault(false)

        val mlKitReady = runCatching {
          TranslateLanguage.ENGLISH.isNotEmpty()
        }.getOrDefault(false)

        val llamaJniReady = LlamaMtmdBridge.isRuntimeAvailable()

        val pmVulkan = Build.VERSION.SDK_INT >= Build.VERSION_CODES.N &&
          activity.packageManager.hasSystemFeature(PackageManager.FEATURE_VULKAN_HARDWARE_LEVEL)
        val vulkanSupported = if (LlamaMtmdBridge.isLibraryLoaded()) {
          pmVulkan && LlamaMtmdBridge.isVulkanSupported()
        } else {
          pmVulkan
        }

        val res = JSObject()
        res.put("onnxReady", onnxReady)
        res.put("mlKitReady", mlKitReady)
        res.put("llamaJniReady", llamaJniReady)
        res.put("vulkanSupported", vulkanSupported)
        invoke.resolve(res)
      } catch (ex: Exception) {
        invoke.reject(ex.message ?: ex.toString())
      }
    }
  }

  @Command
  fun runTier1OcrAndTranslate(invoke: Invoke) {
    val args = try {
      invoke.parseArgs(RunTier1Args::class.java)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: "Invalid runTier1OcrAndTranslate arguments")
      return
    }

    val myGen = jobGeneration.incrementAndGet()
    cancelFlag.set(false)

    aiExecutor.execute {
      var bitmap: Bitmap? = null
      try {
        if (isCancelled(myGen)) {
          invoke.resolve(encodeRegionsReply(emptyList()))
          return@execute
        }

        val imageFile = File(args.imagePath)
        if (args.imagePath.isBlank() || !imageFile.isFile) {
          invoke.reject("Input page image does not exist: ${args.imagePath}")
          return@execute
        }

        bitmap = decodeDownscaledBitmap(imageFile, MAX_INFER_SIDE)
          ?: throw IllegalStateException("Failed to decode image for Tier-1 inference")

        if (isCancelled(myGen)) {
          invoke.resolve(encodeRegionsReply(emptyList()))
          return@execute
        }

        val output = runTier1Pipeline(
          bitmap = bitmap,
          targetLang = args.targetLang,
          detectOnly = args.detectOnly,
          myGen = myGen
        )
        invoke.resolve(encodeRegionsReply(output.regions, output.missingLanguages, output.error))
      } catch (ex: Exception) {
        if (isCancelled(myGen)) {
          invoke.resolve(encodeRegionsReply(emptyList()))
        } else {
          invoke.reject(ex.message ?: ex.toString())
        }
      } finally {
        bitmap?.recycle()
      }
    }
  }

  @Command
  fun runTier2Vlm(invoke: Invoke) {
    val args = try {
      invoke.parseArgs(RunTier2Args::class.java)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: "Invalid runTier2Vlm arguments")
      return
    }

    val myGen = jobGeneration.incrementAndGet()
    cancelFlag.set(false)

    aiExecutor.execute {
      var scaledFile: File? = null
      try {
        if (isCancelled(myGen)) {
          val emptyRes = JSObject()
          emptyRes.put("rawJson", "[]")
          invoke.resolve(emptyRes)
          return@execute
        }

        val srcImage = File(args.imagePath)
        val modelFile = File(args.modelPath)
        val mmprojFile = File(args.mmprojPath)
        if (!srcImage.isFile) {
          invoke.reject("Input image not found: ${args.imagePath}")
          return@execute
        }
        if (!modelFile.isFile || !mmprojFile.isFile) {
          invoke.reject("Tier-2 GGUF or mmproj weights are missing on device storage")
          return@execute
        }

        // Downscale to <= 1024px before native mtmd load to preserve < 1.8GB RAM budget.
        val downscaled = decodeDownscaledBitmap(srcImage, MAX_INFER_SIDE)
          ?: throw IllegalStateException("Failed to decode page image for Tier-2 VLM")
        val tmpInput = File(resolveAiTmpDir(), "vlm_${UUID.randomUUID()}.jpg")
        scaledFile = tmpInput
        try {
          FileOutputStream(tmpInput).use { out ->
            downscaled.compress(Bitmap.CompressFormat.JPEG, 90, out)
            out.flush()
          }
        } finally {
          downscaled.recycle()
        }

        if (isCancelled(myGen)) {
          val emptyRes = JSObject()
          emptyRes.put("rawJson", "[]")
          invoke.resolve(emptyRes)
          return@execute
        }

        val rawJson = LlamaMtmdBridge.inferPage(
          imagePath = tmpInput.absolutePath,
          modelPath = modelFile.absolutePath,
          mmprojPath = mmprojFile.absolutePath,
          targetLang = args.targetLang.ifBlank { "en" },
          prompt = args.prompt
        )

        val res = JSObject()
        res.put("rawJson", rawJson)
        invoke.resolve(res)
      } catch (ex: Exception) {
        if (isCancelled(myGen)) {
          val emptyRes = JSObject()
          emptyRes.put("rawJson", "[]")
          invoke.resolve(emptyRes)
        } else {
          invoke.reject(ex.message ?: ex.toString())
        }
      } finally {
        runCatching { scaledFile?.delete() }
      }
    }
  }

  @Command
  fun cancelActiveJobs(invoke: Invoke) {
    try {
      cancelJobsInternal()
      val res = JSObject()
      res.put("cancelled", true)
      invoke.resolve(res)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun hardUnloadAll(invoke: Invoke) {
    try {
      hardUnloadInternal(triggerGc = true)
      val res = JSObject()
      res.put("unloaded", true)
      invoke.resolve(res)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: ex.toString())
    }
  }

  @Command
  fun listMlKitModels(invoke: Invoke) {
    aiExecutor.execute {
      try {
        val manager = RemoteModelManager.getInstance()
        val task = manager.getDownloadedModels(TranslateRemoteModel::class.java)
        val latch = CountDownLatch(1)
        task.addOnCompleteListener { latch.countDown() }
        val completed = latch.await(8_000L, TimeUnit.MILLISECONDS)
        val arr = JSArray()
        if (completed && task.isSuccessful) {
          val models = task.result ?: emptySet()
          for (model in models) {
            arr.put(model.language)
          }
        }
        val res = JSObject()
        res.put("languages", arr)
        invoke.resolve(res)
      } catch (ex: Exception) {
        invoke.reject(ex.message ?: ex.toString())
      }
    }
  }

  @Command
  fun downloadMlKitModel(invoke: Invoke) {
    val args = try {
      invoke.parseArgs(MlKitModelArgs::class.java)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: "Invalid downloadMlKitModel arguments")
      return
    }
    val mlKitLang = mapToMlKitLanguage(args.lang)
    if (mlKitLang == null) {
      invoke.reject("Unsupported ML Kit translation language: ${args.lang}")
      return
    }

    aiExecutor.execute {
      try {
        val manager = RemoteModelManager.getInstance()
        val model = TranslateRemoteModel.Builder(mlKitLang).build()
        val conditions = DownloadConditions.Builder().build()
        val task = manager.download(model, conditions)
        val latch = CountDownLatch(1)
        task.addOnCompleteListener { latch.countDown() }
        val finished = latch.await(120_000L, TimeUnit.MILLISECONDS)
        if (!finished || !task.isSuccessful) {
          val reason = task.exception?.message ?: "Timed out downloading ML Kit model ($mlKitLang)"
          invoke.reject(reason)
          return@execute
        }
        val res = JSObject()
        res.put("downloaded", true)
        res.put("lang", mlKitLang)
        invoke.resolve(res)
      } catch (ex: Exception) {
        invoke.reject(ex.message ?: ex.toString())
      }
    }
  }

  @Command
  fun deleteMlKitModel(invoke: Invoke) {
    val args = try {
      invoke.parseArgs(MlKitModelArgs::class.java)
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: "Invalid deleteMlKitModel arguments")
      return
    }
    val mlKitLang = mapToMlKitLanguage(args.lang)
    if (mlKitLang == null) {
      invoke.reject("Unsupported ML Kit translation language: ${args.lang}")
      return
    }

    aiExecutor.execute {
      try {
        synchronized(runtimeLock) {
          translators.values.forEach { runCatching { it.close() } }
          translators.clear()
        }
        val manager = RemoteModelManager.getInstance()
        val model = TranslateRemoteModel.Builder(mlKitLang).build()
        val task = manager.deleteDownloadedModel(model)
        val latch = CountDownLatch(1)
        task.addOnCompleteListener { latch.countDown() }
        val finished = latch.await(15_000L, TimeUnit.MILLISECONDS)
        if (!finished || !task.isSuccessful) {
          val reason = task.exception?.message ?: "Failed to delete ML Kit model ($mlKitLang)"
          invoke.reject(reason)
          return@execute
        }
        val res = JSObject()
        res.put("deleted", true)
        res.put("lang", mlKitLang)
        invoke.resolve(res)
      } catch (ex: Exception) {
        invoke.reject(ex.message ?: ex.toString())
      }
    }
  }

  private fun cancelJobsInternal() {
    cancelFlag.set(true)
    jobGeneration.incrementAndGet()
    LlamaMtmdBridge.abortActive()
  }

  private fun hardUnloadInternal(triggerGc: Boolean) {
    cancelJobsInternal()
    synchronized(runtimeLock) {
      runCatching { detSession?.close() }
      detSession = null

      runCatching { recSession?.close() }
      recSession = null

      recCharDict = null
      ortEnv = null

      translators.values.forEach { translator ->
        runCatching { translator.close() }
      }
      translators.clear()

      LlamaMtmdBridge.hardUnload()

      runCatching {
        val tmpDir = File(activity.cacheDir, "ai_tmp")
        if (tmpDir.exists()) {
          tmpDir.deleteRecursively()
        }
      }
    }
    if (triggerGc) {
      runCatching { System.gc() }
    }
  }

  private fun encodeRegionsReply(
    regions: List<NormalizedRegion>,
    missingLanguages: List<String> = emptyList(),
    error: String? = null
  ): JSObject {
    val array = JSArray()
    for (region in regions) {
      val bbox = JSArray()
      bbox.put(region.x1)
      bbox.put(region.y1)
      bbox.put(region.x2)
      bbox.put(region.y2)

      val item = JSObject()
      item.put("text", region.text)
      item.put("sourceText", region.sourceText)
      item.put("bbox", bbox)
      array.put(item)
    }
    val missingArr = JSArray()
    for (lang in missingLanguages) {
      missingArr.put(lang)
    }
    val res = JSObject()
    res.put("regions", array)
    res.put("missingLanguages", missingArr)
    if (!error.isNullOrBlank()) {
      res.put("error", error)
    }
    return res
  }

  private fun decodeDownscaledBitmap(file: File, maxSide: Int): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.absolutePath, bounds)
    val origW = bounds.outWidth
    val origH = bounds.outHeight
    if (origW <= 0 || origH <= 0) {
      return null
    }

    var sampleSize = 1
    val longest = max(origW, origH)
    while (longest / (sampleSize * 2) >= maxSide) {
      sampleSize *= 2
    }

    val decodeOpts = BitmapFactory.Options().apply {
      inSampleSize = sampleSize
      inPreferredConfig = Bitmap.Config.ARGB_8888
    }
    val sampled = BitmapFactory.decodeFile(file.absolutePath, decodeOpts) ?: return null
    val curLongest = max(sampled.width, sampled.height)
    if (curLongest <= maxSide) {
      return sampled
    }

    val scale = maxSide.toFloat() / curLongest.toFloat()
    val targetW = max(1, (sampled.width * scale).roundToInt())
    val targetH = max(1, (sampled.height * scale).roundToInt())
    val scaled = Bitmap.createScaledBitmap(sampled, targetW, targetH, true)
    if (scaled !== sampled) {
      sampled.recycle()
    }
    return scaled
  }

  private fun resolveLiteModelFile(assetName: String, storageFallbackName: String = assetName): File? {
    // 1. Prefer bundled APK assets/ai-lite/<assetName>, copied once into cacheDir/ai_tmp for mmap.
    val assetPath = "ai-lite/$assetName"
    val tmpFile = File(resolveAiTmpDir(), assetName)
    if (tmpFile.isFile && tmpFile.length() > 0L) {
      return tmpFile
    }

    val copiedFromAsset = runCatching {
      activity.assets.open(assetPath).use { input ->
        FileOutputStream(tmpFile).use { output ->
          input.copyTo(output, 64 * 1024)
          output.flush()
        }
      }
      tmpFile.isFile && tmpFile.length() > 0L
    }.getOrDefault(false)
    if (copiedFromAsset) {
      return tmpFile
    }
    runCatching { tmpFile.delete() }

    // 2. Fallback to external storage under both `tier1/` (matching manifest.rs) and `ai-lite/`.
    val storageRoot = resolveAiStorageDir()
    val fallbackRoots = listOf(
      File(storageRoot, "tier1"),
      File(storageRoot, "ai-lite"),
      File(File(activity.filesDir, "ai_models"), "tier1"),
      File(File(activity.filesDir, "ai_models"), "ai-lite")
    )
    for (dir in fallbackRoots) {
      val candidate = File(dir, storageFallbackName)
      if (candidate.isFile && candidate.length() > 0L) {
        return candidate
      }
    }
    return null
  }

  private fun buildOrtSessionOptions(): OrtSession.SessionOptions {
    val threads = Runtime.getRuntime().availableProcessors().coerceIn(1, 4)
    return OrtSession.SessionOptions().apply {
      setOptimizationLevel(OrtSession.SessionOptions.OptLevel.ALL_OPT)
      setIntraOpNumThreads(threads)
      setInterOpNumThreads(1)
      setMemoryPatternOptimization(true)
      // Prefer XNNPACK on ARM64 for dynamic-shape OCR; fall back to NNAPI or CPU cleanly.
      val xnnpackOk = runCatching {
        addXnnpack(mapOf("intra_op_num_threads" to threads.toString()))
        true
      }.getOrDefault(false)
      if (!xnnpackOk && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
        runCatching { addNnapi() }
      }
    }
  }

  private fun ensureDetSession(): Pair<OrtEnvironment, OrtSession>? {
    synchronized(runtimeLock) {
      val existingEnv = ortEnv
      val existingDet = detSession
      if (existingEnv != null && existingDet != null) {
        return existingEnv to existingDet
      }
      val detFile = resolveLiteModelFile("det.onnx") ?: return null
      val env = OrtEnvironment.getEnvironment()
      val session = buildOrtSessionOptions().use { opts ->
        env.createSession(detFile.absolutePath, opts)
      }
      ortEnv = env
      detSession = session
      return env to session
    }
  }

  private fun ensureRecSession(): Triple<OrtEnvironment, OrtSession, List<String>>? {
    synchronized(runtimeLock) {
      val existingEnv = ortEnv
      val existingRec = recSession
      val existingDict = recCharDict
      if (existingEnv != null && existingRec != null && existingDict != null) {
        return Triple(existingEnv, existingRec, existingDict)
      }
      val recFile = resolveLiteModelFile("rec.onnx") ?: return null
      val dictFile = resolveLiteModelFile("inference.yml") ?: return null
      val dict = parsePaddleCharDict(dictFile)
      if (dict.isEmpty()) {
        return null
      }
      val env = ortEnv ?: OrtEnvironment.getEnvironment()
      val session = buildOrtSessionOptions().use { opts ->
        env.createSession(recFile.absolutePath, opts)
      }
      ortEnv = env
      recSession = session
      recCharDict = dict
      return Triple(env, session, dict)
    }
  }

  private fun runTier1Pipeline(
    bitmap: Bitmap,
    targetLang: String,
    detectOnly: Boolean,
    myGen: Long
  ): Tier1PipelineOutput {
    val detPair = runCatching { ensureDetSession() }.getOrNull()
    val lineBoxes = if (detPair != null) {
      detectTextBoxesOnnx(detPair.first, detPair.second, bitmap, myGen)
    } else {
      detectFallbackSpeechBubbles(bitmap, myGen)
    }

    if (isCancelled(myGen) || lineBoxes.isEmpty()) {
      return Tier1PipelineOutput(emptyList())
    }

    if (detectOnly) {
      val grouped = mergeNearbyBoxes(lineBoxes).take(MAX_REGIONS_PER_PAGE)
      return Tier1PipelineOutput(
        grouped.map { b ->
          NormalizedRegion(text = "", sourceText = "", x1 = b[0], y1 = b[1], x2 = b[2], y2 = b[3])
        }
      )
    }

    val recBundle = runCatching { ensureRecSession() }.getOrNull()
      ?: return Tier1PipelineOutput(
        regions = mergeNearbyBoxes(lineBoxes).take(MAX_REGIONS_PER_PAGE).map { b ->
          NormalizedRegion(text = "", sourceText = "", x1 = b[0], y1 = b[1], x2 = b[2], y2 = b[3])
        },
        error = "Built-in Lite OCR recognition model (rec.onnx) failed to initialize"
      )

    val (env, recSess, charDict) = recBundle
    val recognizedLines = ArrayList<Pair<IntArray, String>>(lineBoxes.size)
    for (box in lineBoxes) {
      if (isCancelled(myGen)) {
        return Tier1PipelineOutput(emptyList())
      }
      val srcText = recognizeBoxAdaptiveOnnx(env, recSess, charDict, bitmap, box)
      if (srcText.isNotBlank()) {
        recognizedLines.add(box to srcText)
      }
    }

    if (recognizedLines.isEmpty() || isCancelled(myGen)) {
      return Tier1PipelineOutput(emptyList())
    }

    val groupedBubbles = groupRecognizedLinesIntoBubbles(recognizedLines)
    val dstMlKitLang = mapToMlKitLanguage(targetLang)
      ?: return Tier1PipelineOutput(
        regions = groupedBubbles.map { (b, src) ->
          NormalizedRegion(text = "", sourceText = src, x1 = b[0], y1 = b[1], x2 = b[2], y2 = b[3])
        },
        error = "Unsupported target language for ML Kit Translate: $targetLang"
      )

    // Pre-flight check downloaded ML Kit offline language models so we prompt explicitly before downloading.
    val downloadedModels = getDownloadedMlKitLanguages(myGen)
    val missingLangs = LinkedHashSet<String>()
    for ((_, sourceText) in groupedBubbles) {
      val srcMlKitLang = detectScriptLanguage(sourceText)
      if (srcMlKitLang == dstMlKitLang) continue
      if (srcMlKitLang !in downloadedModels) {
        missingLangs.add(srcMlKitLang)
      }
      if (dstMlKitLang !in downloadedModels) {
        missingLangs.add(dstMlKitLang)
      }
    }

    if (missingLangs.isNotEmpty()) {
      val bubbleRegions = groupedBubbles.map { (b, src) ->
        NormalizedRegion(text = "", sourceText = src, x1 = b[0], y1 = b[1], x2 = b[2], y2 = b[3])
      }
      return Tier1PipelineOutput(
        regions = bubbleRegions,
        missingLanguages = missingLangs.toList()
      )
    }

    val results = ArrayList<NormalizedRegion>(groupedBubbles.size)
    var firstError: String? = null
    for ((box, sourceText) in groupedBubbles) {
      if (isCancelled(myGen)) {
        return Tier1PipelineOutput(emptyList())
      }
      val srcMlKitLang = detectScriptLanguage(sourceText)
      val translated = if (srcMlKitLang == dstMlKitLang) {
        sourceText
      } else {
        val out = translateOfflineWithMlKit(sourceText, srcMlKitLang, dstMlKitLang, myGen)
        if (out == null && firstError == null) {
          firstError = "Offline ML Kit translation failed for $srcMlKitLang -> $dstMlKitLang"
        }
        out ?: ""
      }
      results.add(
        NormalizedRegion(
          text = translated,
          sourceText = sourceText,
          x1 = box[0],
          y1 = box[1],
          x2 = box[2],
          y2 = box[3]
        )
      )
    }
    return Tier1PipelineOutput(regions = results, error = firstError)
  }

  private fun detectTextBoxesOnnx(
    env: OrtEnvironment,
    session: OrtSession,
    bitmap: Bitmap,
    myGen: Long
  ): List<IntArray> {
    val srcW = bitmap.width
    val srcH = bitmap.height
    val longest = max(srcW, srcH)
    val scale = if (longest > DET_MAX_SIDE) DET_MAX_SIDE.toFloat() / longest.toFloat() else 1f
    val resizedW = max(32, (ceil((srcW * scale) / 32.0).toInt()) * 32)
    val resizedH = max(32, (ceil((srcH * scale) / 32.0).toInt()) * 32)

    val resized = if (resizedW == srcW && resizedH == srcH) {
      bitmap
    } else {
      Bitmap.createScaledBitmap(bitmap, resizedW, resizedH, true)
    }

    try {
      if (isCancelled(myGen)) return emptyList()

      val pixels = IntArray(resizedW * resizedH)
      resized.getPixels(pixels, 0, resizedW, 0, 0, resizedW, resizedH)

      val planeSize = resizedW * resizedH
      val tensorData = FloatArray(3 * planeSize)
      val meanR = 0.485f
      val meanG = 0.456f
      val meanB = 0.406f
      val stdR = 0.229f
      val stdG = 0.224f
      val stdB = 0.225f

      for (i in 0 until planeSize) {
        val px = pixels[i]
        val r = ((px ushr 16) and 0xFF) / 255.0f
        val g = ((px ushr 8) and 0xFF) / 255.0f
        val b = (px and 0xFF) / 255.0f
        tensorData[i] = (r - meanR) / stdR
        tensorData[planeSize + i] = (g - meanG) / stdG
        tensorData[2 * planeSize + i] = (b - meanB) / stdB
      }

      if (isCancelled(myGen)) return emptyList()

      val inputName = session.inputNames.first()
      val shape = longArrayOf(1L, 3L, resizedH.toLong(), resizedW.toLong())
      val probMap = synchronized(runtimeLock) {
        if (isCancelled(myGen) || detSession !== session) return emptyList()
        OnnxTensor.createTensor(env, FloatBuffer.wrap(tensorData), shape).use { inputTensor ->
          session.run(mapOf(inputName to inputTensor)).use { result ->
            val outTensor = result[0] as OnnxTensor
            val outShape = outTensor.info.shape
            val outH = if (outShape.size >= 2) outShape[outShape.size - 2].toInt() else resizedH
            val outW = if (outShape.size >= 1) outShape[outShape.size - 1].toInt() else resizedW
            val floatBuf = outTensor.floatBuffer
            val values = FloatArray(outH * outW)
            floatBuf.get(values, 0, min(values.size, floatBuf.remaining()))
            Triple(values, outW, outH)
          }
        }
      }

      if (isCancelled(myGen)) return emptyList()

      return extractDbNetBoxes(probMap.first, probMap.second, probMap.third)
    } finally {
      if (resized !== bitmap) {
        resized.recycle()
      }
    }
  }

  private fun extractDbNetBoxes(prob: FloatArray, width: Int, height: Int): List<IntArray> {
    val threshold = 0.30f
    val visited = BooleanArray(width * height)
    val queue = ArrayDeque<Int>(256)
    val rawBoxes = ArrayList<IntArray>()

    val minPixels = max(12, (width * height) / 15000)

    for (y in 0 until height) {
      val rowOffset = y * width
      for (x in 0 until width) {
        val idx = rowOffset + x
        if (visited[idx] || prob[idx] < threshold) continue

        visited[idx] = true
        queue.clear()
        queue.add(idx)

        var minX = x
        var maxX = x
        var minY = y
        var maxY = y
        var count = 0
        var scoreSum = 0f

        while (queue.isNotEmpty()) {
          val cur = queue.removeFirst()
          val cx = cur % width
          val cy = cur / width
          count++
          scoreSum += prob[cur]
          if (cx < minX) minX = cx
          if (cx > maxX) maxX = cx
          if (cy < minY) minY = cy
          if (cy > maxY) maxY = cy

          if (cx > 0) {
            val left = cur - 1
            if (!visited[left] && prob[left] >= threshold) {
              visited[left] = true
              queue.add(left)
            }
          }
          if (cx + 1 < width) {
            val right = cur + 1
            if (!visited[right] && prob[right] >= threshold) {
              visited[right] = true
              queue.add(right)
            }
          }
          if (cy > 0) {
            val up = cur - width
            if (!visited[up] && prob[up] >= threshold) {
              visited[up] = true
              queue.add(up)
            }
          }
          if (cy + 1 < height) {
            val down = cur + width
            if (!visited[down] && prob[down] >= threshold) {
              visited[down] = true
              queue.add(down)
            }
          }
        }

        val meanScore = scoreSum / max(1, count)
        if (count < minPixels || meanScore < 0.45f) continue

        // Apply DBNet unclip expansion (~18%) so characters are not clipped tightly.
        val bw = (maxX - minX + 1).toFloat()
        val bh = (maxY - minY + 1).toFloat()
        val padX = max(2f, bw * 0.14f)
        val padY = max(2f, bh * 0.22f)

        val nx1 = (((minX - padX) / width.toFloat()) * 1000f).roundToInt().coerceIn(0, 999)
        val ny1 = (((minY - padY) / height.toFloat()) * 1000f).roundToInt().coerceIn(0, 999)
        val nx2 = (((maxX + 1 + padX) / width.toFloat()) * 1000f).roundToInt().coerceIn(nx1 + 1, 1000)
        val ny2 = (((maxY + 1 + padY) / height.toFloat()) * 1000f).roundToInt().coerceIn(ny1 + 1, 1000)
        rawBoxes.add(intArrayOf(nx1, ny1, nx2, ny2))
      }
    }

    return rawBoxes
      .sortedWith(compareBy<IntArray>({ it[1] / 120 }, { -it[0] }))
      .take(64)
  }

  private fun boxesBelongToSameBubble(a: IntArray, b: IntArray): Boolean {
    val unionW = max(a[2], b[2]) - min(a[0], b[0])
    val unionH = max(a[3], b[3]) - min(a[1], b[1])
    if (unionW > 460 || unionH > 460) return false
    val gapX = max(0, max(a[0] - b[2], b[0] - a[2]))
    val gapY = max(0, max(a[1] - b[3], b[1] - a[3]))
    val hOverlap = min(a[2], b[2]) - max(a[0], b[0])
    val vOverlap = min(a[3], b[3]) - max(a[1], b[1])
    return (gapX <= 24 && vOverlap > 0) || (gapY <= 22 && hOverlap > 0)
  }

  private fun mergeNearbyBoxes(boxes: List<IntArray>): List<IntArray> {
    if (boxes.size <= 1) return boxes
    val merged = boxes.map { it.copyOf() }.toMutableList()
    var changed = true
    while (changed) {
      changed = false
      var i = 0
      while (i < merged.size) {
        var j = i + 1
        while (j < merged.size) {
          val a = merged[i]
          val b = merged[j]
          if (boxesBelongToSameBubble(a, b)) {
            a[0] = min(a[0], b[0])
            a[1] = min(a[1], b[1])
            a[2] = max(a[2], b[2])
            a[3] = max(a[3], b[3])
            merged.removeAt(j)
            changed = true
          } else {
            j++
          }
        }
        i++
      }
    }
    return merged.sortedWith(compareBy<IntArray>({ it[1] / 120 }, { -it[0] }))
  }

  private fun isCjkOrKanaChar(ch: Char): Boolean {
    val code = ch.code
    return code in 0x3000..0x30FF ||
      code in 0x31F0..0x31FF ||
      code in 0x4E00..0x9FFF ||
      code in 0x3400..0x4DBF ||
      code in 0xFF00..0xFFEF
  }

  private fun groupRecognizedLinesIntoBubbles(
    lines: List<Pair<IntArray, String>>
  ): List<Pair<IntArray, String>> {
    if (lines.isEmpty()) return emptyList()
    val groups = lines.indices.map { mutableListOf(it) }.toMutableList()
    val groupBoxes = lines.map { it.first.copyOf() }.toMutableList()

    var changed = true
    while (changed) {
      changed = false
      var i = 0
      while (i < groups.size) {
        var j = i + 1
        while (j < groups.size) {
          if (boxesBelongToSameBubble(groupBoxes[i], groupBoxes[j])) {
            groups[i].addAll(groups.removeAt(j))
            val b = groupBoxes.removeAt(j)
            val a = groupBoxes[i]
            a[0] = min(a[0], b[0])
            a[1] = min(a[1], b[1])
            a[2] = max(a[2], b[2])
            a[3] = max(a[3], b[3])
            changed = true
          } else {
            j++
          }
        }
        i++
      }
    }

    val out = ArrayList<Pair<IntArray, String>>(groups.size)
    for (gIdx in groups.indices) {
      val members = groups[gIdx]
      val verticalCount = members.count { idx ->
        val b = lines[idx].first
        (b[3] - b[1]) * 10 > (b[2] - b[0]) * 13
      }
      if (verticalCount * 2 > members.size) {
        members.sortWith(compareBy<Int>({ -lines[it].first[0] }, { lines[it].first[1] }))
      } else {
        members.sortWith(compareBy<Int>({ lines[it].first[1] / 18 }, { lines[it].first[0] }))
      }
      val sb = StringBuilder()
      for (idx in members) {
        val seg = lines[idx].second.trim()
        if (seg.isEmpty()) continue
        if (sb.isNotEmpty()) {
          val prev = sb[sb.length - 1]
          val next = seg[0]
          if (!(isCjkOrKanaChar(prev) && isCjkOrKanaChar(next))) {
            sb.append(' ')
          }
        }
        sb.append(seg)
      }
      if (sb.isNotEmpty()) {
        out.add(groupBoxes[gIdx] to sb.toString())
      }
    }
    return out
      .sortedWith(compareBy<Pair<IntArray, String>>({ it.first[1] / 120 }, { -it.first[0] }))
      .take(MAX_REGIONS_PER_PAGE)
  }

  private fun recognizeBoxAdaptiveOnnx(
    env: OrtEnvironment,
    session: OrtSession,
    charDict: List<String>,
    bitmap: Bitmap,
    box: IntArray
  ): String {
    val px1 = ((box[0] / 1000f) * bitmap.width).toInt().coerceIn(0, bitmap.width - 1)
    val py1 = ((box[1] / 1000f) * bitmap.height).toInt().coerceIn(0, bitmap.height - 1)
    val px2 = ((box[2] / 1000f) * bitmap.width).toInt().coerceIn(px1 + 1, bitmap.width)
    val py2 = ((box[3] / 1000f) * bitmap.height).toInt().coerceIn(py1 + 1, bitmap.height)
    val cropW = px2 - px1
    val cropH = py2 - py1
    if (cropW < 4 || cropH < 4) return ""

    val crop = Bitmap.createBitmap(bitmap, px1, py1, cropW, cropH)
    try {
      if (cropH * 10 > cropW * 15 && cropW >= 6) {
        val step = max(10, cropW)
        val verticalText = StringBuilder()
        var cy = 0
        while (cy < cropH) {
          val sliceH = min(cropH - cy, (step * 11) / 10)
          if (sliceH < 6) break
          val sub = Bitmap.createBitmap(crop, 0, cy, cropW, sliceH)
          try {
            val txt = recognizeCropOnnx(env, session, charDict, sub)
            if (txt.isNotBlank()) verticalText.append(txt)
          } finally {
            sub.recycle()
          }
          cy += step
        }
        if (verticalText.isNotEmpty()) {
          return verticalText.toString().trim()
        }
      }
      return recognizeCropOnnx(env, session, charDict, crop)
    } finally {
      crop.recycle()
    }
  }

  private fun recognizeCropOnnx(
    env: OrtEnvironment,
    session: OrtSession,
    charDict: List<String>,
    crop: Bitmap
  ): String {
    val targetW = max(16, min(REC_MAX_WIDTH, ((crop.width.toFloat() / max(1, crop.height).toFloat()) * REC_HEIGHT).roundToInt()))
    val resized = Bitmap.createScaledBitmap(crop, targetW, REC_HEIGHT, true)

    try {
      val pixels = IntArray(targetW * REC_HEIGHT)
      resized.getPixels(pixels, 0, targetW, 0, 0, targetW, REC_HEIGHT)
      val planeSize = targetW * REC_HEIGHT
      val data = FloatArray(3 * planeSize)
      for (i in 0 until planeSize) {
        val px = pixels[i]
        val r = ((px ushr 16) and 0xFF) / 255.0f
        val g = ((px ushr 8) and 0xFF) / 255.0f
        val b = (px and 0xFF) / 255.0f
        // PaddleOCR recognition expects BGR channels normalized into [-1, 1].
        data[i] = (b - 0.5f) / 0.5f
        data[planeSize + i] = (g - 0.5f) / 0.5f
        data[2 * planeSize + i] = (r - 0.5f) / 0.5f
      }

      val inputName = session.inputNames.first()
      val shape = longArrayOf(1L, 3L, REC_HEIGHT.toLong(), targetW.toLong())
      return synchronized(runtimeLock) {
        if (recSession !== session) return@synchronized ""
        OnnxTensor.createTensor(env, FloatBuffer.wrap(data), shape).use { inputTensor ->
          session.run(mapOf(inputName to inputTensor)).use { result ->
            val outTensor = result[0] as OnnxTensor
            val outShape = outTensor.info.shape
            val timeSteps = if (outShape.size >= 3) outShape[1].toInt() else 0
            val vocabSize = if (outShape.size >= 3) outShape[2].toInt() else 0
            if (timeSteps <= 0 || vocabSize <= 0) {
              return@use ""
            }
            val buf = outTensor.floatBuffer
            val logits = FloatArray(timeSteps * vocabSize)
            buf.get(logits, 0, min(logits.size, buf.remaining()))
            decodeCtcGreedy(logits, timeSteps, vocabSize, charDict)
          }
        }
      }
    } finally {
      if (resized !== crop) {
        resized.recycle()
      }
    }
  }

  private fun decodeCtcGreedy(
    logits: FloatArray,
    timeSteps: Int,
    vocabSize: Int,
    charDict: List<String>
  ): String {
    val sb = StringBuilder()
    var prevIdx = 0
    for (t in 0 until timeSteps) {
      val base = t * vocabSize
      var bestIdx = 0
      var bestVal = logits[base]
      for (c in 1 until vocabSize) {
        val v = logits[base + c]
        if (v > bestVal) {
          bestVal = v
          bestIdx = c
        }
      }
      if (bestIdx != 0 && bestIdx != prevIdx) {
        val dictIdx = bestIdx - 1
        if (dictIdx in charDict.indices) {
          sb.append(charDict[dictIdx])
        } else if (dictIdx == charDict.size) {
          sb.append(' ')
        }
      }
      prevIdx = bestIdx
    }
    return sb.toString().trim()
  }

  private fun parsePaddleCharDict(file: File): List<String> {
    val chars = ArrayList<String>(7000)
    var inDictSection = false
    file.bufferedReader(Charsets.UTF_8).useLines { lines ->
      for (rawLine in lines) {
        val leading = rawLine.trimStart(' ')
        if (!inDictSection) {
          if (leading.trimEnd(' ', '\t', '\r') == "character_dict:") {
            inDictSection = true
          }
          continue
        }
        if (leading.startsWith("- ")) {
          var token = leading.substring(2).trimEnd('\r', '\n')
          if ((token.startsWith("'") && token.endsWith("'") && token.length >= 2) ||
              (token.startsWith("\"") && token.endsWith("\"") && token.length >= 2)
          ) {
            token = token.substring(1, token.length - 1).replace("''", "'")
          }
          chars.add(token)
        } else if (leading.trimEnd(' ', '\t', '\r').isNotEmpty() && !leading.startsWith("#")) {
          if (chars.isNotEmpty()) break
        }
      }
    }
    return chars
  }

  private fun detectFallbackSpeechBubbles(bitmap: Bitmap, myGen: Long): List<IntArray> {
    val maxDim = 420
    val longest = max(bitmap.width, bitmap.height)
    val scale = if (longest > maxDim) maxDim.toFloat() / longest.toFloat() else 1f
    val w = max(16, (bitmap.width * scale).roundToInt())
    val h = max(16, (bitmap.height * scale).roundToInt())

    val small = if (w == bitmap.width && h == bitmap.height) {
      bitmap
    } else {
      Bitmap.createScaledBitmap(bitmap, w, h, true)
    }

    try {
      if (isCancelled(myGen)) return emptyList()
      val pixels = IntArray(w * h)
      small.getPixels(pixels, 0, w, 0, 0, w, h)

      val luma = ByteArray(w * h)
      for (i in pixels.indices) {
        val px = pixels[i]
        val r = (px ushr 16) and 0xFF
        val g = (px ushr 8) and 0xFF
        val b = px and 0xFF
        luma[i] = ((r * 77 + g * 150 + b * 29) ushr 8).toByte()
      }

      val bright = BooleanArray(w * h) { (luma[it].toInt() and 0xFF) >= 225 }
      val visited = BooleanArray(w * h)
      val queue = ArrayDeque<Int>(512)
      val totalArea = w * h
      val minArea = max(80, (totalArea * 0.0045).toInt())
      val maxArea = (totalArea * 0.26).toInt()
      val candidates = ArrayList<IntArray>()

      for (y in 1 until h - 1) {
        if (isCancelled(myGen)) return emptyList()
        val rowOffset = y * w
        for (x in 1 until w - 1) {
          val start = rowOffset + x
          if (visited[start] || !bright[start]) continue

          visited[start] = true
          queue.clear()
          queue.add(start)

          var minX = x
          var maxX = x
          var minY = y
          var maxY = y
          var area = 0
          var touchesBorder = false

          while (queue.isNotEmpty()) {
            val cur = queue.removeFirst()
            val cx = cur % w
            val cy = cur / w
            area++
            if (cx == 0 || cy == 0 || cx == w - 1 || cy == h - 1) {
              touchesBorder = true
            }
            if (cx < minX) minX = cx
            if (cx > maxX) maxX = cx
            if (cy < minY) minY = cy
            if (cy > maxY) maxY = cy

            if (cx > 0) {
              val n = cur - 1
              if (!visited[n] && bright[n]) {
                visited[n] = true
                queue.add(n)
              }
            }
            if (cx + 1 < w) {
              val n = cur + 1
              if (!visited[n] && bright[n]) {
                visited[n] = true
                queue.add(n)
              }
            }
            if (cy > 0) {
              val n = cur - w
              if (!visited[n] && bright[n]) {
                visited[n] = true
                queue.add(n)
              }
            }
            if (cy + 1 < h) {
              val n = cur + w
              if (!visited[n] && bright[n]) {
                visited[n] = true
                queue.add(n)
              }
            }
          }

          if (touchesBorder || area < minArea || area > maxArea) continue
          val bw = maxX - minX + 1
          val bh = maxY - minY + 1
          val aspect = bw.toFloat() / max(1, bh).toFloat()
          val fill = area.toFloat() / max(1, bw * bh).toFloat()
          if (aspect < 0.22f || aspect > 4.2f || fill < 0.42f) continue

          // Require interior dark ink strokes so blank gutters/margins are excluded.
          var darkInkPixels = 0
          for (iy in minY..maxY) {
            val base = iy * w
            for (ix in minX..maxX) {
              if ((luma[base + ix].toInt() and 0xFF) < 110) {
                darkInkPixels++
              }
            }
          }
          val inkRatio = darkInkPixels.toFloat() / max(1, bw * bh).toFloat()
          if (inkRatio < 0.015f || inkRatio > 0.45f) continue

          val nx1 = ((minX.toFloat() / w) * 1000f).roundToInt().coerceIn(0, 999)
          val ny1 = ((minY.toFloat() / h) * 1000f).roundToInt().coerceIn(0, 999)
          val nx2 = (((maxX + 1).toFloat() / w) * 1000f).roundToInt().coerceIn(nx1 + 1, 1000)
          val ny2 = (((maxY + 1).toFloat() / h) * 1000f).roundToInt().coerceIn(ny1 + 1, 1000)
          candidates.add(intArrayOf(nx1, ny1, nx2, ny2))
        }
      }

      return candidates
        .sortedWith(compareBy<IntArray>({ it[1] / 120 }, { -it[0] }))
        .take(MAX_REGIONS_PER_PAGE)
    } finally {
      if (small !== bitmap) {
        small.recycle()
      }
    }
  }

  private fun detectScriptLanguage(text: String): String {
    var hiraganaKatakana = 0
    var hangul = 0
    var cjk = 0
    var cyrillic = 0
    var thai = 0
    var vietnamese = 0
    var latin = 0

    for (ch in text) {
      val code = ch.code
      when {
        code in 0x3040..0x30FF || code in 0x31F0..0x31FF -> hiraganaKatakana++
        code in 0xAC00..0xD7AF || code in 0x1100..0x11FF || code in 0x3130..0x318F -> hangul++
        code in 0x4E00..0x9FFF || code in 0x3400..0x4DBF -> cjk++
        code in 0x0400..0x04FF -> cyrillic++
        code in 0x0E00..0x0E7F -> thai++
        code in 0x1EA0..0x1EF9 || ch in "ăâđêôơưĂÂĐÊÔƠƯ" -> vietnamese++
        ch in 'a'..'z' || ch in 'A'..'Z' -> latin++
      }
    }

    return when {
      hiraganaKatakana > 0 -> TranslateLanguage.JAPANESE
      hangul > 0 -> TranslateLanguage.KOREAN
      cjk > 0 -> TranslateLanguage.CHINESE
      cyrillic > 0 -> TranslateLanguage.RUSSIAN
      thai > 0 -> TranslateLanguage.THAI
      vietnamese > 0 -> TranslateLanguage.VIETNAMESE
      latin > 0 -> TranslateLanguage.ENGLISH
      else -> TranslateLanguage.JAPANESE
    }
  }

  private fun mapToMlKitLanguage(raw: String): String? {
    val norm = raw.trim().lowercase(Locale.ROOT).substringBefore('-').substringBefore('_')
    return when (norm) {
      "en" -> TranslateLanguage.ENGLISH
      "vi" -> TranslateLanguage.VIETNAMESE
      "ja", "jp" -> TranslateLanguage.JAPANESE
      "ko", "kr" -> TranslateLanguage.KOREAN
      "zh", "cn" -> TranslateLanguage.CHINESE
      "es" -> TranslateLanguage.SPANISH
      "fr" -> TranslateLanguage.FRENCH
      "de" -> TranslateLanguage.GERMAN
      "it" -> TranslateLanguage.ITALIAN
      "pt" -> TranslateLanguage.PORTUGUESE
      "ru" -> TranslateLanguage.RUSSIAN
      "id" -> TranslateLanguage.INDONESIAN
      "th" -> TranslateLanguage.THAI
      "ar" -> TranslateLanguage.ARABIC
      "tr" -> TranslateLanguage.TURKISH
      "pl" -> TranslateLanguage.POLISH
      "nl" -> TranslateLanguage.DUTCH
      "hi" -> TranslateLanguage.HINDI
      else -> TranslateLanguage.fromLanguageTag(norm)
    }
  }

  private fun awaitTaskSuccess(task: Task<*>, timeoutMs: Long, myGen: Long): Boolean {
    val latch = CountDownLatch(1)
    task.addOnCompleteListener { latch.countDown() }
    val deadlineNs = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(timeoutMs)
    while (System.nanoTime() < deadlineNs) {
      if (isCancelled(myGen)) return false
      if (latch.await(100L, TimeUnit.MILLISECONDS)) {
        return task.isSuccessful
      }
    }
    return false
  }

  private fun <T> awaitTaskResult(task: Task<T>, timeoutMs: Long, myGen: Long): T? {
    return if (awaitTaskSuccess(task, timeoutMs, myGen)) task.result else null
  }

  private fun getDownloadedMlKitLanguages(myGen: Long): Set<String> {
    if (isCancelled(myGen)) return emptySet()
    val task = RemoteModelManager.getInstance().getDownloadedModels(TranslateRemoteModel::class.java)
    val models = awaitTaskResult(task, 5_000L, myGen) ?: return emptySet()
    return models.mapTo(HashSet()) { it.language }
  }

  private fun translateOfflineWithMlKit(
    sourceText: String,
    sourceLang: String,
    targetLang: String,
    myGen: Long
  ): String? {
    if (isCancelled(myGen)) return null
    val key = "$sourceLang->$targetLang"
    val translator = synchronized(runtimeLock) {
      if (isCancelled(myGen)) return null
      translators.getOrPut(key) {
        val opts = TranslatorOptions.Builder()
          .setSourceLanguage(sourceLang)
          .setTargetLanguage(targetLang)
          .build()
        Translation.getClient(opts)
      }
    }

    return runCatching {
      if (isCancelled(myGen)) return null
      val translateTask = synchronized(runtimeLock) {
        if (isCancelled(myGen) || translators[key] !== translator) return null
        translator.translate(sourceText)
      }
      awaitTaskResult(translateTask, 15_000L, myGen)
    }.getOrNull()
  }
}
