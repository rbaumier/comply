//! playwright-no-commented-out-tests — flag commented-out test blocks.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Patterns that indicate a commented-out test or describe block.
fn looks_like_test_comment(text: &str) -> bool {
    let trimmed = text.trim_start();
    // Match: test(, test.only(, it(, it.only(, describe(, describe.only(
    for kw in &["test", "it", "describe"] {
        if let Some(rest) = trimmed.strip_prefix(kw)
            && (rest.starts_with('(') || rest.starts_with('.') || rest.starts_with('['))
        {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !crate::rules::playwright::is_playwright_context(ctx) {
        return;
    }

    // Only look at comment nodes.
    let text = node.utf8_text(source).unwrap_or("");

    // Strip comment prefixes.
    let body = if let Some(rest) = text.strip_prefix("//") {
        rest
    } else if let Some(rest) = text.strip_prefix("/*").and_then(|r| r.strip_suffix("*/")) {
        rest
    } else {
        text
    };

    // Check each line of the comment.
    for line in body.lines() {
        if looks_like_test_comment(line) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "playwright-no-commented-out-tests".into(),
                message: "Some tests seem to be commented.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }

    #[test]
    fn flags_commented_test() {
        let d = run_ts("// test('should work', () => {});");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-commented-out-tests");
    }

    #[test]
    fn flags_commented_describe() {
        let d = run_ts("// describe('suite', () => {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_normal_comment() {
        let d = run_ts("// This is a normal comment");
        assert!(d.is_empty());
    }
}
