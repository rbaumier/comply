//! todo-needs-issue-link backend — scan comments for `TODO`/`FIXME`
//! without a linked issue.
//!
//! A reference counts as: a `#<number>`, a `GH-<number>`, a full URL
//! (`http://` / `https://`), or a `JIRA-XXX` style tag. Anything else is
//! considered an untracked reminder — those always rot into silent bugs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::issue_link::has_issue_reference;

#[derive(Debug)]
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

/// Find a tracked-debt marker inside a comment on the given line. The full
/// marker set covers every conventional rot signal — `TODO`/`FIXME` for
/// future work, plus `XXX`/`HACK`/`BUG` for "this is wrong, fix it later"
/// markers used in many older codebases. Without an issue link they all rot
/// the same way.
/// Returns `(marker, trailing_text)` or None if the line has no comment marker.
fn find_marker(line: &str) -> Option<(&'static str, &str)> {
    let comment_pos = line.find("//").or_else(|| line.find("/*"))?;
    let after_comment = &line[comment_pos..];
    for marker in ["TODO", "FIXME", "XXX", "HACK", "BUG"] {
        if let Some(pos) = after_comment.find(marker) {
            return Some((marker, &after_comment[pos + marker.len()..]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
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

    #[test]
    fn flags_xxx_marker() {
        assert_eq!(run("// XXX broken on safari").len(), 1);
    }

    #[test]
    fn flags_hack_marker() {
        assert_eq!(run("// HACK forced cast").len(), 1);
    }

    #[test]
    fn flags_bug_marker() {
        assert_eq!(run("// BUG drops the last item").len(), 1);
    }

    #[test]
    fn xxx_with_link_passes() {
        assert!(run("// XXX #42 known limitation").is_empty());
    }
}
