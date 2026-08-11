//! Real llama.cpp inference backend, only compiled with `--features llama-real`.
//!
//! Loads a GGUF model once and runs greedy single-shot completions on
//! demand. Tokenization + sampling are kept simple — we are aiming for
//! correctness, not throughput. Vision (`mmproj`) is intentionally not
//! handled here in v1.

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend as InnerBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::data_array::LlamaTokenDataArray;

/// Holds the (one-time-loaded) llama.cpp backend + model. Cloning shares
/// the underlying model via `Arc`.
#[derive(Clone)]
pub struct LlamaBackend {
    backend: Arc<InnerBackend>,
    model: Arc<LlamaModel>,
}

impl LlamaBackend {
    pub fn load(model_path: &Path) -> Result<Arc<Self>> {
        let backend = InnerBackend::init().context("init llama backend")?;
        let params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, model_path, &params)
            .with_context(|| format!("loading {}", model_path.display()))?;

        Ok(Arc::new(Self {
            backend: Arc::new(backend),
            model: Arc::new(model),
        }))
    }

    pub fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        let n_ctx = NonZeroU32::new(8192).unwrap();
        let ctx_params = LlamaContextParams::default().with_n_ctx(Some(n_ctx));
        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .context("creating llama context")?;

        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .context("tokenizing prompt")?;

        let mut batch = LlamaBatch::new(512, 1);
        let last_index = (tokens.len() as i32) - 1;
        for (i, token) in tokens.iter().enumerate() {
            let is_last = i as i32 == last_index;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(|e| anyhow!("batch add: {e}"))?;
        }

        ctx.decode(&mut batch).context("initial decode")?;

        let mut sampler = LlamaSampler::greedy();
        let mut out = String::new();
        let mut n_cur = batch.n_tokens();

        for _ in 0..max_tokens {
            let candidates =
                LlamaTokenDataArray::from_iter(ctx.candidates_ith(batch.n_tokens() - 1), false);
            let new_token = sampler.sample(&candidates);

            if self.model.is_eog_token(new_token) {
                break;
            }

            let piece = self
                .model
                .token_to_str(new_token, Special::Tokenize)
                .unwrap_or_default();
            out.push_str(&piece);

            batch.clear();
            batch
                .add(new_token, n_cur, &[0], true)
                .map_err(|e| anyhow!("batch add: {e}"))?;
            n_cur += 1;

            ctx.decode(&mut batch).context("step decode")?;
        }

        Ok(out)
    }
}
