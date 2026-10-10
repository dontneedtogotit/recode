const TURN_SUMMARY_MAX_WORDS: usize = 8;
const TURN_SUMMARY_MAX_CHARS: usize = 72;
const ELLIPSIS: &str = "…";

const EMPTY_PHRASES: &[&str] = &[
    "ok",
    "done",
    "success",
    "succeeded",
    "complete",
    "completed",
    "finished",
    "responded",
    "response",
    "result",
    "no changes",
    "n/a",
    "null",
    "undefined",
];

fn empty_phrases() -> std::collections::HashSet<String> {
    EMPTY_PHRASES
        .iter()
        .map(|value| value.to_lowercase())
        .collect()
}

fn strip_wrapping_quotes(value: &str) -> &str {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() >= 2 {
        let first = chars[0];
        let last = chars[chars.len() - 1];
        if first == last && (first == '"' || first == '\'' || first == '`') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn truncate_trailing_separators(text: &str) -> &str {
    let chars: Vec<char> = text.chars().collect();
    let mut end = chars.len();
    while end > 0 {
        let c = chars[end - 1];
        if c == ' ' || c == ',' || c == ';' || c == ':' || c == '·' || c == '-' {
            end -= 1;
        } else {
            break;
        }
    }
    &text[..end]
}

/// Normalizes a candidate summary into the phrase the rail renders.
///
/// Returns `None` for anything unusable — a non-string, an empty string,
/// or a phrase that carries no information. Callers should treat `None` as
/// "this agent has no summary", not as an error.
pub fn normalize_turn_summary(raw: Option<&str>) -> Option<String> {
    let text = match raw {
        Some(value) => value.trim(),
        None => return None,
    };

    let text = strip_wrapping_quotes(text);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text.trim();

    if text.is_empty() {
        return None;
    }

    if empty_phrases().contains(text.to_lowercase().as_str()) {
        return None;
    }

    let mut text = text.to_string();

    // Trim trailing sentence punctuation.
    while text.ends_with('.') || text.ends_with('!') || text.ends_with('…') {
        text.pop();
        text = text.trim().to_string();
    }

    let words: Vec<&str> = text.split(' ').collect();
    let mut truncated = false;

    if words.len() > TURN_SUMMARY_MAX_WORDS {
        text = words[..TURN_SUMMARY_MAX_WORDS].join(" ");
        truncated = true;
    }

    if text.chars().count() > TURN_SUMMARY_MAX_CHARS {
        let chars: Vec<char> = text.chars().collect();
        text = chars[..TURN_SUMMARY_MAX_CHARS].iter().collect();
        truncated = true;
    }

    text = truncate_trailing_separators(text.trim()).to_string();

    if text.is_empty() {
        return None;
    }

    if truncated {
        Some(format!("{text}{ELLIPSIS}"))
    } else {
        Some(text)
    }
}

/// Builds the summary for a turn that failed.
///
/// Model and provider errors often arrive as "Prefix: real cause" — the cause
/// is the informative half.
pub fn failure_turn_summary(error: Option<&str>) -> Option<String> {
    let message = match error {
        Some(value) => value.trim(),
        None => return None,
    };

    let cause = if let Some(index) = message.find(": ") {
        message[index + 2..].trim()
    } else {
        message
    };

    normalize_turn_summary(Some(cause)).or_else(|| normalize_turn_summary(Some(message)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_string_input() {
        assert!(normalize_turn_summary(None).is_none());
    }

    #[test]
    fn collapses_whitespace_and_trims() {
        let summary = normalize_turn_summary(Some("  hello   world  "));
        assert_eq!(summary, Some("hello world".into()));
    }

    #[test]
    fn strips_wrapping_quotes() {
        let summary = normalize_turn_summary(Some("'hello world'"));
        assert_eq!(summary, Some("hello world".into()));
    }

    #[test]
    fn rejects_empty_phrases() {
        assert_eq!(normalize_turn_summary(Some("done")), None);
        assert_eq!(normalize_turn_summary(Some("OK")), None);
        assert_eq!(normalize_turn_summary(Some("null")), None);
    }

    #[test]
    fn truncates_long_summaries() {
        let long_summary = "a".repeat(100);
        let summary = normalize_turn_summary(Some(long_summary.as_str()));
        assert!(summary.is_some());
        assert!(summary.unwrap().len() <= TURN_SUMMARY_MAX_CHARS + ELLIPSIS.len());
    }

    #[test]
    fn appends_ellipsis_when_truncated() {
        let summary = normalize_turn_summary(Some("one two three four five six seven eight nine"));
        assert!(summary.unwrap().ends_with(ELLIPSIS));
    }

    #[test]
    fn strips_trailing_punctuation() {
        let summary = normalize_turn_summary(Some("fixed lint error."));
        assert_eq!(summary, Some("fixed lint error".into()));
    }

    #[test]
    fn extracts_cause_from_prefixed_errors() {
        let summary = failure_turn_summary(Some("RateLimitError: too many requests"));
        assert_eq!(summary, Some("too many requests".into()));
    }

    #[test]
    fn falls_back_to_full_message_when_no_colon() {
        let summary = failure_turn_summary(Some("connection reset"));
        assert_eq!(summary, Some("connection reset".into()));
    }
}
