//! no-misleading-collection-name oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };
        let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &decl.id else {
            return;
        };
        let name = ident.name.as_str();
        let Some(claimed) = super::name_suffix_shape(name) else {
            return;
        };
        let Some(init) = &decl.init else {
            return;
        };
        let Some(actual) = initializer_shape_oxc(init) else {
            return;
        };
        if claimed == actual {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, ident.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "no-misleading-collection-name".into(),
            message: format!(
                "`{name}` is named like {ca} {cl} but holds {aa} {al}. \
                 Rename to match the actual type — the suffix is part of the contract.",
                ca = super::article(claimed.label()),
                cl = claimed.label(),
                aa = super::article(actual.label()),
                al = actual.label()
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn initializer_shape_oxc(expr: &Expression) -> Option<super::Shape> {
    match expr {
        Expression::ArrayExpression(_) => Some(super::Shape::Array),
        Expression::NewExpression(new_expr) => {
            let Expression::Identifier(ident) = &new_expr.callee else {
                return None;
            };
            match ident.name.as_str() {
                "Set" => Some(super::Shape::Set),
                "Map" => Some(super::Shape::Map),
                "Array" => Some(super::Shape::Array),
                _ => None,
            }
        }
        _ => None,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_array_holding_set() {
        let d = run("const userArray = new Set();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("an Array"));
    }

    #[test]
    fn flags_set_holding_array() {
        let d = run("const userSet = [];");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_map_holding_array() {
        let d = run("const userMap = [];");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("a Map"));
    }

    #[test]
    fn allows_matching_array() {
        assert!(run("const userArray = [];").is_empty());
    }

    #[test]
    fn allows_matching_set_set() {
        assert!(run("const userSet = new Set();").is_empty());
    }

    #[test]
    fn allows_matching_map_map() {
        assert!(run("const cacheMap = new Map();").is_empty());
    }

    #[test]
    fn ignores_unsuffixed_name() {
        assert!(run("const cache = new Set();").is_empty());
    }

    // `List` is a general collection term, not an Array contract:
    // `allowList`/`denyList`/`blockList` backed by a Set are not misleading.
    #[test]
    fn allows_list_holding_set() {
        assert!(run("const allowList = new Set(['a', 'b']);").is_empty());
        assert!(run("const denyList = new Set(['a', 'b']);").is_empty());
        assert!(run("const blockList = new Set(['a', 'b']);").is_empty());
    }

    #[test]
    fn allows_list_holding_array_or_map() {
        assert!(run("const userList = [];").is_empty());
        assert!(run("const userList = new Map();").is_empty());
    }

    // Mid-word fragments must not read as a type token (issue #3953):
    // `listOffset` ends in `set`, `bitmap` ends in `map`, but neither claims a shape.
    #[test]
    fn allows_offset_holding_array() {
        assert!(run("const listOffset = [];").is_empty());
    }

    #[test]
    fn allows_bitmap_holding_array() {
        assert!(run("const bitmap = [];").is_empty());
    }

    // A genuine trailing `Set` token still flags an Array mismatch.
    #[test]
    fn flags_set_token_holding_array() {
        let d = run("const userSet: number[] = [];");
        assert_eq!(d.len(), 1);
    }
}
