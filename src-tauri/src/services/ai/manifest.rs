//! Verified manifest of official AI model weights, OCR dictionaries, OPUS-MT ONNX translation
//! packs, and Windows `llama.cpp` sidecar binaries.
//!
//! Every entry embeds an official upstream URL, exact byte length, and real SHA-256 digest
//! verified against Hugging Face / GitHub Releases.

/// Functional category of a verified AI model or runtime asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiAssetKind {
    /// Tier 1 compact ONNX OCR detector, recognizer, or dictionary.
    Tier1Ocr,
    /// Tier 2 multimodal Vision-Language Model GGUF weights or vision projector.
    Tier2Vlm,
    /// Windows x64 headless `llama.cpp` sidecar archive (`llama-mtmd-cli.exe` + DLLs).
    WindowsSidecar,
    /// Optional Windows ONNX OPUS-MT neural machine translation pack.
    #[cfg(any(not(target_os = "android"), test))]
    WindowsNmtPack,
}

/// Packaging layout of a downloaded asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiArchiveFormat {
    /// Downloaded directly to its relative path under the AI storage root.
    RawFile,
    /// ZIP archive extracted with zip-slip protection into `<storage_root>/bin/llama-b12808/`.
    ZipArchive,
}

/// Immutable verified descriptor for a single downloadable AI asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModelAssetSpec {
    /// Stable machine identifier reported in download progress events.
    pub id: &'static str,
    /// Relative destination path inside the AI model storage root (or archive staging filename).
    pub rel_path: &'static str,
    /// Official HTTPS download URL (`huggingface.co` or `github.com`).
    pub url: &'static str,
    /// Exact expected byte length verified before atomic promotion.
    pub expected_bytes: u64,
    /// Exact lowercase hexadecimal SHA-256 digest verified over the downloaded bytes.
    pub sha256_hex: &'static str,
    /// Asset category.
    pub kind: AiAssetKind,
    /// File or archive handling strategy.
    pub archive: AiArchiveFormat,
}

/// Maximum total Tier 1 bundled asset + native runtime budget (`35 MiB`).
pub(crate) const TIER1_BUDGET_BYTES: u64 = 35 * 1024 * 1024;

/// Relative subdirectory where the extracted Windows `llama.cpp` `b12808` sidecar resides.
pub(crate) const WIN_LLAMA_SIDECAR_DIR: &str = "bin/llama-b12808";

/// Tier 1 `PP-OCRv6_tiny` text detection ONNX model (`1.78 MB`).
pub(crate) const TIER1_OCR_DET: ModelAssetSpec = ModelAssetSpec {
    id: "ppocrv6_tiny_det_onnx",
    rel_path: "tier1/det.onnx",
    url: "https://huggingface.co/PaddlePaddle/PP-OCRv6_tiny_det_onnx/resolve/main/inference.onnx",
    expected_bytes: 1_780_590,
    sha256_hex: "193bab7a04fca699a6c82e6abb5b81bdb28177f0abd4062552b04908dafb19f8",
    kind: AiAssetKind::Tier1Ocr,
    archive: AiArchiveFormat::RawFile,
};

/// Tier 1 `PP-OCRv6_tiny` text recognition ONNX model (`4.46 MB`).
pub(crate) const TIER1_OCR_REC: ModelAssetSpec = ModelAssetSpec {
    id: "ppocrv6_tiny_rec_onnx",
    rel_path: "tier1/rec.onnx",
    url: "https://huggingface.co/PaddlePaddle/PP-OCRv6_tiny_rec_onnx/resolve/main/inference.onnx",
    expected_bytes: 4_462_639,
    sha256_hex: "9ef676d6ed3c88256a2d92c640c44f25b0c40947e111b14b8be8f594091563e6",
    kind: AiAssetKind::Tier1Ocr,
    archive: AiArchiveFormat::RawFile,
};

/// Tier 1 `PP-OCRv6_tiny` CTC character dictionary & preprocessing config (`55.6 KB`).
pub(crate) const TIER1_OCR_DICT: ModelAssetSpec = ModelAssetSpec {
    id: "ppocrv6_tiny_rec_dict",
    rel_path: "tier1/inference.yml",
    url: "https://huggingface.co/PaddlePaddle/PP-OCRv6_tiny_rec_onnx/resolve/main/inference.yml",
    expected_bytes: 55_571,
    sha256_hex: "66170210bad538e83fff3c4a3867e547d6bf20b50d64b20347c4b913f3034ea1",
    kind: AiAssetKind::Tier1Ocr,
    archive: AiArchiveFormat::RawFile,
};

/// Tier 2 `Qwen3-VL-2B-Instruct` `IQ4_XS` quantized GGUF weights (`1.01 GB`).
pub(crate) const TIER2_VLM_GGUF: ModelAssetSpec = ModelAssetSpec {
    id: "qwen3_vl_2b_gguf",
    rel_path: "tier2/Qwen3-VL-2B-Instruct-IQ4_XS.gguf",
    url: "https://huggingface.co/unsloth/Qwen3-VL-2B-Instruct-GGUF/resolve/main/Qwen3-VL-2B-Instruct-IQ4_XS.gguf",
    expected_bytes: 1_010_384_576,
    sha256_hex: "fb10b41100e1b4115f55de4ead6c892d1d2459dba67e07b1cf7279d4c5c79bd7",
    kind: AiAssetKind::Tier2Vlm,
    archive: AiArchiveFormat::RawFile,
};

/// Tier 2 `Qwen3-VL-2B-Instruct` `Q8_0` multimodal vision projector (`445 MB`).
pub(crate) const TIER2_VLM_MMPROJ: ModelAssetSpec = ModelAssetSpec {
    id: "qwen3_vl_2b_mmproj",
    rel_path: "tier2/mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf",
    url: "https://huggingface.co/Qwen/Qwen3-VL-2B-Instruct-GGUF/resolve/main/mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf",
    expected_bytes: 445_053_216,
    sha256_hex: "f9a68fabba69c3b81e153367b2c7521030b0fa8bb0de400c9599c8e6725f9c82",
    kind: AiAssetKind::Tier2Vlm,
    archive: AiArchiveFormat::RawFile,
};

/// Windows x64 `llama.cpp` `b12808` headless Vulkan sidecar archive (`39.2 MB`).
pub(crate) const WIN_LLAMA_VULKAN: ModelAssetSpec = ModelAssetSpec {
    id: "llama_cpp_win_vulkan_b12808",
    rel_path: "downloads/llama-b12808-bin-win-vulkan-x64.zip",
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12808/llama-b12808-bin-win-vulkan-x64.zip",
    expected_bytes: 39_198_554,
    sha256_hex: "a5342c87aa185cfa777f9c6360465be1b1a971dac4336a48842abc3b3377eaee",
    kind: AiAssetKind::WindowsSidecar,
    archive: AiArchiveFormat::ZipArchive,
};

/// Windows x64 `llama.cpp` `b12808` headless CPU fallback sidecar archive (`25.3 MB`).
pub(crate) const WIN_LLAMA_CPU: ModelAssetSpec = ModelAssetSpec {
    id: "llama_cpp_win_cpu_b12808",
    rel_path: "downloads/llama-b12808-bin-win-cpu-x64.zip",
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12808/llama-b12808-bin-win-cpu-x64.zip",
    expected_bytes: 25_324_778,
    sha256_hex: "841e8aea2fc265a40f81347aed57902343fba12918b0eab024ae8fabf91d770a",
    kind: AiAssetKind::WindowsSidecar,
    archive: AiArchiveFormat::ZipArchive,
};

/// Verified quantized OPUS-MT ONNX translation pack: Japanese -> English (`Xenova/opus-mt-ja-en`).
#[cfg(any(not(target_os = "android"), test))]
pub(crate) const OPUS_MT_JA_EN: [ModelAssetSpec; 5] = [
    ModelAssetSpec {
        id: "opus_mt_ja_en_encoder",
        rel_path: "nmt/ja-en/encoder_model_quantized.onnx",
        url: "https://huggingface.co/Xenova/opus-mt-ja-en/resolve/main/onnx/encoder_model_quantized.onnx",
        expected_bytes: 50_705_822,
        sha256_hex: "345262b16bcdda1468b0f3380c112b7ce79f731176b4b1d21f6edd5b2ae0d25c",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_ja_en_decoder",
        rel_path: "nmt/ja-en/decoder_model_quantized.onnx",
        url: "https://huggingface.co/Xenova/opus-mt-ja-en/resolve/main/onnx/decoder_model_quantized.onnx",
        expected_bytes: 57_631_042,
        sha256_hex: "ec639efdbb8dd9a5c458ab3aeeaee34402b21200eb9c24a95a32a16f0bb22d66",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_ja_en_source_spm",
        rel_path: "nmt/ja-en/source.spm",
        url: "https://huggingface.co/Xenova/opus-mt-ja-en/resolve/main/source.spm",
        expected_bytes: 781_853,
        sha256_hex: "d0b5c3b10b5959f056ff2c86e2f2356129242ff1fc72d3a4a34d6a8c0eee4e57",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_ja_en_vocab",
        rel_path: "nmt/ja-en/vocab.json",
        url: "https://huggingface.co/Xenova/opus-mt-ja-en/resolve/main/vocab.json",
        expected_bytes: 1_622_775,
        sha256_hex: "a1667b43d021e09c76f385101bd473f20b5691a12cdca7dda43e4a8daff20021",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_ja_en_config",
        rel_path: "nmt/ja-en/config.json",
        url: "https://huggingface.co/Xenova/opus-mt-ja-en/resolve/main/config.json",
        expected_bytes: 1_376,
        sha256_hex: "d35476f1d3858883feec67e5a63ddc387452a70fda9c8e26db99c71aa2ad6a34",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
];

/// Verified quantized OPUS-MT ONNX translation pack: English -> Vietnamese (`Xenova/opus-mt-en-vi`).
#[cfg(any(not(target_os = "android"), test))]
pub(crate) const OPUS_MT_EN_VI: [ModelAssetSpec; 5] = [
    ModelAssetSpec {
        id: "opus_mt_en_vi_encoder",
        rel_path: "nmt/en-vi/encoder_model_quantized.onnx",
        url: "https://huggingface.co/Xenova/opus-mt-en-vi/resolve/main/onnx/encoder_model_quantized.onnx",
        expected_bytes: 47_105_950,
        sha256_hex: "dfcd7a32be3fdd21cbcf5d354520d626f1768fd6e205ca62a6a11a4641d31f36",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_en_vi_decoder",
        rel_path: "nmt/en-vi/decoder_model_quantized.onnx",
        url: "https://huggingface.co/Xenova/opus-mt-en-vi/resolve/main/onnx/decoder_model_quantized.onnx",
        expected_bytes: 54_003_046,
        sha256_hex: "cde3e02aaca72cfaf2267f6c31c4dcc1b323d769bf90c273f2a1c1d56198ddba",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_en_vi_source_spm",
        rel_path: "nmt/en-vi/source.spm",
        url: "https://huggingface.co/Xenova/opus-mt-en-vi/resolve/main/source.spm",
        expected_bytes: 808_612,
        sha256_hex: "fdd9ab486ca8ea060967105e30db7d8928729c78a67448711d2dd7c3153b37cd",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_en_vi_vocab",
        rel_path: "nmt/en-vi/vocab.json",
        url: "https://huggingface.co/Xenova/opus-mt-en-vi/resolve/main/vocab.json",
        expected_bytes: 1_299_764,
        sha256_hex: "bcc8bbafeb0b6b58fecf244f8da996da9aa45dbea7871164f9c8ce480507f9fe",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_en_vi_config",
        rel_path: "nmt/en-vi/config.json",
        url: "https://huggingface.co/Xenova/opus-mt-en-vi/resolve/main/config.json",
        expected_bytes: 1_389,
        sha256_hex: "0c5ab981c2f5a4878c78522c8a61e3d733f5e3be6b1f83b68bbfb7f00e604637",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
];

/// Verified quantized OPUS-MT ONNX translation pack: Chinese -> English (`Xenova/opus-mt-zh-en`).
#[cfg(any(not(target_os = "android"), test))]
pub(crate) const OPUS_MT_ZH_EN: [ModelAssetSpec; 5] = [
    ModelAssetSpec {
        id: "opus_mt_zh_en_encoder",
        rel_path: "nmt/zh-en/encoder_model_quantized.onnx",
        url: "https://huggingface.co/Xenova/opus-mt-zh-en/resolve/main/onnx/encoder_model_quantized.onnx",
        expected_bytes: 52_899_742,
        sha256_hex: "84d5e171b626bc8b6b220d022ac58696e9528c25deeacca62b5cbf4364547a99",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_zh_en_decoder",
        rel_path: "nmt/zh-en/decoder_model_quantized.onnx",
        url: "https://huggingface.co/Xenova/opus-mt-zh-en/resolve/main/onnx/decoder_model_quantized.onnx",
        expected_bytes: 59_842_102,
        sha256_hex: "debcc3054b9ff9aaed972bd6e251a459abe1e2f1402da1efa63163dc2f46ef73",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_zh_en_source_spm",
        rel_path: "nmt/zh-en/source.spm",
        url: "https://huggingface.co/Xenova/opus-mt-zh-en/resolve/main/source.spm",
        expected_bytes: 804_677,
        sha256_hex: "e27a3a1b539f4959ec72ea60e453f49156289f95d4e6000b29332efc45616203",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_zh_en_vocab",
        rel_path: "nmt/zh-en/vocab.json",
        url: "https://huggingface.co/Xenova/opus-mt-zh-en/resolve/main/vocab.json",
        expected_bytes: 1_747_906,
        sha256_hex: "08a119a1defd522fa047cb5e3bfe3e89633e96caa38ced0dc9cee7ef1021a011",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_zh_en_config",
        rel_path: "nmt/zh-en/config.json",
        url: "https://huggingface.co/Xenova/opus-mt-zh-en/resolve/main/config.json",
        expected_bytes: 1_389,
        sha256_hex: "293d318fce41dbf04114eac45037bb88a32d7c4ee21011a75e24a8b98ca45ad1",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
];

/// Verified quantized OPUS-MT ONNX translation pack: Korean -> English (`Xenova/opus-mt-ko-en`).
#[cfg(any(not(target_os = "android"), test))]
pub(crate) const OPUS_MT_KO_EN: [ModelAssetSpec; 5] = [
    ModelAssetSpec {
        id: "opus_mt_ko_en_encoder",
        rel_path: "nmt/ko-en/encoder_model_quantized.onnx",
        url: "https://huggingface.co/Xenova/opus-mt-ko-en/resolve/main/onnx/encoder_model_quantized.onnx",
        expected_bytes: 52_899_741,
        sha256_hex: "210e1d2cb5b457cdeeffe288099cc9d1df9f13c35dc3d39fde4ba252af877380",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_ko_en_decoder",
        rel_path: "nmt/ko-en/decoder_model_quantized.onnx",
        url: "https://huggingface.co/Xenova/opus-mt-ko-en/resolve/main/onnx/decoder_model_quantized.onnx",
        expected_bytes: 59_842_101,
        sha256_hex: "0c961ea33a62ea84e73c9ec94c0737e65d06b5f73a7d0398b018fbbb7262d929",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_ko_en_source_spm",
        rel_path: "nmt/ko-en/source.spm",
        url: "https://huggingface.co/Xenova/opus-mt-ko-en/resolve/main/source.spm",
        expected_bytes: 841_805,
        sha256_hex: "c9496f7c2be9aecb84c751ae9f35a875915dde8e3892f652a5c76811ab2a0f49",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_ko_en_vocab",
        rel_path: "nmt/ko-en/vocab.json",
        url: "https://huggingface.co/Xenova/opus-mt-ko-en/resolve/main/vocab.json",
        expected_bytes: 1_849_870,
        sha256_hex: "97197044d4803c664a37f8ca2ced9c3405b8a3df6b262a12da22d990b0d756cd",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
    ModelAssetSpec {
        id: "opus_mt_ko_en_config",
        rel_path: "nmt/ko-en/config.json",
        url: "https://huggingface.co/Xenova/opus-mt-ko-en/resolve/main/config.json",
        expected_bytes: 1_389,
        sha256_hex: "8260080db48141328ae198ede2b25483fa4a6f46dc2859784d6e21c188865e43",
        kind: AiAssetKind::WindowsNmtPack,
        archive: AiArchiveFormat::RawFile,
    },
];

/// Descriptor for a selectable Windows OPUS-MT offline translation pack.
#[cfg(any(not(target_os = "android"), test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpusPackMeta {
    pub id: &'static str,
    pub display_name: &'static str,
    pub source_lang: &'static str,
    pub target_lang: &'static str,
    pub assets: &'static [ModelAssetSpec],
}

/// All verified offline Windows OPUS-MT translation packs.
#[cfg(any(not(target_os = "android"), test))]
pub(crate) const ALL_OPUS_MT_PACKS: [OpusPackMeta; 4] = [
    OpusPackMeta {
        id: "ja-en",
        display_name: "JA → EN",
        source_lang: "ja",
        target_lang: "en",
        assets: &OPUS_MT_JA_EN,
    },
    OpusPackMeta {
        id: "zh-en",
        display_name: "ZH → EN",
        source_lang: "zh",
        target_lang: "en",
        assets: &OPUS_MT_ZH_EN,
    },
    OpusPackMeta {
        id: "ko-en",
        display_name: "KO → EN",
        source_lang: "ko",
        target_lang: "en",
        assets: &OPUS_MT_KO_EN,
    },
    OpusPackMeta {
        id: "en-vi",
        display_name: "EN → VI",
        source_lang: "en",
        target_lang: "vi",
        assets: &OPUS_MT_EN_VI,
    },
];

/// Sidecar flavor selected for Windows Tier 2 `llama.cpp` provisioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SidecarTarget {
    /// No external sidecar needed (Android uses JNI `libllama_mtmd_jni.so`).
    None,
    /// Windows x64 Vulkan-accelerated `llama.cpp` `b12808` binary.
    WindowsVulkan,
    /// Windows x64 CPU-only `llama.cpp` `b12808` binary.
    WindowsCpu,
}

/// Returns the Tier 1 `PP-OCRv6_tiny` asset bundle (`det.onnx`, `rec.onnx`, `inference.yml`).
pub(crate) const fn tier1_ocr_assets() -> [ModelAssetSpec; 3] {
    [TIER1_OCR_DET, TIER1_OCR_REC, TIER1_OCR_DICT]
}

/// Returns the Tier 2 `Qwen3-VL-2B` asset list for the active target.
pub(crate) fn tier2_bundle_assets(sidecar: SidecarTarget) -> Vec<ModelAssetSpec> {
    let mut assets = vec![TIER2_VLM_GGUF, TIER2_VLM_MMPROJ];
    match sidecar {
        SidecarTarget::None => {}
        SidecarTarget::WindowsVulkan => assets.push(WIN_LLAMA_VULKAN),
        SidecarTarget::WindowsCpu => assets.push(WIN_LLAMA_CPU),
    }
    assets
}

#[cfg(any(not(target_os = "android"), test))]
pub(crate) fn opus_mt_pack_meta(pair: &str) -> Option<&'static OpusPackMeta> {
    ALL_OPUS_MT_PACKS.iter().find(|meta| meta.id == pair)
}

/// Looks up a verified Windows OPUS-MT ONNX translation pack by pair slug (`"ja-en"`, `"en-vi"`,
/// `"zh-en"`, `"ko-en"`).
#[cfg(any(not(target_os = "android"), test))]
pub(crate) fn opus_mt_pack(pair: &str) -> Option<&'static [ModelAssetSpec]> {
    opus_mt_pack_meta(pair).map(|meta| meta.assets)
}

/// Computes the exact cumulative byte footprint of `assets`.
pub(crate) fn total_bytes(assets: &[ModelAssetSpec]) -> u64 {
    assets
        .iter()
        .fold(0_u64, |acc, a| acc.saturating_add(a.expected_bytes))
}

/// Validates that `digest` is a 64-character lowercase hexadecimal SHA-256 string.
pub(crate) fn is_valid_sha256_hex(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Validates that a manifest URL is HTTPS on an allowlisted official release host.
pub(crate) fn is_allowed_manifest_url(raw_url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(raw_url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    matches!(
        parsed.host_str(),
        Some("huggingface.co" | "github.com" | "objects.githubusercontent.com")
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn all_manifest_assets() -> Vec<ModelAssetSpec> {
        let mut all = Vec::new();
        all.extend_from_slice(&tier1_ocr_assets());
        all.push(TIER2_VLM_GGUF);
        all.push(TIER2_VLM_MMPROJ);
        all.push(WIN_LLAMA_VULKAN);
        all.push(WIN_LLAMA_CPU);
        all.extend_from_slice(&OPUS_MT_JA_EN);
        all.extend_from_slice(&OPUS_MT_EN_VI);
        all.extend_from_slice(&OPUS_MT_ZH_EN);
        all.extend_from_slice(&OPUS_MT_KO_EN);
        all
    }

    #[test]
    fn every_manifest_asset_has_verified_url_size_and_sha256() {
        let all = all_manifest_assets();
        let mut ids = HashSet::new();
        for asset in &all {
            assert!(
                ids.insert(asset.id),
                "duplicate manifest asset id: {}",
                asset.id
            );
            assert!(
                is_allowed_manifest_url(asset.url),
                "asset {} URL must be allowlisted HTTPS: {}",
                asset.id,
                asset.url
            );
            assert!(
                asset.expected_bytes > 0,
                "asset {} must declare non-zero expected_bytes",
                asset.id
            );
            assert!(
                is_valid_sha256_hex(asset.sha256_hex),
                "asset {} has invalid SHA-256 digest: {}",
                asset.id,
                asset.sha256_hex
            );
            assert!(
                !asset.sha256_hex.chars().all(|c| c == '0'),
                "asset {} must never use a placeholder zero hash",
                asset.id
            );
            assert!(
                !asset.rel_path.contains("..") && !asset.rel_path.starts_with('/'),
                "asset {} rel_path must be safe relative path: {}",
                asset.id,
                asset.rel_path
            );
        }
    }

    #[test]
    fn tier1_bundle_stays_well_under_35mb_budget() {
        let tier1_total = total_bytes(&tier1_ocr_assets());
        assert!(
            tier1_total < TIER1_BUDGET_BYTES,
            "Tier 1 OCR assets ({tier1_total} B) must stay below 35 MiB budget"
        );
        assert_eq!(
            tier1_total,
            1_780_590 + 4_462_639 + 55_571,
            "Tier 1 OCR total must equal exact sum of verified PP-OCRv6 tiny files"
        );
    }

    #[test]
    fn tier2_bundle_includes_sidecar_only_for_windows_targets() {
        let android_bundle = tier2_bundle_assets(SidecarTarget::None);
        assert_eq!(android_bundle.len(), 2, "Android bundle has GGUF + mmproj");
        assert_eq!(
            total_bytes(&android_bundle),
            1_010_384_576 + 445_053_216,
            "Android Tier 2 bundle byte total"
        );

        let win_vulkan = tier2_bundle_assets(SidecarTarget::WindowsVulkan);
        assert_eq!(
            win_vulkan.len(),
            3,
            "Windows Vulkan bundle adds llama.cpp Vulkan zip"
        );
        assert_eq!(
            win_vulkan[2].id, WIN_LLAMA_VULKAN.id,
            "Windows Vulkan bundle must include Vulkan sidecar"
        );

        let win_cpu = tier2_bundle_assets(SidecarTarget::WindowsCpu);
        assert_eq!(win_cpu[2].id, WIN_LLAMA_CPU.id, "Windows CPU sidecar");
    }

    #[test]
    fn opus_mt_lookup_resolves_verified_language_packs() {
        assert!(opus_mt_pack("ja-en").is_some(), "ja-en pack exists");
        assert!(opus_mt_pack("en-vi").is_some(), "en-vi pack exists");
        assert!(opus_mt_pack("zh-en").is_some(), "zh-en pack exists");
        assert!(opus_mt_pack("ko-en").is_some(), "ko-en pack exists");
        assert!(opus_mt_pack("xx-yy").is_none(), "unknown pack returns None");
    }
}
