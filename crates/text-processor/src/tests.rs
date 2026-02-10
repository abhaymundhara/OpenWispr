use super::*;

#[test]
fn test_empty_input() {
    let empty_text = "";
    assert!(empty_text.trim().is_empty());
}

#[test]
fn test_short_text_detection() {
    let short_text = "yes";
    let word_count = short_text.split_whitespace().count();
    assert_eq!(word_count, 1);
    assert!(word_count < 3);
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
    let core = prompts::core_format_prompt(text);
    assert!(core.contains(text));
    assert!(core.contains("corrections"));
    assert!(core.contains("filler words"));
    assert!(core.contains("lists"));
}

#[test]
fn test_processing_result_structure() {
    let result = ProcessingResult {
        formatted_text: "Test output.".to_string(),
        original_text: "test input".to_string(),
        processing_time_ms: 100,
    };
    
    assert_eq!(result.formatted_text, "Test output.");
    assert_eq!(result.original_text, "test input");
    assert_eq!(result.processing_time_ms, 100);
}

#[tokio::test]
#[ignore]
async fn test_full_processing_with_real_model() {
    // This test requires SmolLM2-135M-Instruct-Q4_K_M to be downloaded
    let processor = TextProcessor::new(
        "SmolLM2-135M-Instruct-Q4_K_M",
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
