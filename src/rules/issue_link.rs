//! Shared "issue reference detection" helpers.
//!
//! Several rules need to know whether a piece of text already references
//! a tracked issue: `todo-needs-issue-link` and `no-skipped-test-without-link`
//! both gate their diagnostics on this check, and previously each rule
//! carried its own copy of the matcher. This module is the single source
//! of truth — both rules import from here.
//!
//! Accepted reference shapes:
//! - Hash + digits: `#42`, `#1234`
//! - GitHub flavoured: `GH-42`
//! - JIRA-style ticket key: `ABC-123` (uppercase prefix, dash, digits)
//! - Full URL: `http://...` or `https://...`

/// True if `text` contains any kind of issue reference. Used by the
/// `todo-needs-issue-link` and `no-skipped-test-without-link` rules to
/// decide whether a TODO marker / skipped test is "tracked enough". // comply-ignore: todo-needs-issue-link — mention, not marker.
pub fn has_issue_reference(text: &str) -> bool {
    if text.contains("http://") || text.contains("https://") {
        return true;
    }
    if has_hash_number(text) {
        return true;
    }
    has_ticket_key(text)
}

/// Detect `ABC-123` / `GH-45` patterns — uppercase prefix, dash, digits.
/// The prefix must be at least one uppercase letter, followed by a `-`,
/// followed by at least one digit. We don't require the prefix to start
/// at a word boundary because comments often embed the key inline
/// (`see ABC-123 for context`).
pub fn has_ticket_key(text: &str) -> bool {
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if !bytes[i].is_ascii_uppercase() {
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && bytes[j].is_ascii_uppercase() {
            j += 1;
        }
        if j == i + 1 || j >= bytes.len() || bytes[j] != b'-' {
            continue;
        }
        let mut k = j + 1;
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k > j + 1 {
            return true;
        }
    }
    false
}

/// True if `text` contains a `#<digit>` reference anywhere.
fn has_hash_number(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().any(|(i, &b)| {
        b == b'#'
            && bytes
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_hash_number() {
        assert!(has_issue_reference("see #42"));
        assert!(has_issue_reference("#1"));
    }

    #[test]
    fn detects_url() {
        assert!(has_issue_reference("https://github.com/foo/bar/issues/42"));
        assert!(has_issue_reference("http://example.com"));
    }

    #[test]
    fn detects_jira_key() {
        assert!(has_issue_reference("see ABC-123 for context"));
        assert!(has_issue_reference("ABC-1"));
    }

    #[test]
    fn detects_gh_key() {
        assert!(has_issue_reference("GH-7"));
    }

    #[test]
    fn rejects_plain_text() {
        assert!(!has_issue_reference("just some text"));
        assert!(!has_issue_reference("#"));
        assert!(!has_issue_reference("ABC"));
        assert!(!has_issue_reference("ABC-"));
    }
}
