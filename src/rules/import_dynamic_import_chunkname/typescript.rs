//! import-dynamic-import-chunkname backend — enforce webpackChunkName on dynamic imports.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["import("] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("webpack") { return; }
    // Match `import(...)` expressions — tree-sitter parses these as `call_expression`
    // with callee kind `import`.
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "import" {
        return;
    }

    // Check the full text of the call expression for a webpackChunkName comment.
    // The comment `/* webpackChunkName: "foo" */` lives inside the arguments.
    let call_text = node.utf8_text(source).unwrap_or("");
    if call_text.contains("webpackChunkName") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "import-dynamic-import-chunkname".into(),
        message: "Dynamic imports require a leading comment with the webpack chunkname.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "webpack")
    }

    #[test]
    fn flags_missing_chunkname() {
        let d = run_on("const Foo = import('./foo');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("chunkname"));
    }

    #[test]
    fn allows_chunkname_comment() {
        let src = r#"const Foo = import(/* webpackChunkName: "foo" */ './foo');"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_wrong_comment() {
        let d = run_on("const Foo = import(/* some comment */ './foo');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn ignores_non_webpack_projects() {
        let d = crate::rules::test_helpers::run_ts("const Foo = import('./foo');", &Check);
        assert!(
            d.is_empty(),
            "webpack-only rule must be silent without webpack"
        );
    }
}
