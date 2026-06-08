use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::FunctionType;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Function(func) = node.kind() else {
            return;
        };
        if func.r#type != FunctionType::FunctionDeclaration {
            return;
        }
        // Walk ancestors to check if inside a control-flow block.
        if !is_inside_control_flow(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, func.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function declaration inside a control-flow block — move it to the top level or use a function expression.".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

fn is_inside_control_flow(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::IfStatement(_)
            | AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::SwitchCase(_) => return true,
            AstKind::Program(_) => return false,
            _ => {}
        }
    }
    false
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

    #[test]
    fn flags_function_in_if_block() {
        let src = "if (true) {\n  function foo() {}\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_function_in_for_block() {
        let src = "for (let i = 0; i < 10; i++) {\n  function bar() { return i; }\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_top_level_function() {
        let src = "function baz() {\n  return 1;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_in_block() {
        let src = "if (true) {\n  const fn = () => {};\n}";
        assert!(run_on(src).is_empty());
    }
}
