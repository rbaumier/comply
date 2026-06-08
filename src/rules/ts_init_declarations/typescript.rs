//! ts-init-declarations backend — flag `let`/`var` declarations without
//! an initializer, skipping `declare` and ambient contexts.
//!
//! Detection: walk `variable_declarator` nodes that lack a `value` field,
//! inside `let` or `var` declarations (not `const`, which requires init).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    // Already has an initializer — fine.
    if node.child_by_field_name("value").is_some() {
        return;
    }

    // Parent must be a lexical_declaration (let/const) or variable_declaration (var).
    let Some(parent) = node.parent() else { return };
    let pk = parent.kind();
    if pk != "lexical_declaration" && pk != "variable_declaration" {
        return;
    }

    // Skip `const` — TS/JS already errors on uninitialized const.
    // We only care about `let`/`var`.
    let decl_text = match std::str::from_utf8(&source[parent.byte_range()]) {
        Ok(t) => t,
        Err(_) => return,
    };
    if decl_text.starts_with("const ") {
        return;
    }

    // Skip `declare` contexts: walk up to see if any ancestor is
    // `ambient_declaration`.
    let mut ancestor = parent.parent();
    while let Some(a) = ancestor {
        if a.kind() == "ambient_declaration" {
            return;
        }
        ancestor = a.parent();
    }

    // Has a type annotation? That's the TS-specific case where init
    // might be intentionally deferred. Still flag it — the rule's
    // purpose is to encourage initialization.
    let name = node.child_by_field_name("name")
        .and_then(|n| std::str::from_utf8(&source[n.byte_range()]).ok())
        .unwrap_or("variable");

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-init-declarations".into(),
        message: format!(
            "`{name}` is declared without initialization — \
             assign a value at declaration."
        ),
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
    fn flags_let_without_init() {
        let diags = run_on("let x: number;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x"));
    }

    #[test]
    fn allows_let_with_init() {
        assert!(run_on("let x: number = 0;").is_empty());
    }

    #[test]
    fn allows_declare_context() {
        assert!(run_on("declare let x: number;").is_empty());
    }
}
