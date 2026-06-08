//! ts-ban-tslint-comment backend — flag any `tslint:enable` or `tslint:disable`
//! comment directives. TSLint is deprecated; these comments are dead weight.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] prefilter = ["tslint"] => |node, source, ctx, diagnostics|
    let text = match std::str::from_utf8(&source[node.byte_range()]) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Strip leading // or /* and whitespace.
    let stripped = text.trim_start_matches('/').trim_start_matches('*').trim();

    // tslint:(enable|disable)(-line|-next-line)?
    if stripped.starts_with("tslint:enable") || stripped.starts_with("tslint:disable") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-ban-tslint-comment".into(),
            message: format!("TSLint comment detected: `{}`.", text.trim()),
            severity: Severity::Warning,
            span: None,
        });
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_tslint_disable() {
        let diags = run_on("// tslint:disable\nconst x = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("tslint"));
    }

    #[test]
    fn flags_tslint_enable() {
        let diags = run_on("// tslint:enable\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_tslint_disable_next_line() {
        let diags = run_on("// tslint:disable-next-line: no-any\nconst x: any = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_normal_comments() {
        let diags = run_on("// This uses tslint-style formatting\nconst x = 1;");
        assert!(diags.is_empty());
    }
}
