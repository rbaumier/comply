//! no-and-in-function-name OXC backend — flag function names containing `And`
//! on a camelCase boundary.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
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
        let (name, span_start) = match node.kind() {
            AstKind::Function(func) => {
                let Some(id) = &func.id else { return };
                (id.name.as_str(), id.span.start)
            }
            AstKind::MethodDefinition(method) => {
                // An `override` method's name is dictated by the supertype it
                // overrides, not chosen by the author, so the "split into two
                // functions" remediation is impossible without breaking the
                // override contract (e.g. TypeORM `Repository.findAndCount`).
                if method.r#override {
                    return;
                }
                let (name, span_start) = match &method.key {
                    oxc_ast::ast::PropertyKey::StaticIdentifier(id) => {
                        (id.name.as_str(), id.span.start)
                    }
                    _ => return,
                };
                (name, span_start)
            }
            AstKind::VariableDeclarator(decl) => {
                // Only flag when the value is an arrow or function expression.
                let has_fn_value = decl.init.as_ref().is_some_and(|v| {
                    matches!(
                        v,
                        oxc_ast::ast::Expression::ArrowFunctionExpression(_)
                            | oxc_ast::ast::Expression::FunctionExpression(_)
                    )
                });
                if !has_fn_value {
                    return;
                }
                let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = decl.id else {
                    return;
                };
                (id.name.as_str(), id.span.start)
            }
            _ => return,
        };

        if !contains_and_boundary(name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Function `{name}` has `And` in its name — that signals two \
                 responsibilities glued together (CQS violation). Split into two \
                 functions named after each responsibility and let the caller \
                 sequence them."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True if `name` contains an `And` segment on a camelCase boundary —
/// i.e. preceded by a lowercase letter and followed by an uppercase letter.
fn contains_and_boundary(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 5 {
        return false;
    }
    let mut i = 1;
    while i + 3 < bytes.len() {
        if bytes[i] == b'A'
            && bytes[i + 1] == b'n'
            && bytes[i + 2] == b'd'
            && bytes[i - 1].is_ascii_lowercase()
            && bytes[i + 3].is_ascii_uppercase()
        {
            return true;
        }
        i += 1;
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
    fn allows_override_method_implementing_inherited_contract() {
        // Regression for rbaumier/comply#7423 — twentyhq/twenty
        // `WorkspaceRepository.findAndCount`. The `override` keyword binds the
        // method's name to the supertype's contract (TypeORM `Repository`), so
        // it cannot be renamed or split without breaking the override.
        let src = r#"class WorkspaceRepository<T> extends Repository<T> {
            override async findAndCount(o?: FindManyOptions<T>): Promise<[T[], number]> {
                return [[], 0];
            }
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_non_override_method_with_and_boundary() {
        // Negative space for #7423: a non-`override` method whose name has an
        // `And` boundary has no supertype dictating its name — it stays flagged.
        let src = "class C { invalidateAndRecompute() {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_second_non_override_method_with_and_boundary() {
        // Second control for #7423: another non-`override` `And`-boundary method.
        let src = "class C { getTargetEntityAndOperationType() {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_free_function_with_and_boundary() {
        // The `override` exemption is scoped to method definitions; a free
        // function with an `And` boundary stays flagged.
        assert_eq!(run_on("function getFooAndBar() {}").len(), 1);
    }

    #[test]
    fn still_flags_arrow_assigned_const_with_and_boundary() {
        // The `override` exemption is scoped to method definitions; an
        // arrow-assigned const (the VariableDeclarator arm) stays flagged.
        assert_eq!(run_on("const doFooAndBar = () => {};").len(), 1);
    }
}
