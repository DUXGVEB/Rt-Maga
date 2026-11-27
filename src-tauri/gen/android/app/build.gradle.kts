import java.io.File
import java.util.Locale
import java.util.Properties
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

val releaseSigningPropertiesFile = rootProject.file("release-signing.properties")
val releaseSigningProperties = Properties().apply {
    if (releaseSigningPropertiesFile.isFile) {
        releaseSigningPropertiesFile.inputStream().use(::load)
    }
}

fun releaseSigningProperty(name: String): String =
    releaseSigningProperties.getProperty(name)
        ?: throw GradleException(
            "Missing $name in ${releaseSigningPropertiesFile.name}. " +
                "See ANDROID-SIGNING.md before building a release APK."
        )

android {
    compileSdk = 37
    namespace = "com.rtmaga.reader"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.rtmaga.reader"
        minSdk = 24
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")

        // Keep APK <= 100MB by packaging ARM64 native libraries only.
        ndk {
            abiFilters += listOf("arm64-v8a")
        }

        externalNativeBuild {
            cmake {
                cppFlags += listOf("-std=c++17", "-O3", "-fvisibility=hidden", "-Wall", "-Wextra")
                arguments += listOf(
                    "-DANDROID_STL=c++_static",
                    "-DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON"
                )
                val llamaCppDir = providers.environmentVariable("LLAMA_CPP_DIR").orNull
                    ?: providers.gradleProperty("llamaCppDir").orNull
                if (!llamaCppDir.isNullOrBlank()) {
                    arguments += "-DLLAMA_CPP_DIR=$llamaCppDir"
                }
            }
        }
    }
    androidResources {
        noCompress += listOf("onnx")
        // Exclude GGUF model weights at AAPT2 level so Tier-2 weights are never bundled in APK assets.
        ignoreAssetsPattern =
            "!*.gguf:!*.GGUF:!.svn:!.git:!.ds_store:!*.scc:.*:!CVS:!thumbs.db:!picasa.ini:!*~"
    }
    packaging {
        resources {
            excludes += listOf("**/*.gguf", "**/*.GGUF")
        }
        jniLibs {
            pickFirsts += listOf("**/libc++_shared.so")
        }
    }
    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }
    signingConfigs {
        create("release") {
            storeFile = rootProject.file(releaseSigningProperty("storeFile"))
            storePassword = releaseSigningProperty("storePassword")
            keyAlias = releaseSigningProperty("keyAlias")
            keyPassword = releaseSigningProperty("keyPassword")
            enableV1Signing = true
            enableV2Signing = true
            enableV3Signing = true
            enableV4Signing = true
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
            }
        }
        getByName("release") {
            signingConfig = signingConfigs.getByName("release")
            optimization {
                enable = true
            }
            proguardFiles(
                *fileTree(".") {
                    include("**/*.pro")
                    exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
    // Enforce ARM64-only packaging even on the universal flavor so APK stays <= 100MB.
    productFlavors.matching { it.name == "universal" }.configureEach {
        ndk {
            abiFilters.clear()
            abiFilters += listOf("arm64-v8a")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    implementation("com.microsoft.onnxruntime:onnxruntime-android:1.21.0")
    implementation("com.google.mlkit:translate:17.0.3")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

abstract class VerifyApkBudgetTask : DefaultTask() {
    @get:Input
    abstract val assetsPath: Property<String>

    @get:Input
    abstract val jniLibsPath: Property<String>

    @TaskAction
    fun verify() {
        val maxAiLiteBytes = 35L * 1024L * 1024L
        val assetsRoot = File(assetsPath.get())
        val jniLibsRoot = File(jniLibsPath.get())

        fun ensureNoGguf(root: File) {
            if (!root.exists()) return
            root.walkTopDown().filter { it.isFile }.forEach { file ->
                if (file.extension.equals("gguf", ignoreCase = true)) {
                    throw GradleException(
                        "APK budget violation: GGUF weights must never be bundled in APK (${file.name})"
                    )
                }
            }
        }

        ensureNoGguf(assetsRoot)
        ensureNoGguf(jniLibsRoot)

        val aiLiteDir = File(assetsRoot, "ai-lite")
        if (aiLiteDir.exists()) {
            val totalBytes = aiLiteDir.walkTopDown()
                .filter { it.isFile }
                .sumOf { it.length() }
            if (totalBytes > maxAiLiteBytes) {
                val sizeMb = String.format(Locale.US, "%.2f", totalBytes / (1024.0 * 1024.0))
                throw GradleException(
                    "APK budget violation: src/main/assets/ai-lite is $sizeMb MB (limit: 35.00 MB)"
                )
            }
        }
    }
}

val verifyApkBudget = tasks.register<VerifyApkBudgetTask>("verifyApkBudget") {
    group = "verification"
    description =
        "Fails build if any .gguf weight exists in assets/jniLibs or if assets/ai-lite exceeds 35 MB."
    assetsPath.set(layout.projectDirectory.dir("src/main/assets").asFile.absolutePath)
    jniLibsPath.set(layout.projectDirectory.dir("src/main/jniLibs").asFile.absolutePath)
    outputs.upToDateWhen { false }
}

tasks.matching { it.name == "preBuild" }.configureEach {
    dependsOn(verifyApkBudget)
}

apply(from = file("tauri.build.gradle.kts"))
