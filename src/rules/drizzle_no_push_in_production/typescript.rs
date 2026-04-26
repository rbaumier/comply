//! drizzle-no-push-in-production — flag `drizzle-kit push` occurrences
//! inside string and template-string literals.
//!
//! `drizzle-kit push` applies schema changes directly without writing a
//! migration file — fine for prototyping, catastrophic in production
//! because there is no audit trail and diffs cannot be reviewed.
//!
//! Detection: walk every `string` / `template_string` node in the AST
//! and scan its content for the substring `drizzle-kit push`, accepting
//! common boundaries (end, `:`, whitespace, quote) to allow
//! dialect-suffixed variants (`drizzle-kit push:pg`) while rejecting
//! accidental substrings like `drizzle-kit pusher`.

use crate::diagnostic::{Diagnostic, Severity};

const NEEDLE: &str = "drizzle-kit push";

crate::ast_check! { on ["string", "template_string"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    let Some(pos) = find_push(text) else { return };

    let start = node.start_position();
    // Convert the byte offset inside the literal into a file position.
    let prefix = &text[..pos];
    let newlines = prefix.bytes().filter(|b| *b == b'\n').count();
    let line = start.row + newlines + 1;
    let column = if newlines == 0 {
        // Same line as the literal opener — offset from the literal start column.
        start.column + pos + 1
    } else {
        let last_nl = prefix.rfind('\n').unwrap();
        pos - last_nl
    };

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: "drizzle-no-push-in-production".into(),
        message: "`drizzle-kit push` bypasses migrations — use `drizzle-kit generate` \
                  + `drizzle-kit migrate` in CI and production deployments."
            .into(),
        severity: Severity::Error,
        span: None,
    });
}

/// Find `drizzle-kit push` with a valid trailing boundary inside `text`.
fn find_push(text: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find(NEEDLE) {
        let abs = search_from + rel;
        let after = abs + NEEDLE.len();
        let ok = match text.as_bytes().get(after) {
            None => true,
            Some(b) => matches!(*b, b':' | b' ' | b'\t' | b'\n' | b'"' | b'\'' | b'`'),
        };
        if ok {
            return Some(abs);
        }
        search_from = after;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_plain_push_in_string() {
        assert_eq!(run_on("const cmd = \"drizzle-kit push\";").len(), 1);
    }

    #[test]
    fn flags_dialect_suffixed_push_in_template() {
        assert_eq!(
            run_on("const cmd = `drizzle-kit push:pg --config=drizzle.config.ts`;").len(),
            1
        );
    }

    #[test]
    fn flags_push_in_object_literal() {
        assert_eq!(
            run_on("const scripts = { deploy: 'drizzle-kit push' };").len(),
            1
        );
    }

    #[test]
    fn allows_drizzle_kit_migrate() {
        assert!(run_on("const cmd = \"drizzle-kit migrate\";").is_empty());
    }

    #[test]
    fn allows_pusher_word() {
        // `drizzle-kit pusher` is not a command we care about.
        assert!(run_on("const cmd = \"drizzle-kit pusher\";").is_empty());
    }

    #[test]
    fn allows_push_outside_string() {
        // A bare identifier `push` in code shouldn't be flagged.
        assert!(run_on("queue.push(item);").is_empty());
    }
}
