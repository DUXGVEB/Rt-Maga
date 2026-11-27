cmake_minimum_required(VERSION 3.22.1)
project(llama_mtmd_jni LANGUAGES CXX C)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

set(LLAMA_CPP_DIR "" CACHE PATH "Optional path to llama.cpp checkout for direct static linking with GGML_VULKAN")

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

# 16KB ELF segment alignment for Android 15+ (API 35+) devices.
target_link_options(llama_mtmd_jni PRIVATE
    "-Wl,-z,max-page-size=16384"
)

find_package(Vulkan QUIET)
if(Vulkan_FOUND)
    target_compile_definitions(llama_mtmd_jni PRIVATE RTMAGA_HAS_VULKAN_HEADERS=1)
    if(Vulkan_INCLUDE_DIRS)
        target_include_directories(llama_mtmd_jni PRIVATE ${Vulkan_INCLUDE_DIRS})
    endif()
endif()

if(LLAMA_CPP_DIR AND EXISTS "${LLAMA_CPP_DIR}/CMakeLists.txt")
    message(STATUS "RtMaga JNI: Linking llama.cpp + mtmd directly from ${LLAMA_CPP_DIR}")
    set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
    set(LLAMA_BUILD_TESTS OFF CACHE BOOL "" FORCE)
    set(LLAMA_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
    set(LLAMA_BUILD_TOOLS ON CACHE BOOL "" FORCE)
    set(LLAMA_BUILD_SERVER OFF CACHE BOOL "" FORCE)
    set(LLAMA_CURL OFF CACHE BOOL "" FORCE)
    set(GGML_OPENMP OFF CACHE BOOL "" FORCE)

    if(Vulkan_FOUND)
        set(GGML_VULKAN ON CACHE BOOL "" FORCE)
        message(STATUS "RtMaga JNI: GGML_VULKAN=ON")
    else()
        set(GGML_VULKAN OFF CACHE BOOL "" FORCE)
        message(STATUS "RtMaga JNI: Vulkan not found in toolchain; building CPU fallback")
    endif()

    add_subdirectory("${LLAMA_CPP_DIR}" "${CMAKE_BINARY_DIR}/llama_cpp" EXCLUDE_FROM_ALL)

    target_compile_definitions(llama_mtmd_jni PRIVATE RTMAGA_HAS_LLAMA_CPP=1)
    target_include_directories(llama_mtmd_jni PRIVATE
        "${LLAMA_CPP_DIR}/include"
        "${LLAMA_CPP_DIR}/ggml/include"
        "${LLAMA_CPP_DIR}/tools/mtmd"
    )
    target_link_libraries(llama_mtmd_jni PRIVATE
        llama
        mtmd
        ${ANDROID_LOG_LIB}
        ${DL_LIB}
    )
    if(Vulkan_FOUND)
        target_link_libraries(llama_mtmd_jni PRIVATE Vulkan::Vulkan)
    endif()
else()
    message(STATUS "RtMaga JNI: LLAMA_CPP_DIR not configured; building dynamic dlopen bridge")
    target_link_libraries(llama_mtmd_jni PRIVATE
        ${ANDROID_LOG_LIB}
        ${DL_LIB}
    )
endif()
