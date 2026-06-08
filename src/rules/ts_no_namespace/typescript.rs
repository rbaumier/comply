//! ts-no-namespace backend — flag `module` (namespace) declaration nodes,
//! excluding `declare namespace` (ambient declarations).
//!
//! Detection: walk `module` nodes (tree-sitter maps TS `namespace` to
//! `module` kind) and skip those that are ambient declarations.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["internal_module"] prefilter = ["namespace"] => |node, source, ctx, diagnostics|
    // tree-sitter-typescript parses `namespace Foo {}` as an
    // `internal_module` node (not `module`).
    // Check if this is a `declare namespace` — allowed.
    // Walk up to see if parent is `ambient_declaration`.
    if let Some(parent) = node.parent()
        && parent.kind() == "ambient_declaration" {
            return;
        }

    // Get the namespace name for better reporting
    let Some(_name_node) = node.child_by_field_name("name") else {
        return;
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-namespace".into(),
        message: "TypeScript `namespace` is a legacy construct — \
                  use ES module `export` / `import` instead."
            .into(),
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
    fn flags_namespace() {
        let diags = run_on("namespace Foo { export const x = 1; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_export_namespace() {
        let diags = run_on("export namespace Foo { }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_declare_namespace() {
        assert!(run_on("declare namespace NodeJS { }").is_empty());
    }

    #[test]
    fn allows_regular_code() {
        assert!(run_on("const x = 1;").is_empty());
    }
}
