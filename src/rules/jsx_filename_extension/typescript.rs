//! jsx-filename-extension AST backend.
//!
//! JSX is only legitimate in `.jsx` and `.tsx` files. Finding JSX in a `.js`
//! or `.ts` file means the project's build/tooling convention has been
//! violated — tsc or bundlers may refuse to compile, and editor tooling
//! won't apply JSX-aware behavior.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let _ = source;
    let ext = ctx
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if ext != "js" && ext != "ts" {
        return;
    }

    let Some(first_jsx) = find_first_jsx(node) else { return };
    let pos = first_jsx.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "JSX found in `.{ext}` file — rename the file to `.{ext}x` or move the JSX out."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

fn find_first_jsx(root: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = root.walk();
    let mut progressed = cursor.goto_first_child();
    while progressed {
        let child = cursor.node();
        if matches!(child.kind(), "jsx_element" | "jsx_self_closing_element") {
            return Some(child);
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                progressed = false;
                break;
            }
            if cursor.node().id() == root.id() {
                progressed = false;
                break;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::CheckCtx;
    use std::path::Path;

    fn run_with_path(source: &str, fake_path: &str) -> Vec<Diagnostic> {
        use crate::rules::backend::AstCheck;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .expect("grammar should load");
        let tree = parser.parse(source, None).expect("parser should produce a tree");
        Check.check(&CheckCtx::for_test(Path::new(fake_path), source), &tree)
    }

    #[test]
    fn flags_jsx_in_js_file() {
        let src = "const x = <div />;";
        assert_eq!(run_with_path(src, "a.js").len(), 1);
    }

    #[test]
    fn flags_jsx_in_ts_file() {
        let src = "const x = <div>hi</div>;";
        assert_eq!(run_with_path(src, "a.ts").len(), 1);
    }

    #[test]
    fn allows_jsx_in_tsx_file() {
        let src = "const x = <div />;";
        assert!(run_with_path(src, "a.tsx").is_empty());
    }

    #[test]
    fn allows_jsx_in_jsx_file() {
        let src = "const x = <div />;";
        assert!(run_with_path(src, "a.jsx").is_empty());
    }

    #[test]
    fn allows_plain_ts_without_jsx() {
        let src = "const x = 1;";
        assert!(run_with_path(src, "a.ts").is_empty());
    }

    #[test]
    fn reports_only_first_jsx_occurrence() {
        let src = "const x = <div />; const y = <span />;";
        assert_eq!(run_with_path(src, "a.ts").len(), 1);
    }
}
