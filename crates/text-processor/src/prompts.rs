/// Prompt templates for text formatting

/// Quick mode: Filler removal + basic punctuation
pub fn quick_format_prompt(text: &str) -> String {
    format!(
        r#"Clean up this transcribed speech:
1. Remove filler words (um, uh, like, you know, etc.)
2. Add proper punctuation (periods, commas, question marks)
3. Keep the exact wording otherwise

Input: "{}"

Output (cleaned text only, no extra explanation):"#,
        text
    )
}

/// Standard mode: Quick + capitalization
pub fn standard_format_prompt(text: &str) -> String {
    format!(
        r#"Clean up this transcribed speech:
1. Remove filler words (um, uh, like, you know, etc.)
2. Add proper punctuation (periods, commas, question marks)
3. Fix capitalization (proper nouns, sentence starts, "I")
4. Keep the exact wording otherwise

Input: "{}"

Output (cleaned text only, no extra explanation):"#,
        text
    )
}

/// Smart mode: Standard + smart formatting
pub fn smart_format_prompt(text: &str) -> String {
    format!(
        r#"Clean up this transcribed speech intelligently:
1. Remove filler words (um, uh, like, you know, etc.)
2. Add proper punctuation (periods, commas, question marks)
3. Fix capitalization (proper nouns, sentence starts, "I")
4. Format numbers ("two hundred" → "200", "five PM" → "5 PM")
5. Format dates ("january first" → "January 1st")
6. Remove false starts and course corrections
7. Keep the exact wording otherwise

Input: "{}"

Output (cleaned text only, no extra explanation):"#,
        text
    )
}
