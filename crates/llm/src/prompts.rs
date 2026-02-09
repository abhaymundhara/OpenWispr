/// Prompt templates for text formatting tasks

/// Remove filler words and clean up transcription
pub fn create_filler_removal_prompt(text: &str) -> String {
    format!(
        "Remove filler words (um, uh, like, you know, etc.) from the following text. Only return the cleaned text, nothing else:\n\n{}",
        text
    )
}

/// Add punctuation to text
pub fn create_punctuation_prompt(text: &str) -> String {
    format!(
        "Add proper punctuation (periods, commas, question marks, etc.) to the following text. Only return the punctuated text, nothing else:\n\n{}",
        text
    )
}

/// Fix capitalization
pub fn create_capitalization_prompt(text: &str) -> String {
    format!(
        "Fix capitalization in the following text. Capitalize proper nouns, sentence starts, and the word 'I'. Only return the corrected text, nothing else:\n\n{}",
        text
    )
}

/// Handle course correction (remove backtracking)
pub fn create_course_correction_prompt(text: &str) -> String {
    format!(
        "The following is dictated text where the speaker may have corrected themselves. Remove any false starts or corrections, keeping only the final intended text. Only return the final text, nothing else:\n\n{}",
        text
    )
}

/// Combined smart formatting prompt (all-in-one)
pub fn create_smart_format_prompt(text: &str) -> String {
    format!(
        r#"Clean up the following dictated text by:
1. Removing filler words (um, uh, like, you know)
2. Adding proper punctuation
3. Fixing capitalization
4. Removing false starts and course corrections

Only return the cleaned text, nothing else:

{}"#,
        text
    )
}
