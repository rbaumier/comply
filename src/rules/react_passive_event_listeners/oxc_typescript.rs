//! react-passive-event-listeners OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

const SCROLL_EVENTS: &[&str] = &["touchstart", "touchmove", "wheel", "scroll"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["addEventListener"])
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

        // callee must be `*.addEventListener`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "addEventListener" {
            return;
        }

        // First argument must be a scroll/touch event string.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let event_name = match first_arg.as_expression() {
            Some(Expression::StringLiteral(lit)) => lit.value.as_str(),
            _ => return,
        };
        if !SCROLL_EVENTS.contains(&event_name) {
            return;
        }

        // If the callback calls preventDefault(), passive:true would break it — skip.
        if let Some(second_arg) = call.arguments.get(1) {
            let cb_src = &ctx.source
                [second_arg.span().start as usize..second_arg.span().end as usize];
            if cb_src.contains("preventDefault") {
                return;
            }
        }

        // Only flag when we can PROVE the listener carries no passive option:
        // no 3rd argument, an inline object literal without a `passive` property,
        // or a boolean-literal `useCapture` (legacy form, carries no passive).
        // A present-but-opaque options argument (an identifier/member/call/... that
        // holds the options object, e.g. `listenOpts.passive`) is not inspectable,
        // so `passive` may already be set — bail without flagging.
        if let Some(third_arg) = call.arguments.get(2) {
            let Some(opt_expr) = third_arg.as_expression() else {
                return;
            };
            match opt_expr {
                Expression::ObjectExpression(obj) => {
                    // A spread (`{ ...opts }`) may carry `passive` opaquely, so absence
                    // cannot be proven — bail.
                    if obj
                        .properties
                        .iter()
                        .any(|prop| matches!(prop, ObjectPropertyKind::SpreadProperty(_)))
                    {
                        return;
                    }
                    // A `passive` property (true or false) is a conscious choice —
                    // `true` is already optimal, `false` a deliberate opt-out so the
                    // handler can call preventDefault(); either way, do not suggest it.
                    let has_passive = obj.properties.iter().any(|prop| {
                        let ObjectPropertyKind::ObjectProperty(p) = prop else {
                            return false;
                        };
                        let key = match &p.key {
                            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                            PropertyKey::StringLiteral(s) => s.value.as_str(),
                            _ => return false,
                        };
                        key == "passive"
                    });
                    if has_passive {
                        return;
                    }
                }
                Expression::BooleanLiteral(_) => {}
                _ => return,
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Add `{{ passive: true }}` to `addEventListener('{event_name}', ...)` to avoid jank."
            ),
            severity: Severity::Warning,
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_touchmove_no_options() {
        assert_eq!(run("el.addEventListener('touchmove', handler)").len(), 1);
    }

    #[test]
    fn flags_touchmove_options_without_passive_key() {
        assert_eq!(
            run("el.addEventListener('touchmove', handler, { capture: true })").len(),
            1
        );
    }

    #[test]
    fn allows_passive_true() {
        assert!(
            run("el.addEventListener('touchmove', handler, { passive: true })").is_empty()
        );
    }

    #[test]
    fn allows_explicit_passive_false() {
        assert!(
            run("document.addEventListener('touchmove', handleTouchMove, { passive: false })")
                .is_empty()
        );
    }

    #[test]
    fn allows_explicit_passive_false_no_space() {
        assert!(run("el.addEventListener('touchmove', h, { passive:false })").is_empty());
    }

    #[test]
    fn allows_inline_prevent_default() {
        assert!(
            run("el.addEventListener('touchmove', (e) => { e.preventDefault(); })").is_empty()
        );
    }

    #[test]
    fn allows_options_by_member_reference() {
        // quasar Scroll.js: options passed as `listenOpts.passive` (opaque, may hold passive).
        assert!(
            run("ctx.scrollTarget.addEventListener('scroll', ctx.scroll, listenOpts.passive)")
                .is_empty()
        );
    }

    #[test]
    fn allows_options_by_bare_identifier() {
        // quasar ScrollFire.js: `const { passive } = listenOpts` then passed as `passive`.
        assert!(run("ctx.scrollTarget.addEventListener('scroll', ctx.scroll, passive)").is_empty());
    }

    #[test]
    fn allows_options_by_object_spread() {
        // A spread may carry `passive` opaquely — absence is unprovable.
        assert!(run("el.addEventListener('scroll', cb, { ...listenOpts })").is_empty());
    }

    #[test]
    fn flags_scroll_no_options() {
        assert_eq!(run("el.addEventListener('scroll', cb)").len(), 1);
    }

    #[test]
    fn flags_boolean_use_capture() {
        // Legacy `useCapture` boolean carries no passive option — provably absent.
        assert_eq!(run("el.addEventListener('scroll', cb, true)").len(), 1);
    }
}
