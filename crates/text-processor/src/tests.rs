use super::*;

// Mock LLM adapter for testing
struct MockLlmAdapter {
    response: String,
}

impl MockLlmAdapter {
    fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
        }
    }
}

impl LlmAdapter for MockLlmAdapter {
    fn load_model(&mut self, _model_path: &str) -> llm::Result<()> {
        Ok(())
    }

    fn generate_response(&self, _prompt: &str, _max_tokens: u32) -> llm::Result<String> {
        Ok(self.response.clone())
    }
}

#[test]
fn test_passthrough_disabled_mode() {
    let adapter = Box::new(MockLlmAdapter::new("should not be used"));
    let processor = TextProcessor::new(adapter, FormattingMode::Disabled);

    let result = processor.process("um this is a test").unwrap();

    assert_eq!(result.formatted_text, "um this is a test");
    assert_eq!(result.mode_used, FormattingMode::Disabled);
}

#[test]
fn test_passthrough_short_text() {
    let adapter = Box::new(MockLlmAdapter::new("should not be used"));
    let processor = TextProcessor::new(adapter, FormattingMode::Standard);

    let result = processor.process("yes").unwrap();

    assert_eq!(result.formatted_text, "yes");
    assert_eq!(result.mode_used, FormattingMode::Disabled); // Skipped due to length
}

#[test]
fn test_filler_removal() {
    let adapter = Box::new(MockLlmAdapter::new("I need to schedule a meeting tomorrow."));
    let processor = TextProcessor::new(adapter, FormattingMode::Quick);

    let result = processor
        .process("um I need to uh schedule a meeting like tomorrow")
        .unwrap();

    assert_eq!(result.formatted_text, "I need to schedule a meeting tomorrow.");
    assert_eq!(result.mode_used, FormattingMode::Quick);
    assert!(result.processing_time_ms > 0);
}

#[test]
fn test_punctuation() {
    let adapter = Box::new(MockLlmAdapter::new("Where is the nearest coffee shop?"));
    let processor = TextProcessor::new(adapter, FormattingMode::Standard);

    let result = processor
        .process("where is the nearest coffee shop")
        .unwrap();

    assert_eq!(result.formatted_text, "Where is the nearest coffee shop?");
    assert!(result.formatted_text.ends_with('?'));
}

#[test]
fn test_empty_input() {
    let adapter = Box::new(MockLlmAdapter::new(""));
    let processor = TextProcessor::new(adapter, FormattingMode::Standard);

    let result = processor.process("");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ProcessorError::EmptyInput));
}

#[test]
fn test_empty_llm_response_fallback() {
    let adapter = Box::new(MockLlmAdapter::new("")); // LLM returns empty
    let processor = TextProcessor::new(adapter, FormattingMode::Standard);

    let result = processor.process("this is a test input").unwrap();

    // Should fallback to original trimmed text
    assert_eq!(result.formatted_text, "this is a test input");
}

#[test]
fn test_whitespace_trimming() {
    let adapter = Box::new(MockLlmAdapter::new("Test output."));
    let processor = TextProcessor::new(adapter, FormattingMode::Quick);

    let result = processor.process("  \n\t this is a test  \n  ").unwrap();

    assert_eq!(result.original_text, "  \n\t this is a test  \n  ");
    assert_eq!(result.formatted_text, "Test output."); // LLM response
}

#[test]
fn test_mode_switching() {
    let adapter = Box::new(MockLlmAdapter::new("Formatted."));
    let mut processor = TextProcessor::new(adapter, FormattingMode::Quick);

    assert_eq!(processor.mode(), FormattingMode::Quick);

    processor.set_mode(FormattingMode::Smart);
    assert_eq!(processor.mode(), FormattingMode::Smart);
}

#[test]
fn test_custom_min_words() {
    let adapter = Box::new(MockLlmAdapter::new("should not be used"));
    let processor = TextProcessor::new(adapter, FormattingMode::Standard)
        .with_min_words(5); // Require 5 words minimum

    // 4 words - should skip LLM
    let result = processor.process("this is a test").unwrap();
    assert_eq!(result.mode_used, FormattingMode::Disabled);
}
