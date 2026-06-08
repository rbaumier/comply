//! ts-no-non-null-asserted-optional-chain backend — flag `non_null_expression`
//! wrapping a node that contains an optional chain (`?.`).
//!
//! The `!` contradicts the `?.` — one says "definitely not null" while the
//! other says "might be null".
//!
//! Tree-sitter structure for member access: `a?.b`
//!   member_expression > optional_chain > `?.`
//! Tree-sitter structure for optional call: `a?.()`
//!   call_expression > `?.` (direct child)

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node or any of its descendants contains an optional chain (`?.`).
fn contains_optional_chain(node: tree_sitter::Node) -> bool {
    if node.kind() == "optional_chain" || node.kind() == "?." {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_optional_chain(child) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["non_null_expression"] => |node, _source, ctx, diagnostics|
    let Some(inner) = node.named_child(0) else {
        return;
    };
    if !contains_optional_chain(inner) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-non-null-asserted-optional-chain".into(),
        message: "Non-null assertion `!` after optional chain `?.` is unsafe — \
                  the chain can return `undefined` by design."
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
    fn flags_optional_member_with_non_null() {
        let diags = run_on("const x = (a?.b)!;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_optional_call_with_non_null() {
        let diags = run_on("const x = (a?.())!;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_non_null_without_optional_chain() {
        assert!(run_on("const x = a.b!;").is_empty());
    }

    #[test]
    fn allows_optional_chain_without_non_null() {
        assert!(run_on("const x = a?.b;").is_empty());
    }
}
