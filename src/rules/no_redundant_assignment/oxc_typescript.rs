use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentTarget, BindingPattern, Expression, Statement, VariableDeclarationKind,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

struct AssignInfo<'a> {
    name: &'a str,
    is_const: bool,
    start: u32,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let stmts: Option<&oxc_allocator::Vec<'a, Statement<'a>>> = match node.kind() {
                AstKind::Program(prog) => Some(&prog.body),
                AstKind::BlockStatement(block) => Some(&block.body),
                AstKind::FunctionBody(body) => Some(&body.statements),
                _ => None,
            };
            let Some(stmts) = stmts else { continue };
            check_consecutive_assignments(stmts, ctx, &mut diagnostics);
        }
        diagnostics
    }
}

fn check_consecutive_assignments(
    stmts: &[Statement],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let infos: Vec<Option<AssignInfo>> = stmts.iter().map(|s| extract_assign(s)).collect();
    for pair in infos.windows(2) {
        let (Some(a), Some(b)) = (&pair[0], &pair[1]) else {
            continue;
        };
        if a.is_const || a.name != b.name {
            continue;
        }
        let (line_a, col_a) = byte_offset_to_line_col(ctx.source, a.start as usize);
        let (line_b, _) = byte_offset_to_line_col(ctx.source, b.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: line_a,
            column: col_a,
            rule_id: super::META.id.into(),
            message: format!(
                "Variable `{}` is assigned on line {} then immediately overwritten on line {}.",
                a.name, line_a, line_b,
            ),
            severity: super::META.severity,
            span: None,
        });
    }
}

fn extract_assign<'a>(stmt: &'a Statement<'a>) -> Option<AssignInfo<'a>> {
    match stmt {
        Statement::VariableDeclaration(decl) => {
            if decl.declarations.len() != 1 {
                return None;
            }
            let declarator = &decl.declarations[0];
            let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                return None;
            };
            // Must have an initializer.
            declarator.init.as_ref()?;
            let is_const = decl.kind == VariableDeclarationKind::Const;
            Some(AssignInfo {
                name: id.name.as_str(),
                is_const,
                start: stmt.span().start,
            })
        }
        Statement::ExpressionStatement(expr_stmt) => {
            let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
                return None;
            };
            let AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left else {
                return None;
            };
            Some(AssignInfo {
                name: id.name.as_str(),
                is_const: false,
                start: stmt.span().start,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_immediate_overwrite() {
        let d = run_on("let x = 1;\nx = 2;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn flags_reassignment_pair() {
        let d = run_on("x = foo();\nx = bar();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_different_variables() {
        assert!(run_on("let x = 1;\nlet y = 2;").is_empty());
    }

    #[test]
    fn allows_used_between() {
        assert!(run_on("let x = 1;\nconsole.log(x);\nx = 2;").is_empty());
    }
}
