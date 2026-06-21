//! no-primitive-wrappers oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, peel_parens};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, NewExpression};
use oxc_span::GetSpan;
use std::sync::Arc;

const WRAPPER_TYPES: &[&str] = &["String", "Number", "Boolean"];

pub struct Check;

/// True when `expr` (parens peeled) is exactly the `new Wrapper(...)` under
/// inspection (matched by span so we only exempt the box that is itself the
/// parent's value slot, including when wrapped as `(new String(x))`).
fn is_new_expr(expr: &Expression, new_expr: &NewExpression) -> bool {
    let inner = peel_parens(expr);
    matches!(inner, Expression::NewExpression(_)) && inner.span() == new_expr.span
}

/// Deliberate boxing: the box is stored into an object so a downstream
/// consumer can recover its object identity (`instanceof String` / assigned
/// properties). Two identity-preserving placements:
///   * `obj.prop = new String(x)` / `obj[k] = new String(x)` — member-write,
///     and `x = new String(x)` — self-rebox (same binding on both sides).
///   * `{ key: new String(x) }` — object-literal property value.
fn is_deliberate_boxing(parent: AstKind, new_expr: &NewExpression) -> bool {
    match parent {
        AstKind::AssignmentExpression(assign) => {
            is_new_expr(&assign.right, new_expr)
                && is_identity_preserving_target(&assign.left, new_expr)
        }
        AstKind::ObjectProperty(prop) => is_new_expr(&prop.value, new_expr),
        _ => false,
    }
}

/// True when the assignment target preserves the box's object identity:
/// a member-write (`obj.prop = …` / `obj[k] = …`) or a self-rebox
/// (`x = new String(x)`, same binding name on both sides).
fn is_identity_preserving_target(target: &AssignmentTarget, new_expr: &NewExpression) -> bool {
    if target.as_member_expression().is_some() {
        return true;
    }
    let AssignmentTarget::AssignmentTargetIdentifier(id) = target else {
        return false;
    };
    let [arg] = new_expr.arguments.as_slice() else {
        return false;
    };
    let Some(Expression::Identifier(arg_id)) = arg.as_expression() else {
        return false;
    };
    arg_id.name == id.name
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        let Expression::Identifier(ident) = &new_expr.callee else { return };
        let name = ident.name.as_str();
        if !WRAPPER_TYPES.contains(&name) {
            return;
        }

        // Deliberate boxing: the wrapper object's *identity* is what the code
        // relies on, so the box is required and must not be flagged. A boxed
        // value is distinguishable from its primitive (via `instanceof String`
        // / assigned properties) — the pattern pdfkit uses to tag two distinct
        // PDF value types from one JS string. We detect it structurally: the
        // box is stored into an object slot, where its identity is preserved
        // for a downstream serializer's `instanceof` check (which usually lives
        // in a different file the per-file engine can't see). The genuine
        // anti-pattern (`const s = new String("x")` consumed as an ordinary
        // string, or `new String(x)` passed inline) is not such a placement and
        // stays flagged.
        // Skip `ParenthesizedExpression` ancestors so `obj.k = (new String(x))`
        // is read like the unparenthesized form.
        if let Some(parent) = semantic
            .nodes()
            .ancestors(node.id())
            .find(|n| !matches!(n.kind(), AstKind::ParenthesizedExpression(_)))
            && is_deliberate_boxing(parent.kind(), new_expr)
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Primitive wrapper object detected — `new {name}(...)` creates an object, not a primitive. Use `{name}(...)` without `new`.",
            ),
            severity: Severity::Error,
            span: None,
        });
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_plain_new_string() {
        // The genuine anti-pattern: a boxed value consumed as an ordinary
        // string with no object-identity use.
        let src = r#"const s = new String("x"); console.log(s.length);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_new_number_and_boolean() {
        assert_eq!(run("const n = new Number(1);").len(), 1);
        assert_eq!(run("const b = new Boolean(true);").len(), 1);
    }

    #[test]
    fn flags_new_string_passed_inline() {
        // Inline argument is not an assignment RHS — still the anti-pattern.
        let src = r#"f(new String("x"));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_box_assigned_to_object_property() {
        // Regression for rbaumier/comply#5324 — pdfkit structure_element.js:
        // storing the box in an object property keeps its identity so a
        // downstream serializer can discriminate it via `instanceof String`.
        let src = r#"
            const data = {};
            if (options.title) {
                data.T = new String(options.title);
            }
            if (options.lang) {
                data.Lang = new String(options.lang);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_box_assigned_to_computed_member() {
        let src = r#"obj[key] = new String(value);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_box_as_object_literal_property() {
        // Regression for rbaumier/comply#5324 — pdfkit annotations.js / acroform.js:
        // boxing a string into a PDF dictionary so the serializer renders it as
        // a PDF literal string `(...)` rather than a PDF name `/...`.
        let src = r#"
            const a = this.ref({
                S: 'URI',
                URI: new String(url),
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_box_as_object_literal_property_with_literal_arg() {
        // pdfkit pdfa.js: boxed string literal stored in a PDF dict entry.
        let src = r#"const d = { Info: new String('sRGB IEC61966-2.1') };"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_self_rebox_under_typeof_guard() {
        // Regression for rbaumier/comply#5324 — pdfkit document.js: re-boxing
        // a value into its own binding promotes the primitive to a boxed tag
        // the serializer distinguishes via `instanceof String`.
        let src = r#"
            for (let key in this.info) {
                let val = this.info[key];
                if (typeof val === 'string') {
                    val = new String(val);
                }
                let entry = this.ref(val);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_assignment_to_plain_identifier_with_literal() {
        // Negative space: assigning a freshly boxed literal to a binding whose
        // name differs from the argument is not self-reboxing — stays flagged.
        let src = r#"let s; s = new String("x");"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_bare_expression_statement() {
        // Negative space: a box discarded as a bare statement has no identity
        // consumer — stays flagged.
        let src = r#"new String("x");"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_zero_arg_self_rebox() {
        // Negative space: `x = new String()` boxes nothing meaningful (no arg
        // to preserve), so it is not the self-rebox idiom — stays flagged.
        let src = r#"let x; x = new String();"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_parenthesized_box_in_object_property() {
        // Parens around the box must not defeat the exemption.
        let src = r#"const d = { K: (new String(x)) };"#;
        assert!(run(src).is_empty());
    }
}
