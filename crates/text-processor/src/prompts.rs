pub fn core_format_prompt(text: &str, dictionary_words: &[String]) -> String {
    let mut instructions = String::from(
        r#"Convert this speech transcript into professional text:
- Handle corrections (e.g., if the user says "actually", "wait", "no", only output the intended meaning)
- Remove filler words (um, uh, like, you know)
- Auto-format lists: If the text starts with "one" or contains a list pattern, format it as a clean numbered or bulleted list. Maintain list continuity if it feels like part of a series.
- Add proper punctuation and capitalization
- Format numbers, dates and units correctly"#,
    );

    if !dictionary_words.is_empty() {
        instructions.push_str("\n- The following specialized jargon or names might be present; use these spellings: ");
        instructions.push_str(&dictionary_words.join(", "));
    }

    format!(
        r#"{instructions}

Input: "{text}"

Output (Transcribed text only):"#
    )
}
pub fn rewrite_prompt(text: &str) -> String {
    format!(
        r#"Rewrite the following text to be professional, clear, and concise. 
Maintain the original meaning but improve the flow and vocabulary.

Input: "{text}"

Output (Rewritten text only):"#
    )
}

pub fn grammar_prompt(text: &str) -> String {
    format!(
        r#"Fix the grammar, spelling, and punctuation of the following text. 
Do NOT rewrite it or change the style unless absolutely necessary for grammatical correctness.

Input: "{text}"

Output (Corrected text only):"#
    )
}
