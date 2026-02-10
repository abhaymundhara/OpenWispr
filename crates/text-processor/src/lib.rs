use llm::{
    adapters::llamacpp::LlamaCppAdapter, LlmAdapter, LlmConfig,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use thiserror::Error;

mod prompts;

fn should_skip_llm_processing(trimmed: &str, min_words_for_processing: usize) -> bool {
    trimmed.split_whitespace().count() < min_words_for_processing
}

fn build_prompt(
    mode: &str,
    text: &str,
    dictionary: &[String],
    clipboard_context: Option<&str>,
) -> String {
    match mode {
        "rewrite" => prompts::rewrite_prompt(text, clipboard_context),
        "grammar" => prompts::grammar_prompt(text),
        _ => prompts::core_format_prompt(text, dictionary),
    }
}

#[derive(Debug, Error)]
pub enum ProcessorError {
    #[error("LLM error: {0}")]
    LlmError(String),

    #[error("Empty input text")]
    EmptyInput,

    #[error("Processing timeout")]
    Timeout,
}

pub type Result<T> = std::result::Result<T, ProcessorError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    pub formatted_text: String,
    pub original_text: String,
    pub processing_time_ms: u64,
}

pub struct TextProcessor {
    llm_adapter: LlamaCppAdapter,
    min_words_for_processing: usize,
}

impl TextProcessor {
    pub async fn new(model_name: &str) -> Result<Self> {
        let mut adapter = LlamaCppAdapter::new();
        
        let config = LlmConfig {
            model_name: model_name.to_string(),
            model_path: None, 
            temperature: 0.1, // Lower temperature for more consistent formatting
            max_tokens: 512,
            top_p: 0.9,
            top_k: 40,
        };
        
        adapter
            .initialize(config)
            .await
            .map_err(|e| ProcessorError::LlmError(e.to_string()))?;

        Ok(Self {
            llm_adapter: adapter,
            min_words_for_processing: 3,
        })
    }

    pub fn with_min_words(mut self, min_words: usize) -> Self {
        self.min_words_for_processing = min_words;
        self
    }

    pub async fn process(&self, raw_text: &str, dictionary: &[String], mode: &str) -> Result<ProcessingResult> {
        self.process_with_context(raw_text, dictionary, mode, None).await
    }

    pub async fn process_with_context(
        &self,
        raw_text: &str,
        dictionary: &[String],
        mode: &str,
        clipboard_context: Option<&str>,
    ) -> Result<ProcessingResult> {
        let start = Instant::now();

        let trimmed = raw_text.trim();
        if trimmed.is_empty() {
            return Err(ProcessorError::EmptyInput);
        }

        // Skip processing for very short text to avoid LLM "over-fixing" or hallucinations
        if should_skip_llm_processing(trimmed, self.min_words_for_processing) {
            return Ok(ProcessingResult {
                formatted_text: trimmed.to_string(),
                original_text: raw_text.to_string(),
                processing_time_ms: start.elapsed().as_millis() as u64,
            });
        }

        let prompt = build_prompt(mode, trimmed, dictionary, clipboard_context);

        // Run LLM inference
        let formatted = self
            .llm_adapter
            .run_prompt(prompt, 512)
            .await
            .map_err(|e| ProcessorError::LlmError(e.to_string()))?
            .trim()
            .to_string();

        let final_text = if formatted.is_empty() {
            trimmed.to_string()
        } else {
            formatted
        };

        Ok(ProcessingResult {
            formatted_text: final_text,
            original_text: raw_text.to_string(),
            processing_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests;
