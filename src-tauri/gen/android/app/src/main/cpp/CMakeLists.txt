cmake_minimum_required(VERSION 3.22.1)
project(llama_mtmd_jni LANGUAGES CXX C)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(LLAMA_CPP_DIR "" CACHE PATH "Optional local path to llama.cpp checkout for direct static linking")
option(RTMAGA_SKIP_LLAMA_FETCH "Skip automatic FetchContent download of llama.cpp b12808" OFF)
option(RTMAGA_ANDROID_VULKAN "Enable GGML_VULKAN on Android when host shader toolchain is available" OFF)

add_library(llama_mtmd_jni SHARED
    llama_mtmd_jni.cpp
)

find_library(ANDROID_LOG_LIB log)
find_library(DL_LIB dl)

target_compile_options(llama_mtmd_jni PRIVATE
    -Wall
    -Wextra
    -Werror=return-type
    -fvisibility=hidden
)

# 16KB ELF segment alignment for Android 15+ (API 35+) devices, plus dead-code elimination.
target_link_options(llama_mtmd_jni PRIVATE
    "-Wl,-z,max-page-size=16384"
    "-Wl,--gc-sections"
)
if(NOT CMAKE_BUILD_TYPE STREQUAL "Debug")
    target_link_options(llama_mtmd_jni PRIVATE "-Wl,--strip-all")
endif()

find_package(Vulkan QUIET)
if(Vulkan_FOUND)
    target_compile_definitions(llama_mtmd_jni PRIVATE RTMAGA_HAS_VULKAN_HEADERS=1)
    if(Vulkan_INCLUDE_DIRS)
        target_include_directories(llama_mtmd_jni PRIVATE ${Vulkan_INCLUDE_DIRS})
    endif()
endif()

set(RTMAGA_LLAMA_SRC "")
if(LLAMA_CPP_DIR AND EXISTS "${LLAMA_CPP_DIR}/CMakeLists.txt")
    set(RTMAGA_LLAMA_SRC "${LLAMA_CPP_DIR}")
    message(STATUS "RtMaga JNI: Using local llama.cpp at ${RTMAGA_LLAMA_SRC}")
elseif(NOT RTMAGA_SKIP_LLAMA_FETCH)
    include(FetchContent)
    message(STATUS "RtMaga JNI: Fetching pinned llama.cpp b12808 (ed2153e708de813c35559102aed43b3085179c27)")
    set(_RTMAGA_FETCH_ARGS
        URL "https://github.com/ggml-org/llama.cpp/archive/ed2153e708de813c35559102aed43b3085179c27.tar.gz"
    )
    if(CMAKE_VERSION VERSION_GREATER_EQUAL "3.24")
        list(APPEND _RTMAGA_FETCH_ARGS DOWNLOAD_EXTRACT_TIMESTAMP TRUE)
    endif()
    FetchContent_Declare(llama_cpp_src ${_RTMAGA_FETCH_ARGS})
    FetchContent_GetProperties(llama_cpp_src)
    if(NOT llama_cpp_src_POPULATED)
        FetchContent_Populate(llama_cpp_src)
    endif()
    if(EXISTS "${llama_cpp_src_SOURCE_DIR}/CMakeLists.txt")
        set(RTMAGA_LLAMA_SRC "${llama_cpp_src_SOURCE_DIR}")
    endif()
endif()

if(RTMAGA_LLAMA_SRC AND EXISTS "${RTMAGA_LLAMA_SRC}/CMakeLists.txt")
    message(STATUS "RtMaga JNI: Linking llama.cpp + mtmd statically from ${RTMAGA_LLAMA_SRC}")
    set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
    set(LLAMA_BUILD_TESTS OFF CACHE BOOL "" FORCE)
    set(LLAMA_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
    set(LLAMA_BUILD_TOOLS ON CACHE BOOL "" FORCE)
    set(LLAMA_BUILD_COMMON ON CACHE BOOL "" FORCE)
    set(LLAMA_BUILD_SERVER OFF CACHE BOOL "" FORCE)
    set(LLAMA_CURL OFF CACHE BOOL "" FORCE)
    set(LLAMA_HTTPLIB OFF CACHE BOOL "" FORCE)
    set(GGML_OPENMP OFF CACHE BOOL "" FORCE)
    set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
    set(GGML_BACKEND_DL OFF CACHE BOOL "" FORCE)
    set(GGML_CPU_ALL_VARIANTS OFF CACHE BOOL "" FORCE)

    if(RTMAGA_ANDROID_VULKAN AND Vulkan_FOUND)
        if(NOT Vulkan_GLSLC_EXECUTABLE AND DEFINED ANDROID_NDK)
            find_program(RTMAGA_NDK_GLSLC
                NAMES glslc glslc.exe
                PATHS
                    "${ANDROID_NDK}/shader-tools/windows-x86_64"
                    "${ANDROID_NDK}/shader-tools/linux-x86_64"
                    "${ANDROID_NDK}/shader-tools/darwin-x86_64"
                NO_DEFAULT_PATH
            )
            if(RTMAGA_NDK_GLSLC)
                set(Vulkan_GLSLC_EXECUTABLE "${RTMAGA_NDK_GLSLC}" CACHE FILEPATH "" FORCE)
            endif()
        endif()
        set(GGML_VULKAN ON CACHE BOOL "" FORCE)
        message(STATUS "RtMaga JNI: GGML_VULKAN=ON")
    else()
        set(GGML_VULKAN OFF CACHE BOOL "" FORCE)
        message(STATUS "RtMaga JNI: Building optimized ARM64 CPU backend (GGML_VULKAN=OFF)")
    endif()

    add_subdirectory("${RTMAGA_LLAMA_SRC}" "${CMAKE_BINARY_DIR}/llama_cpp" EXCLUDE_FROM_ALL)

    target_compile_definitions(llama_mtmd_jni PRIVATE RTMAGA_HAS_LLAMA_CPP=1)
    target_include_directories(llama_mtmd_jni PRIVATE
        "${RTMAGA_LLAMA_SRC}/include"
        "${RTMAGA_LLAMA_SRC}/ggml/include"
        "${RTMAGA_LLAMA_SRC}/tools/mtmd"
    )
    target_link_libraries(llama_mtmd_jni PRIVATE
        llama
        mtmd
        ${ANDROID_LOG_LIB}
        ${DL_LIB}
    )
    if(RTMAGA_ANDROID_VULKAN AND Vulkan_FOUND)
        target_link_libraries(llama_mtmd_jni PRIVATE Vulkan::Vulkan)
    endif()
else()
    message(STATUS "RtMaga JNI: llama.cpp source unavailable; building dynamic dlopen bridge")
    target_link_libraries(llama_mtmd_jni PRIVATE
        ${ANDROID_LOG_LIB}
        ${DL_LIB}
    )
endif()
