//! no-set-x-to-y OxcCheck backend — flag function names like
//! `setStatusToClosed`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["setTo", "ToActive", "ToAdmin", "ToClosed"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::MethodDefinition,
            AstType::VariableDeclarator,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (name, offset) = match node.kind() {
            oxc_ast::AstKind::Function(f) => {
                let Some(ref id) = f.id else { return };
                (id.name.as_str(), id.span.start)
            }
            oxc_ast::AstKind::MethodDefinition(m) => {
                let name = match &m.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    _ => return,
                };
                (name, m.key.span().start)
            }
            oxc_ast::AstKind::VariableDeclarator(decl) => {
                // Only if bound to arrow_function or function_expression
                let is_fn = decl.init.as_ref().is_some_and(|e| {
                    matches!(
                        e,
                        Expression::ArrowFunctionExpression(_)
                            | Expression::FunctionExpression(_)
                    )
                });
                if !is_fn {
                    return;
                }
                let BindingPattern::BindingIdentifier(ref id) = decl.id else {
                    return;
                };
                (id.name.as_str(), decl.id.span().start)
            }
            _ => return,
        };

        if !matches_set_x_to_y(name) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Function name `{name}` encodes implementation (set X to Y), not intent. \
                 Rename to describe what the operation accomplishes from the caller's \
                 perspective — `setStatusToClosed` → `closeAccount`."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True if `name` matches `set<X>To<Y>` where X and Y start uppercase.
fn matches_set_x_to_y(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 8 {
        return false;
    }
    if &bytes[..3] != b"set" {
        return false;
    }
    if !bytes[3].is_ascii_uppercase() {
        return false;
    }
    let mut i = 4;
    while i + 2 < bytes.len() {
        if bytes[i] == b'T'
            && bytes[i + 1] == b'o'
            && bytes[i - 1].is_ascii_lowercase()
            && bytes[i + 2].is_ascii_uppercase()
        {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_set_status_to_closed() {
        let diags = run_on("function setStatusToClosed() {}");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-set-x-to-y");
    }

    #[test]
    fn flags_method_definition() {
        let diags = run_on("class A { setRoleToAdmin() {} }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_arrow_function_const() {
        let diags = run_on("const setUserToActive = () => {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_get_user() {
        assert!(run_on("function getUser() {}").is_empty());
    }

    #[test]
    fn allows_set_user() {
        assert!(run_on("function setUser() {}").is_empty());
    }

    #[test]
    fn allows_setup_database() {
        assert!(run_on("function setupDatabase() {}").is_empty());
    }

    #[test]
    fn allows_close_account() {
        assert!(run_on("function closeAccount() {}").is_empty());
    }

    #[test]
    fn unit_pattern_match() {
        assert!(matches_set_x_to_y("setStatusToClosed"));
        assert!(matches_set_x_to_y("setRoleToAdmin"));
        assert!(matches_set_x_to_y("setUserToActive"));
        assert!(!matches_set_x_to_y("setUser"));
        assert!(!matches_set_x_to_y("setupAuto"));
        assert!(!matches_set_x_to_y("getUserToken"));
        assert!(!matches_set_x_to_y("setTo"));
    }
}
