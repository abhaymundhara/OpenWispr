use super::*;

#[test]
fn whitespace_trimming_logic() {
    let text_with_whitespace = "  \n\t this is a test  \n  ";
    let trimmed = text_with_whitespace.trim();
    assert_eq!(trimmed, "this is a test");
}

#[test]
fn should_skip_processing_for_short_text() {
    assert!(should_skip_llm_processing("yes", 3));
    assert!(should_skip_llm_processing("one two", 3));
    assert!(!should_skip_llm_processing("one two three", 3));
}

#[test]
fn core_prompt_includes_dictionary_and_instructions() {
    let text = "um I need to uh schedule a meeting";
    let dictionary = vec!["OpenWispr".to_string(), "Kubernetes".to_string()];
    let core = prompts::core_format_prompt(text, &dictionary);
    assert!(core.contains(text));
    assert!(core.contains("filler words"));
    assert!(core.contains("OpenWispr"));
    assert!(core.contains("Kubernetes"));
}

#[test]
fn rewrite_prompt_includes_clipboard_context_when_present() {
    let prompt = prompts::rewrite_prompt(
        "make this concise",
        Some("Draft email thread about Q1 roadmap"),
    );
    assert!(prompt.contains("Draft email thread about Q1 roadmap"));
    assert!(prompt.contains("Use this context only"));
}

#[test]
fn rewrite_prompt_skips_context_when_absent() {
    let prompt = prompts::rewrite_prompt("make this concise", None);
    assert!(!prompt.contains("Additional context"));
}

#[test]
fn build_prompt_switches_by_mode() {
    let text = "this is the input";
    let dictionary = vec!["OpenWispr".to_string()];

    let smart_prompt = build_prompt("smart", text, &dictionary, None);
    assert!(smart_prompt.contains("Convert this speech transcript"));

    let rewrite_prompt = build_prompt("rewrite", text, &dictionary, Some("ctx"));
    assert!(rewrite_prompt.contains("professional, clear, and concise"));
    assert!(rewrite_prompt.contains("ctx"));

    let grammar_prompt = build_prompt("grammar", text, &dictionary, Some("ctx"));
    assert!(grammar_prompt.contains("Fix the grammar"));
    assert!(!grammar_prompt.contains("Additional context"));
}

#[test]
fn processing_result_structure() {
    let result = ProcessingResult {
        formatted_text: "Test output.".to_string(),
        original_text: "test input".to_string(),
        processing_time_ms: 100,
    };

    assert_eq!(result.formatted_text, "Test output.");
    assert_eq!(result.original_text, "test input");
    assert_eq!(result.processing_time_ms, 100);
}
