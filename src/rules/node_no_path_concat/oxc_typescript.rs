//! node-no-path-concat oxc backend — flag `__dirname + '...'` / `__filename + '...'`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

const PATH_GLOBALS: &[&str] = &["__dirname", "__filename"];

fn is_path_global(expr: &Expression) -> bool {
    if let Expression::Identifier(id) = expr {
        PATH_GLOBALS.contains(&id.name.as_str())
    } else {
        false
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["__dirname", "__filename"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };
        if bin.operator != BinaryOperator::Addition {
            return;
        }

        let left_is_path = is_path_global(&bin.left);
        let right_is_path = is_path_global(&bin.right);
        if !left_is_path && !right_is_path {
            return;
        }

        // Avoid double-reporting: if parent is also a `+` binary expression
        // whose left side is a path global, skip this node.
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::BinaryExpression(parent_bin) = parent.kind()
            && parent_bin.operator == BinaryOperator::Addition && is_path_global(&parent_bin.left) {
                return;
            }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `path.join()` or `path.resolve()` instead of string concatenation with `__dirname`/`__filename`.".into(),
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
    fn flags_dirname_plus_string() {
        let d = run_on(r#"const p = __dirname + '/foo';"#);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_filename_plus_string() {
        assert_eq!(run_on(r#"const p = __filename + '/bar';"#).len(), 1);
    }


    #[test]
    fn flags_string_plus_dirname() {
        assert_eq!(run_on(r#"const p = '/prefix' + __dirname;"#).len(), 1);
    }


    #[test]
    fn allows_path_join() {
        assert!(run_on("const p = path.join(__dirname, 'foo');").is_empty());
    }


    #[test]
    fn allows_normal_concat() {
        assert!(run_on("const p = a + b;").is_empty());
    }
}
