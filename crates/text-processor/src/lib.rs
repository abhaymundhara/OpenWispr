use llm::{
    adapters::llamacpp::LlamaCppAdapter, LlmAdapter, LlmConfig,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use thiserror::Error;

mod prompts;

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
        let start = Instant::now();

        let trimmed = raw_text.trim();
        if trimmed.is_empty() {
            return Err(ProcessorError::EmptyInput);
        }

        // Skip processing for very short text to avoid LLM "over-fixing" or hallucinations
        if trimmed.split_whitespace().count() < self.min_words_for_processing {
            return Ok(ProcessingResult {
                formatted_text: trimmed.to_string(),
                original_text: raw_text.to_string(),
                processing_time_ms: start.elapsed().as_millis() as u64,
            });
        }

        // Generate prompt based on mode
        let prompt = match mode {
            "rewrite" => prompts::rewrite_prompt(trimmed),
            "grammar" => prompts::grammar_prompt(trimmed),
            _ => prompts::core_format_prompt(trimmed, dictionary),
        };

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
