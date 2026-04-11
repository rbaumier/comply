//! ts-ban-ts-comment backend — flag `@ts-ignore`, `@ts-nocheck`, and bare
//! `@ts-expect-error` (without description) in comments.
//!
//! Default behaviour (matches the recommended config):
//! - `@ts-ignore` → always flagged (prefer `@ts-expect-error`)
//! - `@ts-nocheck` → always flagged
//! - `@ts-expect-error` → allowed only with a description (>= 3 chars)
//! - `@ts-check` → allowed

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "comment" {
        return;
    }
    let text = match std::str::from_utf8(&source[node.byte_range()]) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Match @ts-ignore, @ts-nocheck, @ts-expect-error, @ts-check
    // Works for both // and /* */ comments.
    let stripped = text.trim_start_matches('/').trim_start_matches('*').trim();

    if let Some(rest) = stripped.strip_prefix("@ts-ignore") {
        let _ = rest;
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-ban-ts-comment".into(),
            message: "Use `@ts-expect-error` instead of `@ts-ignore`, as `@ts-ignore` will do nothing if the following line is error-free.".into(),
            severity: Severity::Warning,
        });
    } else if let Some(rest) = stripped.strip_prefix("@ts-nocheck") {
        let _ = rest;
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-ban-ts-comment".into(),
            message: "Do not use `@ts-nocheck` because it alters compilation errors.".into(),
            severity: Severity::Warning,
        });
    } else if let Some(rest) = stripped.strip_prefix("@ts-expect-error") {
        let description = rest.trim();
        if description.is_empty() || description.len() < 3 {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-ban-ts-comment".into(),
                message: "Include a description after `@ts-expect-error` to explain why it is necessary (at least 3 characters).".into(),
                severity: Severity::Warning,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_ts_ignore() {
        let diags = run_on("// @ts-ignore\nconst x = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("@ts-expect-error"));
    }

    #[test]
    fn flags_ts_nocheck() {
        let diags = run_on("// @ts-nocheck\nconst x = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("@ts-nocheck"));
    }

    #[test]
    fn flags_bare_ts_expect_error() {
        let diags = run_on("// @ts-expect-error\nconst x = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("description"));
    }

    #[test]
    fn allows_ts_expect_error_with_description() {
        let diags = run_on("// @ts-expect-error legacy API returns wrong type\nconst x = 1;");
        assert!(diags.is_empty());
    }
}
