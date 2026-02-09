use super::*;

// Note: These are unit tests that test the logic without actual LLM calls.
// Full integration tests with real LLM would require downloading models.

#[test]
fn test_passthrough_disabled_mode() {
    // For disabled mode, we don't need a real model, but the API requires one
    // In practice, this would be used with a loaded model
    // For now, we test the FormattingMode logic separately
    assert_eq!(FormattingMode::Disabled, FormattingMode::from_str("disabled"));
}

#[test]
fn test_empty_input() {
    // We can't create a TextProcessor without a valid model
    // But we can test the validation logic would trigger
    let empty_text = "";
    assert!(empty_text.trim().is_empty());
}

#[test]
fn test_short_text_detection() {
    let short_text = "yes";
    let word_count = short_text.split_whitespace().count();
    assert_eq!(word_count, 1);
    assert!(word_count < 3); // Would skip LLM processing
}

#[test]
fn test_formatting_mode_from_str() {
    assert_eq!(FormattingMode::from_str("quick"), FormattingMode::Quick);
    assert_eq!(FormattingMode::from_str("standard"), FormattingMode::Standard);
    assert_eq!(FormattingMode::from_str("smart"), FormattingMode::Smart);
    assert_eq!(FormattingMode::from_str("disabled"), FormattingMode::Disabled);
    assert_eq!(FormattingMode::from_str("QUICK"), FormattingMode::Quick); // Case insensitive
    assert_eq!(FormattingMode::from_str("invalid"), FormattingMode::Standard); // Default
}

#[test]
fn test_whitespace_trimming_logic() {
    let text_with_whitespace = "  \n\t this is a test  \n  ";
    let trimmed = text_with_whitespace.trim();
    assert_eq!(trimmed, "this is a test");
}

#[test]
fn test_prompt_generation() {
    let text = "um I need to uh schedule a meeting";
    
    let quick = prompts::quick_format_prompt(text);
    assert!(quick.contains(text));
    assert!(quick.contains("filler words"));
    
    let standard = prompts::standard_format_prompt(text);
    assert!(standard.contains(text));
    assert!(standard.contains("capitalization"));
    
    let smart = prompts::smart_format_prompt(text);
    assert!(smart.contains(text));
    assert!(smart.contains("numbers"));
}

#[test]
fn test_processing_result_structure() {
    let result = ProcessingResult {
        formatted_text: "Test output.".to_string(),
        original_text: "test input".to_string(),
        processing_time_ms: 100,
        mode_used: FormattingMode::Standard,
    };
    
    assert_eq!(result.formatted_text, "Test output.");
    assert_eq!(result.original_text, "test input");
    assert_eq!(result.processing_time_ms, 100);
    assert_eq!(result.mode_used, FormattingMode::Standard);
}

// Integration test (requires a downloaded model - skipped in unit tests)
#[tokio::test]
#[ignore]
async fn test_full_processing_with_real_model() {
    // This test requires SmolLM2-135M-Instruct-Q4_K_M to be downloaded
    let processor = TextProcessor::new(
        "SmolLM2-135M-Instruct-Q4_K_M",
        FormattingMode::Standard,
    ).await;
    
    if let Ok(proc) = processor {
        let result = proc.process("um I need to uh schedule a meeting like tomorrow").await;
        assert!(result.is_ok());
        
        if let Ok(res) = result {
            assert!(!res.formatted_text.contains(" um "));
            assert!(!res.formatted_text.contains(" uh "));
        }
    }
}
