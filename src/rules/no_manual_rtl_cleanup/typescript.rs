//! no-manual-rtl-cleanup backend — detect manual `cleanup` imports from
//! `@testing-library` in test files.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] prefilter = ["@testing-library"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    if !text.contains("@testing-library") {
        return;
    }
    // Walk import specifiers looking for `cleanup`
    if !has_cleanup_specifier(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-manual-rtl-cleanup".into(),
        message: "Manual `cleanup` import from `@testing-library` — \
                  Vitest runs cleanup automatically after each test."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

fn has_cleanup_specifier(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if find_cleanup(child, source) {
            return true;
        }
    }
    false
}

fn find_cleanup(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "import_specifier" {
        let Some(name_node) = node.child_by_field_name("name") else {
            return false;
        };
        if let Ok(name) = name_node.utf8_text(source)
            && name == "cleanup"
        {
            return true;
        }
        return false;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if find_cleanup(child, source) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        // Use a test-file path so the check doesn't skip
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        let ctx = crate::rules::backend::CheckCtx::for_test(
            std::path::Path::new("src/App.test.tsx"),
            source,
        );
        use crate::rules::backend::AstCheck;
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_cleanup_import() {
        let d = run_on("import { cleanup } from '@testing-library/react';");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-manual-rtl-cleanup");
    }

    #[test]
    fn flags_cleanup_among_other_imports() {
        let d = run_on("import { render, cleanup } from '@testing-library/react';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_render_only() {
        assert!(run_on("import { render } from '@testing-library/react';").is_empty());
    }
}
