use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

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
        let AstKind::SwitchCase(case) = node.kind() else { return };

        for stmt in &case.consequent {
            match stmt {
                Statement::VariableDeclaration(decl)
                    if decl.kind.is_lexical() =>
                {
                    let span = decl.span;
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "block-scope-case".into(),
                        message: "Lexical declaration in `case` clause leaks into sibling cases — wrap the body in `{ ... }`.".into(),
                        severity: Severity::Warning,
                        span: Some((span.start as usize, (span.end - span.start) as usize)),
                    });
                    return;
                }
                Statement::ClassDeclaration(_) => {
                    let span = stmt.span();
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "block-scope-case".into(),
                        message: "Lexical declaration in `case` clause leaks into sibling cases — wrap the body in `{ ... }`.".into(),
                        severity: Severity::Warning,
                        span: Some((span.start as usize, (span.end - span.start) as usize)),
                    });
                    return;
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_const_in_case_without_block() {
        let src = r#"switch (x) {
    case 1:
        const y = 2;
        break;
    case 2:
        break;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_let_in_case_without_block() {
        let src = r#"switch (x) {
    case 1:
        let y = 2;
        break;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_class_decl_in_case() {
        let src = r#"switch (x) {
    case 1:
        class Foo {}
        break;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_case_with_block() {
        let src = r#"switch (x) {
    case 1: {
        const y = 2;
        break;
    }
    case 2:
        break;
}"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_case_without_declaration() {
        let src = r#"switch (x) {
    case 1:
        doSomething();
        break;
}"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_default_with_block() {
        let src = r#"switch (x) {
    case 1:
        break;
    default: {
        const y = 2;
        break;
    }
}"#;
        assert!(run_on(src).is_empty());
    }
}
