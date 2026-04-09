//! todo-needs-issue-link backend — scan comments for `TODO`/`FIXME`
//! without a linked issue.
//!
//! A reference counts as: a `#<number>`, a `GH-<number>`, a full URL
//! (`http://` / `https://`), or a `JIRA-XXX` style tag. Anything else is
//! considered an untracked reminder — those always rot into silent bugs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let Some((marker, rest)) = find_marker(line) else {
                continue;
            };
            if has_issue_reference(rest) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "todo-needs-issue-link".into(),
                message: format!(
                    "{marker} without an issue link — add `#123`, `GH-123`, a \
                     ticket key, or a full URL so this reminder can't rot silently."
                ),
                severity: Severity::Warning,
            });
        }
        diagnostics
    }
}

/// Find a `TODO` or `FIXME` marker inside a comment on the given line.
/// Returns `(marker, trailing_text)` or None if the line has no comment marker.
fn find_marker(line: &str) -> Option<(&'static str, &str)> {
    let comment_pos = line.find("//").or_else(|| line.find("/*"))?;
    let after_comment = &line[comment_pos..];
    for marker in ["TODO", "FIXME"] {
        if let Some(pos) = after_comment.find(marker) {
            return Some((marker, &after_comment[pos + marker.len()..]));
        }
    }
    None
}

/// True if the trailing text contains an issue reference of any accepted form.
fn has_issue_reference(text: &str) -> bool {
    if text.contains("http://") || text.contains("https://") {
        return true;
    }
    // `#<digit>` anywhere in the trailing text.
    if text.bytes().enumerate().any(|(i, b)| {
        b == b'#'
            && text
                .as_bytes()
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_digit())
    }) {
        return true;
    }
    // JIRA-style `ABC-123` or `GH-123`.
    has_ticket_key(text)
}

/// Detect `ABC-123` / `GH-45` patterns — uppercase prefix, dash, digits.
fn has_ticket_key(text: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx {
            path: Path::new("t.ts"),
            source,
        })
    }

    #[test]
    fn flags_bare_todo() {
        assert_eq!(run("// TODO fix this later").len(), 1);
    }

    #[test]
    fn flags_bare_fixme() {
        assert_eq!(run("// FIXME broken").len(), 1);
    }

    #[test]
    fn accepts_issue_hash_reference() {
        assert!(run("// TODO #123 fix later").is_empty());
    }

    #[test]
    fn accepts_full_url_reference() {
        assert!(run("// TODO https://github.com/org/repo/issues/42").is_empty());
    }

    #[test]
    fn accepts_ticket_key() {
        assert!(run("// TODO ABC-123 waiting on spec").is_empty());
        assert!(run("// TODO GH-7 pending review").is_empty());
    }

    #[test]
    fn ignores_todo_in_code_not_comment() {
        assert!(run("const TODO = 1;").is_empty());
    }
}
