//! switch-case-braces OXC backend — flag `case`/`default` clauses that declare
//! a block-scoped binding (`let`/`const`/`using`/`class`/`function`) in their
//! body without wrapping it in a block `{ }` — such a declaration leaks into
//! the enclosing `switch` scope.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Statement, VariableDeclarationKind};
use std::sync::Arc;

pub struct Check;

/// True iff a top-level statement of the case body declares a block-scoped
/// binding (`let`/`const`/`using`/`class`/`function`). `var` is excluded —
/// it is function-scoped, so braces do not change its scope. Declarations
/// nested inside an inner block do not leak and are not counted.
fn declares_block_scoped_binding(stmts: &[Statement]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Statement::VariableDeclaration(decl) => decl.kind != VariableDeclarationKind::Var,
        Statement::FunctionDeclaration(_) | Statement::ClassDeclaration(_) => true,
        _ => false,
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchCase]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SwitchCase(case) = node.kind() else {
            return;
        };

        let stmts = &case.consequent;

        // Fall-through case (no body)
        if stmts.is_empty() {
            return;
        }

        // Already wrapped in a single block statement
        if stmts.len() == 1 && matches!(stmts[0], Statement::BlockStatement(_)) {
            return;
        }

        // Braces only matter when a top-level lexical declaration would leak
        // into the enclosing switch scope. A pure control-flow body has
        // nothing to scope.
        if !declares_block_scoped_binding(stmts) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, case.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Missing braces in `case` clause \u{2014} wrap the body in `{ }` \
                      to avoid scope leaking."
                .into(),
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // Regression for #1019: the TypeScript compiler's watch-mode switch is
    // pure control flow (fall-through, `if`, `break`) — nothing to scope.
    #[test]
    fn allows_control_flow_only_switch_from_issue_1019() {
        let src = r#"
switch (eventName) {
    case "change":
    case "unlink":
    case "unlinkDir":
        break;
    case "add":
    case "addDir":
        if (stats && stats.mtimeMs <= last) { return; }
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_call_and_break_body() {
        let src = r#"
switch (x) {
    case 'a':
        foo();
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fallthrough_with_bare_break() {
        let src = r#"
switch (x) {
    case 'a':
    case 'b':
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_declaration_nested_in_inner_block() {
        let src = r#"
switch (x) {
    case 'a':
        if (c) { let z = 2; }
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_top_level_const_declaration() {
        let src = r#"
switch (x) {
    case 'a':
        const y = 1;
        break;
}
"#;
        let diagnostics = run_on(src);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule_id, "switch-case-braces");
    }

    #[test]
    fn flags_top_level_let_declaration() {
        let src = r#"
switch (x) {
    case 'a':
        let x = 1;
        break;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_braced_case_with_declaration() {
        let src = r#"
switch (x) {
    case 'a': {
        const y = 1;
        break;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_default_with_top_level_declaration() {
        let src = r#"
switch (x) {
    default:
        const z = 2;
        break;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_default_with_control_flow_only() {
        let src = r#"
switch (x) {
    default:
        doThing();
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
