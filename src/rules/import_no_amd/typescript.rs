//! import-no-amd backend — forbid AMD require/define calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["require", "define"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }

    let name = callee.utf8_text(source).unwrap_or("");
    if name != "require" && name != "define" {
        return;
    }

    // AMD pattern: require([...], fn) or define([...], fn) — exactly 2 args, first is array.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let arg_nodes: Vec<_> = args.children(&mut cursor)
        .filter(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",")
        .collect();

    if arg_nodes.len() != 2 {
        return;
    }

    if arg_nodes[0].kind() != "array" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "import-no-amd".into(),
        message: format!("Expected imports instead of AMD `{name}()`."),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_amd_require() {
        let d = run_on("require(['dep'], function(dep) {});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("AMD"));
    }

    #[test]
    fn flags_amd_define() {
        let d = run_on("define(['dep'], function(dep) {});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("define"));
    }

    #[test]
    fn allows_normal_require() {
        assert!(run_on("const x = require('fs');").is_empty());
    }
}
