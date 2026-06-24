//! tailwind-require-motion-reduce OXC backend — require `motion-reduce:*` on
//! elements that use `transition-*` or `animate-*` utilities.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_animation_base(base: &str) -> bool {
    (base == "transition" || base.starts_with("transition-")) && base != "transition-none"
        || base == "animate-spin"
        || base == "animate-ping"
        || base == "animate-pulse"
        || base == "animate-bounce"
        || (base.starts_with("animate-") && base != "animate-none")
}

/// True when any variant segment of `tok` is `motion-reduce` or `motion-safe`,
/// regardless of its position in the chain. A `motion-reduce` (or `motion-safe`)
/// override scoped under a data-attribute variant
/// (`in-data-[type=loading]:motion-reduce:animate-none`) satisfies the rule just
/// as a leading one does — Tailwind treats the two orderings as equivalent.
fn has_motion_preference_variant(tok: &str) -> bool {
    // Drop the final segment: it's the utility base (`animate-none`), never the
    // variant we're looking for, and stopping before it avoids matching a
    // utility that happens to be named like the variant.
    let Some((variants, _base)) = tok.rsplit_once(':') else {
        return false;
    };
    variants
        .split(':')
        .any(|segment| segment == "motion-reduce" || segment == "motion-safe")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        if ident.name.as_str() != "className" && ident.name.as_str() != "class" {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        let value = lit.value.as_str();

        let mut has_motion = false;
        let mut has_reduce = false;

        for tok in value.split_whitespace() {
            if has_motion_preference_variant(tok) {
                has_reduce = true;
                continue;
            }
            let base = tok.rsplit(':').next().unwrap_or(tok);
            if is_animation_base(base) {
                has_motion = true;
            }
        }

        if has_motion && !has_reduce {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, attr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Animation/transition without `motion-reduce:*` — users with `prefers-reduced-motion: reduce` will still see the animation.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::empty_with_tailwind();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        crate::rules::test_helpers::run_rule_with_ctx(&Check, s, "t.tsx", &project, file)
    }

    #[test]
    fn flags_animate_spin_without_motion_reduce() {
        assert_eq!(
            run(r#"export const A = () => <div className="animate-spin" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_transition_opacity_without_motion_reduce() {
        assert_eq!(
            run(r#"export const A = () => <div className="transition-opacity" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_leading_motion_reduce_variant() {
        assert!(
            run(r#"export const A = () => <div className="animate-spin motion-reduce:animate-none" />;"#)
                .is_empty()
        );
    }

    // Regression for issue #499: a `motion-reduce` override nested under a
    // data-attribute variant must satisfy the rule.
    #[test]
    fn allows_motion_reduce_nested_in_data_attribute_variant() {
        assert!(
            run(r#"export const A = () => <Icon className="in-data-[type=loading]:animate-spin in-data-[type=loading]:motion-reduce:animate-none" />;"#)
                .is_empty()
        );
    }

    // Regression for issue #499: the `**:data-current` arbitrary-variant case.
    #[test]
    fn allows_motion_reduce_nested_in_arbitrary_data_variant() {
        assert!(
            run(r#"export const A = () => <div className="**:data-current:transition-opacity **:data-current:motion-reduce:transition-none" />;"#)
                .is_empty()
        );
    }

    #[test]
    fn flags_data_attribute_animation_with_no_motion_reduce_anywhere() {
        assert_eq!(
            run(r#"export const A = () => <Icon className="in-data-[type=loading]:animate-spin" />;"#)
                .len(),
            1
        );
    }

    #[test]
    fn allows_motion_safe_variant() {
        assert!(
            run(r#"export const A = () => <div className="motion-safe:animate-spin" />;"#).is_empty()
        );
    }
}
