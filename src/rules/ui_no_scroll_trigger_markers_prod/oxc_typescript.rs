//! ui-no-scroll-trigger-markers-prod OXC backend — flag `markers: true` in
//! ScrollTrigger configs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn in_scrolltrigger_context<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        let parent = nodes.get_node(parent_id);
        let span = parent.kind().span();
        let text = &source[span.start as usize..span.end as usize];
        if text.contains("ScrollTrigger") || text.contains("scrollTrigger") {
            return true;
        }
        current_id = parent_id;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["ScrollTrigger"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "markers" {
            return;
        }

        // Value must be `true` literal
        let Expression::BooleanLiteral(val) = &prop.value else { return };
        if !val.value {
            return;
        }

        if !in_scrolltrigger_context(node, semantic, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "ScrollTrigger `markers: true` is unguarded \u{2014} wrap with `process.env.NODE_ENV !== \"production\"` so debug overlays stay out of prod.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_markers_true_in_scrolltrigger() {
        let src = r#"
            ScrollTrigger.create({
                trigger: ".box",
                markers: true,
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_markers_true_in_scroll_trigger_field() {
        let src = r#"
            gsap.to(".box", {
                scrollTrigger: { trigger: ".hero", markers: true },
                x: 100,
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_guarded_markers() {
        let src = r#"
            ScrollTrigger.create({
                trigger: ".box",
                markers: process.env.NODE_ENV !== "production",
            });
        "#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_markers_outside_scrolltrigger() {
        let src = r#"
            const cfg = { markers: true };
        "#;
        assert!(run(src).is_empty());
    }
}
