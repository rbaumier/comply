use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Decorator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Decorator(decorator) = node.kind() else {
            return;
        };
        if !is_nestjs_file(ctx.source) {
            return;
        }
        let is_global = match &decorator.expression {
            Expression::CallExpression(call) => match &call.callee {
                Expression::Identifier(id) => id.name.as_str() == "Global",
                _ => false,
            },
            Expression::Identifier(id) => id.name.as_str() == "Global",
            _ => false,
        };
        if !is_global {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, decorator.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "nestjs-no-global-module-abuse".into(),
            message: "`@Global()` modules hide dependencies — import the module explicitly where needed.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_global_module() {
        let src = "import { Global, Module } from '@nestjs/common';\n@Global() @Module({}) export class CommonModule {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_global_module() {
        let src = "import { Module } from '@nestjs/common';\n@Module({}) export class CommonModule {}";
        assert!(run(src).is_empty());
    }
}
