use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_nestjs_file(source: &str) -> bool {
    source.contains("@nestjs/")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@nestjs/"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_nestjs_file(ctx.source) {
            return;
        }
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "forwardRef" {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "nestjs-no-circular-injection".into(),
            message: "`forwardRef()` indicates a circular dependency — refactor to break the cycle."
                .into(),
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
    fn flags_forward_ref() {
        let src = "import { forwardRef } from '@nestjs/common';\nconst x = forwardRef(() => Foo);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_nestjs() {
        let src = "function forwardRef(f: any) { return f(); }\nconst x = forwardRef(() => 1);";
        assert!(run(src).is_empty());
    }
}
