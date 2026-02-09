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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormattingMode {
    Quick,    // Filler removal + basic punctuation (1 LLM call)
    Standard, // Quick + capitalization (1 LLM call)
    Smart,    // Standard + numbers/dates/etc (1 LLM call)
    Disabled, // No processing (passthrough)
}

impl FormattingMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "quick" => Self::Quick,
            "standard" => Self::Standard,
            "smart" => Self::Smart,
            "disabled" => Self::Disabled,
            _ => Self::Standard, // Default fallback
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    pub formatted_text: String,
    pub original_text: String,
    pub processing_time_ms: u64,
    pub mode_used: FormattingMode,
}

pub struct TextProcessor {
    llm_adapter: LlamaCppAdapter,
    mode: FormattingMode,
    min_words_for_processing: usize,
}

impl TextProcessor {
    pub async fn new(model_name: &str, mode: FormattingMode) -> Result<Self> {
        let mut adapter = LlamaCppAdapter::new();
        
        // Create config
        let config = LlmConfig {
            model_name: model_name.to_string(),
            model_path: None, // Will auto-resolve from cache
            temperature: 0.7,
            max_tokens: 512,
            top_p: 0.9,
            top_k: 40,
        };
        
        // Initialize the adapter
        adapter
            .initialize(config)
            .await
            .map_err(|e| ProcessorError::LlmError(e.to_string()))?;

        Ok(Self {
            llm_adapter:adapter,
            mode,
            min_words_for_processing: 3, // Skip LLM for very short text
        })
    }

    pub fn with_min_words(mut self, min_words: usize) -> Self {
        self.min_words_for_processing = min_words;
        self
    }

    pub async fn process(&self, raw_text: &str) -> Result<ProcessingResult> {
        let start = Instant::now();

        // Validate input
        let trimmed = raw_text.trim();
        if trimmed.is_empty() {
            return Err(ProcessorError::EmptyInput);
        }

        // Passthrough mode or very short text
        if self.mode == FormattingMode::Disabled
            || trimmed.split_whitespace().count() < self.min_words_for_processing
        {
            return Ok(ProcessingResult {
                formatted_text: trimmed.to_string(),
                original_text: raw_text.to_string(),
                processing_time_ms: start.elapsed().as_millis() as u64,
                mode_used: FormattingMode::Disabled,
            });
        }

        // Generate prompt based on mode
        let prompt = match self.mode {
            FormattingMode::Quick => prompts::quick_format_prompt(trimmed),
            FormattingMode::Standard => prompts::standard_format_prompt(trimmed),
            FormattingMode::Smart => prompts::smart_format_prompt(trimmed),
            FormattingMode::Disabled => unreachable!(), // Already handled above
        };

        // Run LLM inference
        let formatted = self
            .llm_adapter
            .run_prompt(prompt, 512) // Max 512 tokens output
            .await
            .map_err(|e| ProcessorError::LlmError(e.to_string()))?
            .trim()
            .to_string();

        // Fallback if LLM returns empty
        let final_text = if formatted.is_empty() {
            trimmed.to_string()
        } else {
            formatted
        };

        Ok(ProcessingResult {
            formatted_text: final_text,
            original_text: raw_text.to_string(),
            processing_time_ms: start.elapsed().as_millis() as u64,
            mode_used: self.mode,
        })
    }

    pub fn set_mode(&mut self, mode: FormattingMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> FormattingMode {
        self.mode
    }
}

#[cfg(test)]
mod tests;
