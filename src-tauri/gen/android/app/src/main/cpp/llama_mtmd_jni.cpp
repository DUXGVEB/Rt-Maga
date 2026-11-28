#include <jni.h>
#include <android/log.h>
#include <dlfcn.h>

#include <algorithm>
#include <atomic>
#include <cstdint>
#include <cstring>
#include <mutex>
#include <string>
#include <thread>
#include <type_traits>
#include <vector>

#if defined(RTMAGA_HAS_VULKAN_HEADERS) || __has_include(<vulkan/vulkan.h>)
#define VK_NO_PROTOTYPES
#include <vulkan/vulkan.h>
#define RTMAGA_VK_PROBE_ENABLED 1
#else
#define RTMAGA_VK_PROBE_ENABLED 0
#endif

#ifdef RTMAGA_HAS_LLAMA_CPP
#include "ggml.h"
#include "llama.h"
#include "mtmd.h"
#include "mtmd-helper.h"
#endif

namespace {

constexpr const char* kLogTag = "RtMagaLlamaJni";
constexpr int32_t kContextSize = 2048;

std::atomic<bool> g_abort_flag{false};
std::mutex g_runtime_mutex;

void log_info(const char* msg) {
    __android_log_print(ANDROID_LOG_INFO, kLogTag, "%s", msg);
}

void log_warn(const char* msg) {
    __android_log_print(ANDROID_LOG_WARN, kLogTag, "%s", msg);
}

std::string jstring_to_utf8(JNIEnv* env, jstring value) {
    if (value == nullptr) {
        return {};
    }
    const char* chars = env->GetStringUTFChars(value, nullptr);
    if (chars == nullptr) {
        return {};
    }
    std::string out(chars);
    env->ReleaseStringUTFChars(value, chars);
    return out;
}

void throw_runtime_exception(JNIEnv* env, const std::string& message) {
    jclass ex_cls = env->FindClass("java/lang/IllegalStateException");
    if (ex_cls != nullptr) {
        env->ThrowNew(ex_cls, message.c_str());
        env->DeleteLocalRef(ex_cls);
    }
}

bool probe_vulkan_hardware() {
#if RTMAGA_VK_PROBE_ENABLED
    void* vk_lib = dlopen("libvulkan.so", RTLD_NOW | RTLD_LOCAL);
    if (vk_lib == nullptr) {
        return false;
    }

    bool supported = false;
    auto get_inst_proc = reinterpret_cast<PFN_vkGetInstanceProcAddr>(
        dlsym(vk_lib, "vkGetInstanceProcAddr")
    );
    if (get_inst_proc != nullptr) {
        auto enum_version = reinterpret_cast<PFN_vkEnumerateInstanceVersion>(
            get_inst_proc(nullptr, "vkEnumerateInstanceVersion")
        );
        uint32_t api_version = VK_API_VERSION_1_0;
        if (enum_version != nullptr) {
            if (enum_version(&api_version) != VK_SUCCESS) {
                api_version = VK_API_VERSION_1_0;
            }
        }

        auto create_instance = reinterpret_cast<PFN_vkCreateInstance>(
            get_inst_proc(nullptr, "vkCreateInstance")
        );
        if (create_instance != nullptr) {
            VkApplicationInfo app_info{};
            app_info.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO;
            app_info.pApplicationName = "RtMaga";
            app_info.applicationVersion = VK_MAKE_VERSION(3, 3, 0);
            app_info.pEngineName = "RtMaga";
            app_info.engineVersion = VK_MAKE_VERSION(3, 3, 0);
            app_info.apiVersion = api_version >= VK_API_VERSION_1_1 ? VK_API_VERSION_1_1 : VK_API_VERSION_1_0;

            VkInstanceCreateInfo create_info{};
            create_info.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
            create_info.pApplicationInfo = &app_info;

            VkInstance instance = VK_NULL_HANDLE;
            if (create_instance(&create_info, nullptr, &instance) == VK_SUCCESS &&
                instance != VK_NULL_HANDLE) {
                auto enum_devices = reinterpret_cast<PFN_vkEnumeratePhysicalDevices>(
                    get_inst_proc(instance, "vkEnumeratePhysicalDevices")
                );
                auto destroy_instance = reinterpret_cast<PFN_vkDestroyInstance>(
                    get_inst_proc(instance, "vkDestroyInstance")
                );
                if (enum_devices != nullptr) {
                    uint32_t device_count = 0;
                    if (enum_devices(instance, &device_count, nullptr) == VK_SUCCESS &&
                        device_count > 0) {
                        supported = true;
                    }
                }
                if (destroy_instance != nullptr) {
                    destroy_instance(instance, nullptr);
                }
            }
        }
    }

    dlclose(vk_lib);
    return supported;
#else
    void* vk_lib = dlopen("libvulkan.so", RTLD_NOW | RTLD_LOCAL);
    if (vk_lib == nullptr) {
        return false;
    }
    bool has_entry = dlsym(vk_lib, "vkCreateInstance") != nullptr;
    dlclose(vk_lib);
    return has_entry;
#endif
}

#ifdef RTMAGA_HAS_LLAMA_CPP

constexpr int32_t kBatchSize = 512;
constexpr int32_t kMaxGenerateTokens = 768;

int32_t optimal_inference_threads() {
    const unsigned int hw = std::thread::hardware_concurrency();
    if (hw == 0) {
        return 4;
    }
    return static_cast<int32_t>(std::clamp<unsigned int>(hw / 2, 2U, 6U));
}

std::string build_vlm_chat_prompt(
    const std::string& media_marker,
    const std::string& target_lang,
    const std::string& user_prompt
) {
    std::string instruction = user_prompt;
    if (instruction.empty()) {
        instruction =
            "Extract all manga speech bubbles in reading order and translate into " +
            (target_lang.empty() ? std::string("en") : target_lang) +
            ". Reply ONLY with a compact JSON array of objects: "
            "[{\"t\":\"translated_text\",\"b\":[x1,y1,x2,y2]}] where coordinates are integers in 0..1000.";
    }
    std::string formatted;
    formatted.reserve(instruction.size() + 160);
    formatted += "<|im_start|>user\n";
    formatted += media_marker;
    formatted += "\n";
    formatted += instruction;
    formatted += "<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n";
    return formatted;
}

// Compatibility helpers across llama.cpp revisions (flash_attn bool vs enum, memory clear API).
template <typename Params, typename = void>
struct HasFlashAttnType : std::false_type {};

template <typename Params>
struct HasFlashAttnType<Params, std::void_t<decltype(std::declval<Params&>().flash_attn_type)>>
    : std::true_type {};

template <typename Params, typename = void>
struct HasFlashAttnBool : std::false_type {};

template <typename Params>
struct HasFlashAttnBool<Params, std::void_t<decltype(std::declval<Params&>().flash_attn)>>
    : std::true_type {};

template <typename Params>
void configure_flash_attn(Params& params) {
    if constexpr (HasFlashAttnType<Params>::value) {
        params.flash_attn_type = LLAMA_FLASH_ATTN_TYPE_ENABLED;
    } else if constexpr (HasFlashAttnBool<Params>::value) {
        params.flash_attn = true;
    }
}

template <typename Ctx, typename = void>
struct HasMemoryClear : std::false_type {};

template <typename Ctx>
struct HasMemoryClear<
    Ctx,
    std::void_t<decltype(llama_memory_clear(llama_get_memory(std::declval<Ctx*>()), true))>>
    : std::true_type {};

template <typename Ctx>
void reset_kv_cache_stateless(Ctx* ctx) {
    if (ctx == nullptr) {
        return;
    }
    if constexpr (HasMemoryClear<Ctx>::value) {
        llama_memory_clear(llama_get_memory(ctx), true);
    } else {
        llama_kv_self_clear(ctx);
    }
}

bool llama_abort_cb(void* data) {
    auto* flag = static_cast<std::atomic<bool>*>(data);
    return flag != nullptr && flag->load(std::memory_order_acquire);
}

struct DirectLlamaState {
    bool backend_initialized = false;
    llama_model* model = nullptr;
    llama_context* ctx = nullptr;
    mtmd_context* mtmd = nullptr;
    std::string loaded_model_path;
    std::string loaded_mmproj_path;
    bool using_vulkan = false;

    void hard_unload() {
        if (mtmd != nullptr) {
            mtmd_free(mtmd);
            mtmd = nullptr;
        }
        if (ctx != nullptr) {
            llama_free(ctx);
            ctx = nullptr;
        }
        if (model != nullptr) {
            llama_model_free(model);
            model = nullptr;
        }
        loaded_model_path.clear();
        loaded_mmproj_path.clear();
        using_vulkan = false;
        if (backend_initialized) {
            llama_backend_free();
            backend_initialized = false;
        }
    }

    bool ensure_loaded(
        const std::string& model_path,
        const std::string& mmproj_path,
        std::string& error_out
    ) {
        if (model != nullptr && ctx != nullptr && mtmd != nullptr &&
            loaded_model_path == model_path && loaded_mmproj_path == mmproj_path) {
            return true;
        }

        hard_unload();

        llama_backend_init();
        backend_initialized = true;

        const bool vulkan_available = probe_vulkan_hardware();
        const int32_t threads = optimal_inference_threads();

        auto load_with_gpu_layers = [&](int32_t ngl) -> bool {
            llama_model_params mparams = llama_model_default_params();
            mparams.n_gpu_layers = ngl;
            mparams.load_mode = LLAMA_LOAD_MODE_MMAP;

            model = llama_model_load_from_file(model_path.c_str(), mparams);
            if (model == nullptr) {
                return false;
            }

            llama_context_params cparams = llama_context_default_params();
            cparams.n_ctx = kContextSize;
            cparams.n_batch = kBatchSize;
            cparams.n_threads = threads;
            cparams.n_threads_batch = threads;
            cparams.type_k = GGML_TYPE_Q8_0;
            cparams.type_v = GGML_TYPE_Q8_0;
            configure_flash_attn(cparams);

            ctx = llama_init_from_model(model, cparams);
            if (ctx == nullptr) {
                llama_model_free(model);
                model = nullptr;
                return false;
            }

            llama_set_abort_callback(ctx, &llama_abort_cb, &g_abort_flag);

            mtmd_context_params mtmd_params = mtmd_context_params_default();
            mtmd_params.use_gpu = (ngl > 0);
            mtmd_params.print_timings = false;
            mtmd_params.n_threads = threads;

            mtmd = mtmd_init_from_file(mmproj_path.c_str(), model, mtmd_params);
            if (mtmd == nullptr) {
                llama_free(ctx);
                ctx = nullptr;
                llama_model_free(model);
                model = nullptr;
                return false;
            }

            using_vulkan = (ngl > 0);
            loaded_model_path = model_path;
            loaded_mmproj_path = mmproj_path;
            return true;
        };

        if (vulkan_available && load_with_gpu_layers(99)) {
            log_info("Loaded Tier-2 Qwen3-VL via Vulkan GPU offload (n_ctx=2048, FA=on, KV=q8_0, mmap=on)");
            return true;
        }
        if (load_with_gpu_layers(0)) {
            log_info("Loaded Tier-2 Qwen3-VL via CPU fallback (n_ctx=2048, FA=on, KV=q8_0, mmap=on)");
            return true;
        }

        hard_unload();
        error_out = "Failed to initialize llama.cpp / mtmd context for Tier-2 VLM weights";
        return false;
    }

    bool infer_page(
        const std::string& image_path,
        const std::string& model_path,
        const std::string& mmproj_path,
        const std::string& target_lang,
        const std::string& prompt,
        std::string& raw_json_out,
        std::string& error_out
    ) {
        if (!ensure_loaded(model_path, mmproj_path, error_out)) {
            return false;
        }

        if (g_abort_flag.load(std::memory_order_acquire)) {
            error_out = "Inference cancelled";
            return false;
        }

        // Enforce strictly stateless KV-cache reset before every page.
        reset_kv_cache_stateless(ctx);

        mtmd_helper_bitmap_wrapper bmp_wrap = mtmd_helper_bitmap_init_from_file(
            mtmd,
            image_path.c_str(),
            false,
            mtmd_helper_init_opt_default()
        );
        mtmd_bitmap* bitmap = bmp_wrap.bitmap;
        if (bitmap == nullptr) {
            error_out = "Failed to decode page image into mtmd bitmap";
            return false;
        }

        const char* marker_cstr = mtmd_default_marker();
        const std::string marker = marker_cstr != nullptr ? std::string(marker_cstr) : std::string("<__media__>");
        const std::string full_prompt = build_vlm_chat_prompt(marker, target_lang, prompt);

        mtmd_input_chunks* chunks = mtmd_input_chunks_init();
        mtmd_input_text input_text{};
        input_text.text = full_prompt.c_str();
        input_text.add_special = true;
        input_text.parse_special = true;

        const mtmd_bitmap* bitmaps[1] = {bitmap};
        const int32_t tok_rc = mtmd_tokenize(mtmd, chunks, &input_text, bitmaps, 1);
        mtmd_bitmap_free(bitmap);
        bitmap = nullptr;

        if (tok_rc != 0) {
            mtmd_input_chunks_free(chunks);
            reset_kv_cache_stateless(ctx);
            error_out = "mtmd_tokenize failed with code " + std::to_string(tok_rc);
            return false;
        }

        llama_pos n_past = 0;
        const int32_t eval_rc = mtmd_helper_eval_chunks(
            mtmd,
            ctx,
            chunks,
            n_past,
            0,
            kBatchSize,
            true,
            &n_past
        );
        mtmd_input_chunks_free(chunks);
        chunks = nullptr;

        if (eval_rc != 0 || g_abort_flag.load(std::memory_order_acquire)) {
            reset_kv_cache_stateless(ctx);
            error_out = g_abort_flag.load(std::memory_order_acquire)
                ? "Inference cancelled during prefill"
                : ("mtmd_helper_eval_chunks failed with code " + std::to_string(eval_rc));
            return false;
        }

        const llama_vocab* vocab = llama_model_get_vocab(model);
        llama_sampler_chain_params sparams = llama_sampler_chain_default_params();
        llama_sampler* sampler = llama_sampler_chain_init(sparams);
        llama_sampler_chain_add(sampler, llama_sampler_init_greedy());
        llama_batch_ext* decode_batch = llama_batch_ext_init(ctx);

        std::string output;
        output.reserve(1024);
        std::vector<char> piece_buf(256);

        for (int32_t step = 0; step < kMaxGenerateTokens; ++step) {
            if (g_abort_flag.load(std::memory_order_acquire)) {
                break;
            }
            const llama_token token = llama_sampler_sample(sampler, ctx, -1);
            llama_sampler_accept(sampler, token);
            if (llama_vocab_is_eog(vocab, token)) {
                break;
            }

            int32_t n_chars = llama_token_to_piece(
                vocab,
                token,
                piece_buf.data(),
                static_cast<int32_t>(piece_buf.size()),
                0,
                false
            );
            if (n_chars < 0) {
                piece_buf.resize(static_cast<size_t>(-n_chars) + 16);
                n_chars = llama_token_to_piece(
                    vocab,
                    token,
                    piece_buf.data(),
                    static_cast<int32_t>(piece_buf.size()),
                    0,
                    false
                );
            }
            if (n_chars > 0) {
                output.append(piece_buf.data(), static_cast<size_t>(n_chars));
            }

            if (n_past + 1 >= kContextSize || decode_batch == nullptr) {
                break;
            }

            llama_batch_ext_clear(decode_batch);
            const int32_t b_idx = llama_batch_ext_add_token(decode_batch, 0, token);
            if (b_idx < 0) {
                break;
            }
            llama_batch_ext_set_pos(decode_batch, b_idx, &n_past);
            llama_batch_ext_set_output_logits(decode_batch, b_idx, true);
            if (llama_process(ctx, LLAMA_PROCESS_TYPE_DECODE, decode_batch) != 0) {
                break;
            }
            ++n_past;
        }

        if (decode_batch != nullptr) {
            llama_batch_ext_free(decode_batch);
        }
        llama_sampler_free(sampler);
        // Clear KV cache after completion so zero page state lingers in RAM/VRAM.
        reset_kv_cache_stateless(ctx);

        if (g_abort_flag.load(std::memory_order_acquire)) {
            error_out = "Inference cancelled";
            return false;
        }

        raw_json_out = std::move(output);
        return true;
    }
};

DirectLlamaState g_direct_state;

#else

// Dynamic dlopen bridge when llama.cpp source is not vendored at Gradle compile time.
// Supports either:
//  1) An optional stable-C runtime plugin library `libllama_mtmd_runtime.so`, OR
//  2) Probing dynamically supplied `libllama.so` + `libmtmd.so` shared libraries.
using FnBridgeInfer = int (*)(
    const char* image_path,
    const char* model_path,
    const char* mmproj_path,
    const char* target_lang,
    const char* prompt,
    int32_t n_ctx,
    int32_t flash_attn,
    const char* kv_type,
    int32_t use_mmap,
    char* out_buf,
    size_t out_cap
);
using FnBridgeVoid = void (*)();

struct DynamicBridgeState {
    void* runtime_handle = nullptr;
    void* llama_handle = nullptr;
    void* mtmd_handle = nullptr;

    FnBridgeInfer fn_infer = nullptr;
    FnBridgeVoid fn_abort = nullptr;
    FnBridgeVoid fn_unload = nullptr;

    bool raw_symbols_present = false;

    void try_load_libraries() {
        if (runtime_handle == nullptr) {
            runtime_handle = dlopen("libllama_mtmd_runtime.so", RTLD_NOW | RTLD_LOCAL);
            if (runtime_handle != nullptr) {
                fn_infer = reinterpret_cast<FnBridgeInfer>(
                    dlsym(runtime_handle, "rtmaga_mtmd_infer_page")
                );
                fn_abort = reinterpret_cast<FnBridgeVoid>(
                    dlsym(runtime_handle, "rtmaga_mtmd_abort")
                );
                fn_unload = reinterpret_cast<FnBridgeVoid>(
                    dlsym(runtime_handle, "rtmaga_mtmd_hard_unload")
                );
                if (fn_infer != nullptr) {
                    log_info("Bound dynamic libllama_mtmd_runtime.so bridge");
                    return;
                }
            }
        }

        if (llama_handle == nullptr) {
            llama_handle = dlopen("libllama.so", RTLD_NOW | RTLD_LOCAL);
        }
        if (mtmd_handle == nullptr) {
            mtmd_handle = dlopen("libmtmd.so", RTLD_NOW | RTLD_LOCAL);
        }
        if (llama_handle != nullptr && mtmd_handle != nullptr) {
            const bool has_llama =
                dlsym(llama_handle, "llama_model_load_from_file") != nullptr &&
                dlsym(llama_handle, "llama_init_from_model") != nullptr &&
                dlsym(llama_handle, "llama_free") != nullptr;
            const bool has_mtmd =
                dlsym(mtmd_handle, "mtmd_init_from_file") != nullptr &&
                dlsym(mtmd_handle, "mtmd_free") != nullptr &&
                dlsym(mtmd_handle, "mtmd_tokenize") != nullptr;
            raw_symbols_present = has_llama && has_mtmd;
        }
    }

    bool is_available() {
        try_load_libraries();
        return fn_infer != nullptr;
    }

    void abort_active() const {
        if (fn_abort != nullptr) {
            fn_abort();
        }
    }

    void hard_unload() {
        if (fn_unload != nullptr) {
            fn_unload();
        }
        fn_infer = nullptr;
        fn_abort = nullptr;
        fn_unload = nullptr;
        raw_symbols_present = false;

        if (runtime_handle != nullptr) {
            dlclose(runtime_handle);
            runtime_handle = nullptr;
        }
        if (mtmd_handle != nullptr) {
            dlclose(mtmd_handle);
            mtmd_handle = nullptr;
        }
        if (llama_handle != nullptr) {
            dlclose(llama_handle);
            llama_handle = nullptr;
        }
    }
};

DynamicBridgeState g_dynamic_state;

#endif

}  // namespace

extern "C" {

JNIEXPORT jboolean JNICALL
Java_com_rtmaga_reader_LlamaMtmdBridge_nativeIsRuntimeAvailable(
    JNIEnv* /*env*/,
    jobject /*thiz*/
) {
#ifdef RTMAGA_HAS_LLAMA_CPP
    return JNI_TRUE;
#else
    std::lock_guard<std::mutex> lock(g_runtime_mutex);
    return g_dynamic_state.is_available() ? JNI_TRUE : JNI_FALSE;
#endif
}

JNIEXPORT jboolean JNICALL
Java_com_rtmaga_reader_LlamaMtmdBridge_nativeIsVulkanSupported(
    JNIEnv* /*env*/,
    jobject /*thiz*/
) {
    return probe_vulkan_hardware() ? JNI_TRUE : JNI_FALSE;
}

JNIEXPORT void JNICALL
Java_com_rtmaga_reader_LlamaMtmdBridge_nativeAbortActive(
    JNIEnv* /*env*/,
    jobject /*thiz*/
) {
    g_abort_flag.store(true, std::memory_order_release);
#ifndef RTMAGA_HAS_LLAMA_CPP
    g_dynamic_state.abort_active();
#endif
}

JNIEXPORT void JNICALL
Java_com_rtmaga_reader_LlamaMtmdBridge_nativeHardUnload(
    JNIEnv* /*env*/,
    jobject /*thiz*/
) {
    g_abort_flag.store(true, std::memory_order_release);
    std::lock_guard<std::mutex> lock(g_runtime_mutex);
#ifdef RTMAGA_HAS_LLAMA_CPP
    g_direct_state.hard_unload();
#else
    g_dynamic_state.hard_unload();
#endif
    log_info("Native llama/mtmd state hard-unloaded");
}

JNIEXPORT jstring JNICALL
Java_com_rtmaga_reader_LlamaMtmdBridge_nativeInferPage(
    JNIEnv* env,
    jobject /*thiz*/,
    jstring j_image_path,
    jstring j_model_path,
    jstring j_mmproj_path,
    jstring j_target_lang,
    jstring j_prompt
) {
    const std::string image_path = jstring_to_utf8(env, j_image_path);
    const std::string model_path = jstring_to_utf8(env, j_model_path);
    const std::string mmproj_path = jstring_to_utf8(env, j_mmproj_path);
    const std::string target_lang = jstring_to_utf8(env, j_target_lang);
    const std::string prompt = jstring_to_utf8(env, j_prompt);

    if (image_path.empty() || model_path.empty() || mmproj_path.empty()) {
        throw_runtime_exception(env, "Missing imagePath, modelPath, or mmprojPath for Tier-2 VLM inference");
        return nullptr;
    }

    g_abort_flag.store(false, std::memory_order_release);
    std::lock_guard<std::mutex> lock(g_runtime_mutex);

#ifdef RTMAGA_HAS_LLAMA_CPP
    std::string raw_json;
    std::string error_msg;
    const bool ok = g_direct_state.infer_page(
        image_path,
        model_path,
        mmproj_path,
        target_lang,
        prompt,
        raw_json,
        error_msg
    );
    if (!ok) {
        log_warn(error_msg.c_str());
        throw_runtime_exception(env, error_msg);
        return nullptr;
    }
    return env->NewStringUTF(raw_json.c_str());
#else
    g_dynamic_state.try_load_libraries();
    if (g_dynamic_state.fn_infer != nullptr) {
        std::vector<char> out_buf(64 * 1024, '\0');
        const int rc = g_dynamic_state.fn_infer(
            image_path.c_str(),
            model_path.c_str(),
            mmproj_path.c_str(),
            target_lang.c_str(),
            prompt.c_str(),
            kContextSize,
            1,
            "q8_0",
            1,
            out_buf.data(),
            out_buf.size()
        );
        if (rc != 0 || g_abort_flag.load(std::memory_order_acquire)) {
            throw_runtime_exception(
                env,
                g_abort_flag.load(std::memory_order_acquire)
                    ? "Tier-2 VLM inference cancelled"
                    : ("Dynamic llama_mtmd runtime returned error code " + std::to_string(rc))
            );
            return nullptr;
        }
        return env->NewStringUTF(out_buf.data());
    }

    const std::string msg = g_dynamic_state.raw_symbols_present
        ? "Detected libllama.so/libmtmd.so without C bridge wrapper; build with -DLLAMA_CPP_DIR to enable direct Android VLM execution"
        : "Android Tier-2 llama.cpp mtmd native runtime is not bundled in this build (build with LLAMA_CPP_DIR or switch to Lite tier)";
    log_warn(msg.c_str());
    throw_runtime_exception(env, msg);
    return nullptr;
#endif
}

}  // extern "C"
