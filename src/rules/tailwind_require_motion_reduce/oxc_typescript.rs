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
            if tok.starts_with("motion-reduce:") {
                has_reduce = true;
                continue;
            }
            if tok.starts_with("motion-safe:") {
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
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_transition_without_motion_reduce() {
        assert_eq!(
            run(r#"export const A = () => <div className="transition-colors duration-300" />;"#)
                .len(),
            1
        );
    }


    #[test]
    fn flags_animate_spin_without_motion_reduce() {
        assert_eq!(
            run(r#"export const A = () => <div className="animate-spin" />;"#).len(),
            1
        );
    }


    #[test]
    fn allows_motion_reduce_pair() {
        assert!(run(r#"export const A = () => <div className="transition-colors motion-reduce:transition-none" />;"#).is_empty());
    }


    #[test]
    fn allows_static_classes() {
        assert!(run(r#"export const A = () => <div className="p-4 bg-card" />;"#).is_empty());
    }
}
