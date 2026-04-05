//! Local LLM engine — loads Qwen3.5-0.8B via llama.cpp (GGUF).
//!
//! Uses the `llama-cpp-2` crate which wraps llama.cpp's optimized C++
//! inference engine with fused kernels for the gated delta net architecture.
//!
//! GPU acceleration is automatic:
//! - macOS: Metal (enabled at compile time via target-specific dep)
//! - Linux/Windows: CUDA (opt-in via `--features cuda`)
//! - Fallback: CPU with optimized BLAS
//!
//! Model weights (~530 MB Q8_0 GGUF) download from HuggingFace on first
//! run and are cached by hf-hub.

use anyhow::{anyhow, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use std::num::NonZeroU32;
use tracing::info;

const GGUF_REPO: &str = "unsloth/Qwen3.5-0.8B-GGUF";
const GGUF_FILE: &str = "Qwen3.5-0.8B-Q8_0.gguf";

/// Holds the llama.cpp backend and model for repeated inference.
/// A new context is created per inference call (fresh KV cache).
pub struct LlmEngine {
    backend: LlamaBackend,
    model: LlamaModel,
}

impl LlmEngine {
    /// Download (or load from cache) the GGUF model and initialize llama.cpp.
    pub fn load() -> Result<Self> {
        info!("initializing llama.cpp backend...");
        let backend = LlamaBackend::init()
            .map_err(|e| anyhow!("llama backend init: {e}"))?;

        info!("downloading GGUF model from {GGUF_REPO}/{GGUF_FILE}...");
        let api = hf_hub::api::sync::Api::new()?;
        let repo = api.model(GGUF_REPO.to_string());
        let model_path = repo.get(GGUF_FILE)?;

        info!("loading model (offloading all layers to GPU)...");
        let model_params = LlamaModelParams::default()
            .with_n_gpu_layers(1000);
        let model_params = std::pin::pin!(model_params);

        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
            .map_err(|e| anyhow!("failed to load model: {e}"))?;

        info!("model loaded successfully");
        Ok(Self { backend, model })
    }

    /// Human-readable device name for diagnostics.
    pub fn device_name(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "Metal (auto)"
        } else if cfg!(feature = "cuda") {
            "CUDA"
        } else {
            "CPU"
        }
    }

    /// Run one inference pass.  Creates a fresh context (KV cache) each
    /// time so calls are independent.
    pub fn infer(&mut self, prompt: &str, max_tokens: usize) -> Result<String> {
        // Qwen3.5 uses chain-of-thought by default.  We pre-fill the
        // assistant turn with an empty thinking block so the model skips
        // reasoning and starts its visible response immediately.
        let chat_prompt = format!(
            "<|im_start|>system\n\
             You are a tiny desktop creature. Casual, opinionated, brief. One sentence max.\
             <|im_end|>\n\
             <|im_start|>user\n{prompt}<|im_end|>\n\
             <|im_start|>assistant\n\
             <think>\n</think>\n"
        );

        let tokens = self.model
            .str_to_token(&chat_prompt, AddBos::Always)
            .map_err(|e| anyhow!("tokenize: {e}"))?;

        info!("prompt tokens: {}, starting inference...", tokens.len());

        // Fresh context per call — independent KV cache each time
        let ctx_size = NonZeroU32::new(2048).unwrap();
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(ctx_size));
        let mut ctx = self.model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| anyhow!("create context: {e}"))?;

        // Feed prompt tokens in one batch
        let mut batch = LlamaBatch::new(512, 1);
        let last_idx = (tokens.len() - 1) as i32;
        for (i, &token) in tokens.iter().enumerate() {
            batch.add(token, i as i32, &[0], i as i32 == last_idx)?;
        }

        let t_start = std::time::Instant::now();
        ctx.decode(&mut batch)
            .map_err(|e| anyhow!("decode prompt: {e}"))?;
        info!("prompt decode: {:.2}s", t_start.elapsed().as_secs_f32());

        // Sampler chain: temperature → top-k → token selection
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(1.0),
            LlamaSampler::top_k(50),
            LlamaSampler::dist(rand::random()),
        ]);

        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();
        let mut gen_count = 0_usize;

        // Autoregressive loop — generate one token at a time.
        // Stop early once we have a complete sentence to keep output tight.
        const MIN_TOKENS_BEFORE_STOP: usize = 5;

        for _ in 0..max_tokens {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            let piece = self.model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(|e| anyhow!("decode token: {e}"))?;
            output.push_str(&piece);
            gen_count += 1;

            // Early stop on sentence-ending punctuation
            if gen_count >= MIN_TOKENS_BEFORE_STOP {
                let trimmed = output.trim();
                if trimmed.ends_with('.')
                    || trimmed.ends_with('!')
                    || trimmed.ends_with('?')
                {
                    break;
                }
            }

            batch.clear();
            batch.add(token, n_cur, &[0], true)?;
            n_cur += 1;

            ctx.decode(&mut batch)
                .map_err(|e| anyhow!("decode step: {e}"))?;
        }

        info!("generated {} tokens in {:.2}s", gen_count, t_start.elapsed().as_secs_f32());
        Ok(extract_after_thinking(&output))
    }
}

/// Clean up model output: strip any `<think>...</think>` blocks,
/// remove stray `</think>` tags, and trim.
fn extract_after_thinking(text: &str) -> String {
    let mut s = text.to_string();
    while let Some(start) = s.find("<think>") {
        if let Some(end) = s[start..].find("</think>") {
            s = format!("{}{}", &s[..start], &s[start + end + 8..]);
        } else {
            s = s[..start].to_string();
            break;
        }
    }
    s = s.replace("</think>", "");
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_clean_text() {
        assert_eq!(extract_after_thinking("Hello world!"), "Hello world!");
    }

    #[test]
    fn test_extract_with_think_block() {
        let input = "before <think>reasoning</think> after";
        assert_eq!(extract_after_thinking(input), "before  after");
    }

    #[test]
    fn test_extract_stray_close_tags() {
        let input = "response</think></think></think>";
        assert_eq!(extract_after_thinking(input), "response");
    }

    #[test]
    fn test_extract_unclosed_think() {
        let input = "before <think>unclosed";
        assert_eq!(extract_after_thinking(input), "before");
    }
}
