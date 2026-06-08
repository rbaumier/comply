use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn keyword_for<'a>(stmt: &Statement<'a>) -> &'static str {
    match stmt {
        Statement::BreakStatement(_) => "break",
        Statement::ReturnStatement(_) => "return",
        Statement::ContinueStatement(_) => "continue",
        Statement::ThrowStatement(_) => "throw",
        _ => "unknown",
    }
}

fn is_terminator(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::BreakStatement(_)
            | Statement::ReturnStatement(_)
            | Statement::ContinueStatement(_)
            | Statement::ThrowStatement(_)
    )
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
        let AstKind::SwitchCase(case) = node.kind() else { return };

        let body = &case.consequent;
        // Need at least 2 statements: a block + a terminator after it.
        if body.len() < 2 {
            return;
        }

        let last = &body[body.len() - 1];
        if !is_terminator(last) {
            return;
        }

        // Everything before the terminator should be exactly one block statement.
        let before_terminator = &body[..body.len() - 1];
        if before_terminator.len() != 1 {
            return;
        }
        if !matches!(&before_terminator[0], Statement::BlockStatement(_)) {
            return;
        }

        let keyword = keyword_for(last);
        let span = last.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Move `{keyword}` inside the block statement."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_break_outside_block() {
        let src = r#"
switch (x) {
    case 'a': {
        doStuff();
    }
    break;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "switch-case-break-position");
        assert!(d[0].message.contains("break"));
    }


    #[test]
    fn flags_return_outside_block() {
        let src = r#"
function f(x: string) {
    switch (x) {
        case 'a': {
            doStuff();
        }
        return 1;
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return"));
    }


    #[test]
    fn allows_break_inside_block() {
        let src = r#"
switch (x) {
    case 'a': {
        doStuff();
        break;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_case_without_block() {
        let src = r#"
switch (x) {
    case 'a':
        doStuff();
        break;
}
"#;
        // No block statement, so rule doesn't apply (break is not "after a block")
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_fallthrough_case() {
        let src = r#"
switch (x) {
    case 'a':
    case 'b': {
        doStuff();
        break;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
