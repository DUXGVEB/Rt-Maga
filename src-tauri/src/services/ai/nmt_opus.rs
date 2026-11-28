//! Pure-Rust offline `MarianMT` (`OPUS-MT`) neural machine translation engine using `ort` ONNX
//! sessions and `prost`-decoded `SentencePiece` Unigram models (`source.spm` + `vocab.json`).
//!
//! Supports direct pairs (`ja-en`, `zh-en`, `ko-en`, `en-vi`) as well as automatic English pivot
//! chaining (`ja -> en -> vi`, `zh -> en -> vi`, `ko -> en -> vi`) with zero cloud dependencies.

use std::collections::HashMap;
use std::path::Path;

use prost::Message;
use serde::Deserialize;

use crate::error::AppError;
use crate::services::ai::manifest::{ALL_OPUS_MT_PACKS, opus_mt_pack_meta, total_bytes};
use crate::services::ai::{AiModelStatus, AiNmtPackView};

/// `SentencePiece` whitespace prefix marker (`U+2581` LOWER ONE EIGHTH BLOCK).
const SP_SPACE: char = '\u{2581}';

/// Penalty assigned to single-character fallback steps outside the Unigram vocabulary.
const UNKNOWN_CHAR_PENALTY: f32 = -20.0;

/// Hard upper bound on generated decoder tokens per speech bubble.
const MAX_DECODE_TOKENS: usize = 96;

/// Minimal protobuf mirror of `sentencepiece.ModelProto` (`sentencepiece_model.proto`).
#[derive(Clone, PartialEq, Message)]
struct SpModelProto {
    #[prost(message, repeated, tag = "1")]
    pieces: Vec<SpSentencePiece>,
}

/// Minimal protobuf mirror of `sentencepiece.ModelProto.SentencePiece`.
#[derive(Clone, PartialEq, Message)]
struct SpSentencePiece {
    #[prost(string, tag = "1")]
    piece: String,
    #[prost(float, tag = "2")]
    score: f32,
    #[prost(int32, tag = "3")]
    r#type: i32,
}

/// `MarianMT` generation parameters loaded from `config.json`.
#[derive(Debug, Clone, Deserialize)]
struct OpusMtConfigJson {
    decoder_start_token_id: i64,
    #[serde(default)]
    eos_token_id: i64,
    pad_token_id: Option<i64>,
    #[serde(default = "default_max_length")]
    max_length: usize,
}

const fn default_max_length() -> usize {
    512
}

/// Loaded `SentencePiece` Unigram tokenizer + `MarianMT` vocabulary tables.
#[derive(Debug, Clone)]
pub(crate) struct OpusMtTokenizer {
    unigram_scores: HashMap<String, f32>,
    max_piece_bytes: usize,
    token_to_id: HashMap<String, i64>,
    id_to_token: Vec<Option<String>>,
    unk_id: i64,
    eos_id: i64,
    pad_id: i64,
    decoder_start_id: i64,
    max_length: usize,
}

impl OpusMtTokenizer {
    /// Loads `source.spm`, `vocab.json`, and `config.json` from an installed OPUS-MT pack directory.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::CorruptedData`] if any file is missing or malformed.
    pub(crate) fn load_from_dir(pack_dir: &Path) -> Result<Self, AppError> {
        let spm_bytes =
            std::fs::read(pack_dir.join("source.spm")).map_err(|e| AppError::CorruptedData {
                message: format!("failed to read OPUS-MT source.spm: {e}"),
            })?;
        let vocab_str = std::fs::read_to_string(pack_dir.join("vocab.json")).map_err(|e| {
            AppError::CorruptedData {
                message: format!("failed to read OPUS-MT vocab.json: {e}"),
            }
        })?;
        let cfg_str = std::fs::read_to_string(pack_dir.join("config.json")).map_err(|e| {
            AppError::CorruptedData {
                message: format!("failed to read OPUS-MT config.json: {e}"),
            }
        })?;

        Self::from_raw(&spm_bytes, &vocab_str, &cfg_str)
    }

    /// Parses raw `source.spm`, `vocab.json`, and `config.json` payloads into a ready tokenizer.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::CorruptedData`] if protobuf or JSON decoding fails.
    pub(crate) fn from_raw(
        spm_bytes: &[u8],
        vocab_json: &str,
        config_json: &str,
    ) -> Result<Self, AppError> {
        let proto = SpModelProto::decode(spm_bytes).map_err(|e| AppError::CorruptedData {
            message: format!("failed to decode SentencePiece protobuf: {e}"),
        })?;
        let token_to_id: HashMap<String, i64> =
            serde_json::from_str(vocab_json).map_err(|e| AppError::CorruptedData {
                message: format!("failed to parse OPUS-MT vocab.json: {e}"),
            })?;
        let cfg: OpusMtConfigJson =
            serde_json::from_str(config_json).map_err(|e| AppError::CorruptedData {
                message: format!("failed to parse OPUS-MT config.json: {e}"),
            })?;

        let mut unigram_scores = HashMap::with_capacity(proto.pieces.len());
        let mut max_piece_bytes = 0_usize;
        for item in proto.pieces {
            // SentencePiece Type: 1 = NORMAL, 4 = USER_DEFINED.
            if (item.r#type == 0 || item.r#type == 1 || item.r#type == 4) && !item.piece.is_empty()
            {
                max_piece_bytes = max_piece_bytes.max(item.piece.len());
                unigram_scores.insert(item.piece, item.score);
            }
        }

        let max_id = token_to_id
            .values()
            .copied()
            .filter(|&id| id >= 0)
            .max()
            .unwrap_or(0);
        let table_len = usize::try_from(max_id).unwrap_or(0).saturating_add(1);
        let mut id_to_token: Vec<Option<String>> = vec![None; table_len];
        for (tok, &id) in &token_to_id {
            if let Ok(idx) = usize::try_from(id)
                && idx < id_to_token.len()
            {
                id_to_token[idx] = Some(tok.clone());
            }
        }

        let unk_id = token_to_id.get("<unk>").copied().unwrap_or(1);
        let pad_id = cfg.pad_token_id.unwrap_or(cfg.decoder_start_token_id);

        Ok(Self {
            unigram_scores,
            max_piece_bytes,
            token_to_id,
            id_to_token,
            unk_id,
            eos_id: cfg.eos_token_id,
            pad_id,
            decoder_start_id: cfg.decoder_start_token_id,
            max_length: cfg.max_length.clamp(32, 512),
        })
    }

    /// Encodes `text` into `MarianMT` token IDs terminated with `eos_id`.
    pub(crate) fn encode(&self, text: &str) -> Vec<i64> {
        let normalized = normalize_sentencepiece_input(text);
        if normalized.is_empty() {
            return vec![self.eos_id];
        }

        let pieces = self.viterbi_segment(&normalized);
        let mut ids = Vec::with_capacity(pieces.len().saturating_add(1));
        for piece in pieces {
            let id = self.token_to_id.get(piece).copied().unwrap_or(self.unk_id);
            ids.push(id);
            if ids.len().saturating_add(1) >= self.max_length {
                break;
            }
        }
        ids.push(self.eos_id);
        ids
    }

    /// Decodes generated `MarianMT` token IDs back into clean natural text.
    pub(crate) fn decode(&self, ids: &[i64]) -> String {
        let mut raw = String::new();
        for &id in ids {
            if id == self.eos_id || id == self.pad_id || id == self.decoder_start_id {
                continue;
            }
            if let Ok(idx) = usize::try_from(id)
                && let Some(Some(piece)) = self.id_to_token.get(idx)
            {
                if piece == "<pad>" || piece == "</s>" || piece == "<unk>" || piece == "<s>" {
                    continue;
                }
                raw.push_str(piece);
            }
        }

        let spaced = raw.replace(SP_SPACE, " ");
        collapse_whitespace(&spaced)
    }

    fn viterbi_segment<'a>(&self, normalized: &'a str) -> Vec<&'a str> {
        let n = normalized.len();
        let mut best_score = vec![f32::NEG_INFINITY; n + 1];
        let mut prev_pos = vec![0_usize; n + 1];
        best_score[0] = 0.0;

        for (start, ch) in normalized.char_indices() {
            let cur_score = best_score[start];
            if !cur_score.is_finite() {
                continue;
            }

            let mut matched_any = false;
            let max_end = n.min(start.saturating_add(self.max_piece_bytes));
            let slice = &normalized[start..max_end];
            for (rel_idx, c) in slice.char_indices() {
                let end = start + rel_idx + c.len_utf8();
                let candidate = &normalized[start..end];
                if let Some(&piece_score) = self.unigram_scores.get(candidate) {
                    matched_any = true;
                    let next_score = cur_score + piece_score;
                    if next_score > best_score[end] {
                        best_score[end] = next_score;
                        prev_pos[end] = start;
                    }
                }
            }

            if !matched_any {
                let end = start + ch.len_utf8();
                let fallback_score = cur_score + UNKNOWN_CHAR_PENALTY;
                if fallback_score > best_score[end] {
                    best_score[end] = fallback_score;
                    prev_pos[end] = start;
                }
            }
        }

        let mut pieces = Vec::new();
        let mut cursor = n;
        while cursor > 0 {
            let prev = prev_pos[cursor];
            if prev >= cursor {
                break;
            }
            pieces.push(&normalized[prev..cursor]);
            cursor = prev;
        }
        pieces.reverse();
        pieces
    }
}

/// Active ONNX encoder + decoder sessions for a single installed OPUS-MT pack.
#[cfg(not(target_os = "android"))]
pub(crate) struct OpusMtSession {
    encoder: ort::session::Session,
    decoder: ort::session::Session,
    tokenizer: OpusMtTokenizer,
}

#[cfg(not(target_os = "android"))]
impl std::fmt::Debug for OpusMtSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpusMtSession")
            .field("decoder_start_id", &self.tokenizer.decoder_start_id)
            .finish_non_exhaustive()
    }
}

#[cfg(not(target_os = "android"))]
impl OpusMtSession {
    /// Loads encoder & decoder ONNX sessions + tokenizer for `pack_id` under `storage_root`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if the pack files are missing or ONNX Runtime fails to initialize.
    pub(crate) fn load(
        env: &ort::environment::Environment,
        storage_root: &Path,
        pack_id: &str,
    ) -> Result<Self, AppError> {
        let pack_dir = storage_root.join("nmt").join(pack_id);
        let encoder_path = pack_dir.join("encoder_model_quantized.onnx");
        let decoder_path = pack_dir.join("decoder_model_quantized.onnx");
        if !encoder_path.is_file() || !decoder_path.is_file() {
            return Err(AppError::NotFound {
                message: format!("OPUS-MT pack '{pack_id}' is not installed"),
            });
        }

        let tokenizer = OpusMtTokenizer::load_from_dir(&pack_dir)?;
        let encoder = build_cpu_session(env, &encoder_path)?;
        let decoder = build_cpu_session(env, &decoder_path)?;
        Ok(Self {
            encoder,
            decoder,
            tokenizer,
        })
    }

    fn run_encoder(
        &mut self,
        src_ids: Vec<i64>,
        attn_mask: &[i64],
    ) -> Result<(usize, usize, Vec<f32>), AppError> {
        use ort::value::Tensor;

        let src_len = src_ids.len();
        let enc_ids_tensor =
            Tensor::from_array(([1_usize, src_len], src_ids)).map_err(|e| AppError::Internal {
                message: format!("failed to build OPUS-MT encoder input_ids tensor: {e}"),
            })?;
        let enc_mask_tensor = Tensor::from_array(([1_usize, src_len], attn_mask.to_vec()))
            .map_err(|e| AppError::Internal {
                message: format!("failed to build OPUS-MT encoder attention_mask tensor: {e}"),
            })?;

        let enc_outputs = self
            .encoder
            .run(ort::inputs![
                "input_ids" => enc_ids_tensor,
                "attention_mask" => enc_mask_tensor,
            ])
            .map_err(|e| AppError::Internal {
                message: format!("OPUS-MT encoder inference failed: {e}"),
            })?;

        let (hidden_shape, hidden_slice) =
            enc_outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| AppError::Internal {
                    message: format!("failed to extract OPUS-MT encoder hidden states: {e}"),
                })?;
        let h_dims: &[i64] = hidden_shape;
        if h_dims.len() < 3 {
            return Err(AppError::Internal {
                message: format!("unexpected OPUS-MT encoder output rank: {}", h_dims.len()),
            });
        }
        let enc_seq = usize::try_from(h_dims[1]).unwrap_or(src_len);
        let hidden_dim = usize::try_from(h_dims[2]).unwrap_or(512);
        Ok((enc_seq, hidden_dim, hidden_slice.to_vec()))
    }

    /// Translates a single utterance using greedy autoregressive ONNX decoding.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] if ONNX tensor creation or inference fails.
    pub(crate) fn translate(&mut self, text: &str) -> Result<String, AppError> {
        use ort::value::Tensor;

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(String::new());
        }

        let src_ids = self.tokenizer.encode(trimmed);
        let src_len = src_ids.len();
        let attn_mask = vec![1_i64; src_len];
        let (enc_seq, hidden_dim, hidden_data) = self.run_encoder(src_ids, &attn_mask)?;

        let max_steps = MAX_DECODE_TOKENS
            .min(src_len.saturating_mul(3).saturating_add(16))
            .min(self.tokenizer.max_length);

        let mut generated: Vec<i64> = Vec::with_capacity(max_steps.saturating_add(1));
        generated.push(self.tokenizer.decoder_start_id);

        let mut repeat_run = 0_usize;
        for _step in 0..max_steps {
            let dec_len = generated.len();
            let dec_ids_tensor = Tensor::from_array(([1_usize, dec_len], generated.clone()))
                .map_err(|e| AppError::Internal {
                    message: format!("failed to build OPUS-MT decoder input_ids tensor: {e}"),
                })?;
            let enc_hidden_tensor =
                Tensor::from_array(([1_usize, enc_seq, hidden_dim], hidden_data.clone())).map_err(
                    |e| AppError::Internal {
                        message: format!("failed to build OPUS-MT encoder_hidden_states: {e}"),
                    },
                )?;
            let enc_mask_tensor = Tensor::from_array(([1_usize, src_len], attn_mask.clone()))
                .map_err(|e| AppError::Internal {
                    message: format!("failed to build OPUS-MT encoder_attention_mask: {e}"),
                })?;

            let dec_outputs = self
                .decoder
                .run(ort::inputs![
                    "input_ids" => dec_ids_tensor,
                    "encoder_hidden_states" => enc_hidden_tensor,
                    "encoder_attention_mask" => enc_mask_tensor,
                ])
                .map_err(|e| AppError::Internal {
                    message: format!("OPUS-MT decoder inference failed: {e}"),
                })?;

            let (logit_shape, logits) =
                dec_outputs[0]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| AppError::Internal {
                        message: format!("failed to extract OPUS-MT decoder logits: {e}"),
                    })?;
            let Some(last_row) = extract_last_logit_row(logit_shape, logits) else {
                break;
            };
            let next_id =
                select_greedy_token(last_row, self.tokenizer.pad_id, self.tokenizer.eos_id);
            if next_id == self.tokenizer.eos_id {
                break;
            }
            if generated.last().copied() == Some(next_id) {
                repeat_run = repeat_run.saturating_add(1);
                if repeat_run >= 4 {
                    break;
                }
            } else {
                repeat_run = 0;
            }
            generated.push(next_id);

            // Prevent degenerate 2-gram or 3-gram loops on noisy OCR inputs.
            if has_tail_ngram_loop(&generated) {
                break;
            }
        }

        let decoded = self.tokenizer.decode(&generated);
        if decoded.is_empty() {
            Ok(trimmed.to_owned())
        } else {
            Ok(decoded)
        }
    }
}

#[cfg(not(target_os = "android"))]
fn build_cpu_session(
    env: &ort::environment::Environment,
    model_path: &Path,
) -> Result<ort::session::Session, AppError> {
    ort::session::Session::builder(env)
        .map_err(|e| AppError::Internal {
            message: format!("failed to build OPUS-MT ONNX session: {e}"),
        })?
        .commit_from_file(model_path)
        .map_err(|e| AppError::Internal {
            message: format!(
                "failed to load OPUS-MT model '{}': {e}",
                model_path.display()
            ),
        })
}

#[cfg(not(target_os = "android"))]
fn extract_last_logit_row<'a>(logit_shape: &[i64], logits: &'a [f32]) -> Option<&'a [f32]> {
    if logit_shape.len() < 3 {
        return None;
    }
    let seq_out = usize::try_from(logit_shape[1]).ok()?;
    let vocab_size = usize::try_from(logit_shape[2]).ok()?;
    if seq_out == 0 || vocab_size == 0 {
        return None;
    }
    let row_start = (seq_out - 1).saturating_mul(vocab_size);
    logits.get(row_start..row_start.saturating_add(vocab_size))
}

#[cfg(not(target_os = "android"))]
fn select_greedy_token(last_row: &[f32], pad_id: i64, eos_id: i64) -> i64 {
    let mut best_idx = 0_usize;
    let mut best_val = f32::NEG_INFINITY;
    for (idx, &val) in last_row.iter().enumerate() {
        if let Ok(tok_id) = i64::try_from(idx)
            && tok_id == pad_id
            && tok_id != eos_id
        {
            continue;
        }
        if val > best_val {
            best_val = val;
            best_idx = idx;
        }
    }
    i64::try_from(best_idx).unwrap_or(eos_id)
}

#[cfg(not(target_os = "android"))]
fn has_tail_ngram_loop(tokens: &[i64]) -> bool {
    for n in [2_usize, 3_usize] {
        let need = n.saturating_mul(3);
        if tokens.len() >= need {
            let tail = &tokens[tokens.len() - need..];
            if tail[..n] == tail[n..n * 2] && tail[n..n * 2] == tail[n * 2..] {
                return true;
            }
        }
    }
    false
}

/// Detects the dominant script/language ISO code of OCR text (`"ja"`, `"ko"`, `"zh"`, `"ru"`,
/// `"th"`, `"vi"`, or `"en"`).
pub(crate) fn detect_script_language(text: &str) -> &'static str {
    let mut kana = 0_usize;
    let mut hangul = 0_usize;
    let mut cjk = 0_usize;
    let mut cyrillic = 0_usize;
    let mut thai = 0_usize;
    let mut vietnamese = 0_usize;
    let mut latin = 0_usize;

    for ch in text.chars() {
        let code = u32::from(ch);
        if (0x3040..=0x30FF).contains(&code) || (0x31F0..=0x31FF).contains(&code) {
            kana += 1;
        } else if (0xAC00..=0xD7AF).contains(&code)
            || (0x1100..=0x11FF).contains(&code)
            || (0x3130..=0x318F).contains(&code)
        {
            hangul += 1;
        } else if (0x4E00..=0x9FFF).contains(&code) || (0x3400..=0x4DBF).contains(&code) {
            cjk += 1;
        } else if (0x0400..=0x04FF).contains(&code) {
            cyrillic += 1;
        } else if (0x0E00..=0x0E7F).contains(&code) {
            thai += 1;
        } else if (0x1EA0..=0x1EF9).contains(&code)
            || matches!(
                ch,
                'ă' | 'â'
                    | 'đ'
                    | 'ê'
                    | 'ô'
                    | 'ơ'
                    | 'ư'
                    | 'Ă'
                    | 'Â'
                    | 'Đ'
                    | 'Ê'
                    | 'Ô'
                    | 'Ơ'
                    | 'Ư'
            )
        {
            vietnamese += 1;
        } else if ch.is_ascii_alphabetic() {
            latin += 1;
        }
    }

    if kana > 0 {
        "ja"
    } else if hangul > 0 {
        "ko"
    } else if cjk > 0 {
        "zh"
    } else if cyrillic > 0 {
        "ru"
    } else if thai > 0 {
        "th"
    } else if vietnamese > 0 {
        "vi"
    } else if latin > 0 {
        "en"
    } else {
        "ja"
    }
}

/// Resolves the ordered sequence of offline Windows OPUS-MT pack IDs required to translate from
/// `source_lang` to `target_lang` (including automatic English pivot chains such as
/// `ja -> en -> vi`).
///
/// # Errors
///
/// Returns a human-readable explanation if the requested language pair is not supported by
/// offline Windows Lite OPUS-MT packs (suggesting Qwen3-VL-2B).
pub(crate) fn resolve_opus_pack_chain(
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<&'static str>, String> {
    let src = source_lang.trim().to_ascii_lowercase();
    let dst = target_lang.trim().to_ascii_lowercase();
    if src == dst {
        return Ok(Vec::new());
    }
    match (src.as_str(), dst.as_str()) {
        ("ja", "en") => Ok(vec!["ja-en"]),
        ("zh", "en") => Ok(vec!["zh-en"]),
        ("ko", "en") => Ok(vec!["ko-en"]),
        ("en", "vi") => Ok(vec!["en-vi"]),
        ("ja", "vi") => Ok(vec!["ja-en", "en-vi"]),
        ("zh", "vi") => Ok(vec!["zh-en", "en-vi"]),
        ("ko", "vi") => Ok(vec!["ko-en", "en-vi"]),
        _ => Err(format!(
            "Offline Lite pack for {src} -> {dst} is unavailable on desktop; switch to Qwen3-VL-2B for universal translation."
        )),
    }
}

/// Returns `true` when all verified files of `pack_id` exist on disk with exact expected lengths.
pub(crate) fn is_opus_pack_installed(storage_root: &Path, pack_id: &str) -> bool {
    opus_mt_pack_meta(pack_id).is_some_and(|meta| {
        meta.assets.iter().all(|spec| {
            std::fs::metadata(storage_root.join(spec.rel_path))
                .is_ok_and(|m| m.is_file() && m.len() == spec.expected_bytes)
        })
    })
}

/// Returns the current on-disk byte footprint for `pack_id`.
pub(crate) fn installed_opus_pack_bytes(storage_root: &Path, pack_id: &str) -> u64 {
    let Some(meta) = opus_mt_pack_meta(pack_id) else {
        return 0;
    };
    meta.assets.iter().fold(0_u64, |acc, spec| {
        let len = std::fs::metadata(storage_root.join(spec.rel_path)).map_or(0, |m| m.len());
        acc.saturating_add(len)
    })
}

/// Builds an [`AiNmtPackView`] snapshot for a single Windows OPUS-MT pack.
pub(crate) fn build_opus_pack_view(
    storage_root: &Path,
    pack_id: &str,
    downloading_pack_id: Option<&str>,
) -> Option<AiNmtPackView> {
    let meta = opus_mt_pack_meta(pack_id)?;
    let ready = is_opus_pack_installed(storage_root, pack_id);
    let downloading = downloading_pack_id == Some(pack_id);
    let status = if downloading {
        AiModelStatus::Downloading
    } else if ready {
        AiModelStatus::Ready
    } else {
        AiModelStatus::NotDownloaded
    };
    Some(AiNmtPackView {
        id: meta.id.to_owned(),
        display_name: meta.display_name.to_owned(),
        source_lang: meta.source_lang.to_owned(),
        target_lang: meta.target_lang.to_owned(),
        status,
        installed_bytes: installed_opus_pack_bytes(storage_root, pack_id),
        required_bytes: total_bytes(meta.assets),
        ready,
    })
}

/// Lists all Windows OPUS-MT offline translation packs and their installation states.
pub(crate) fn list_opus_nmt_packs(
    storage_root: &Path,
    downloading_pack_id: Option<&str>,
) -> Vec<AiNmtPackView> {
    ALL_OPUS_MT_PACKS
        .iter()
        .filter_map(|meta| build_opus_pack_view(storage_root, meta.id, downloading_pack_id))
        .collect()
}

fn normalize_sentencepiece_input(raw: &str) -> String {
    let collapsed = collapse_whitespace(raw);
    if collapsed.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(collapsed.len() + 4);
    out.push(SP_SPACE);
    for ch in collapsed.chars() {
        if ch == ' ' {
            out.push(SP_SPACE);
        } else {
            out.push(ch);
        }
    }
    out
}

fn collapse_whitespace(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_space = true;
    for ch in raw.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentencepiece_viterbi_tokenizer_encodes_and_decodes_cleanly() {
        let model = SpModelProto {
            pieces: vec![
                SpSentencePiece {
                    piece: "<unk>".to_owned(),
                    score: 0.0,
                    r#type: 2,
                },
                SpSentencePiece {
                    piece: "\u{2581}hello".to_owned(),
                    score: -1.2,
                    r#type: 1,
                },
                SpSentencePiece {
                    piece: "\u{2581}world".to_owned(),
                    score: -1.5,
                    r#type: 1,
                },
                SpSentencePiece {
                    piece: "!".to_owned(),
                    score: -2.0,
                    r#type: 1,
                },
                SpSentencePiece {
                    piece: "\u{2581}he".to_owned(),
                    score: -4.0,
                    r#type: 1,
                },
                SpSentencePiece {
                    piece: "llo".to_owned(),
                    score: -4.0,
                    r#type: 1,
                },
            ],
        };
        let spm_bytes = model.encode_to_vec();
        let vocab_json = r#"{"</s>":0,"<unk>":1,"\u2581hello":2,"\u2581world":3,"!":4,"<pad>":9}"#;
        let cfg_json =
            r#"{"decoder_start_token_id":9,"eos_token_id":0,"pad_token_id":9,"max_length":128}"#;

        let tok = OpusMtTokenizer::from_raw(&spm_bytes, vocab_json, cfg_json)
            .expect("tokenizer loads from synthetic SentencePiece proto");
        let encoded = tok.encode("hello   world!");
        assert_eq!(
            encoded,
            vec![2, 3, 4, 0],
            "Viterbi picks highest-scoring unigram pieces + EOS"
        );
        let decoded = tok.decode(&[9, 2, 3, 4, 0]);
        assert_eq!(
            decoded, "hello world!",
            "decoder strips pad/eos and restores spaces"
        );
    }

    #[test]
    fn script_detection_and_pivot_resolution_cover_manga_pairs() {
        assert_eq!(detect_script_language("こんにちは世界"), "ja", "kana -> ja");
        assert_eq!(detect_script_language("안녕하세요"), "ko", "hangul -> ko");
        assert_eq!(detect_script_language("你好世界"), "zh", "hanzi -> zh");
        assert_eq!(
            detect_script_language("Xin chào thế giới"),
            "vi",
            "vietnamese diacritics -> vi"
        );
        assert_eq!(detect_script_language("Hello there!"), "en", "latin -> en");

        assert_eq!(
            resolve_opus_pack_chain("ja", "vi").expect("ja->vi pivots through en"),
            vec!["ja-en", "en-vi"],
            "ja->vi chain"
        );
        assert_eq!(
            resolve_opus_pack_chain("zh", "vi").expect("zh->vi pivots through en"),
            vec!["zh-en", "en-vi"],
            "zh->vi chain"
        );
        assert_eq!(
            resolve_opus_pack_chain("ko", "en").expect("ko->en direct"),
            vec!["ko-en"],
            "ko->en chain"
        );
        assert!(
            resolve_opus_pack_chain("vi", "vi")
                .expect("same language needs no packs")
                .is_empty(),
            "same-language identity"
        );
        assert!(
            resolve_opus_pack_chain("ja", "ru").is_err(),
            "unsupported pair returns actionable error"
        );
    }
}
