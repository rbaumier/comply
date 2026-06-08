use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["setProperty"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let callee_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if !callee_text.contains("document.documentElement.style.setProperty") {
            return;
        }

        if !is_inside_raf(node, ctx, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Global CSS variable change inside `requestAnimationFrame` triggers full-page style recalc every frame.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_inside_raf<'a>(
    node: &oxc_semantic::AstNode<'a>,
    ctx: &CheckCtx,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut id = node.id();
    loop {
        let parent_id = nodes.parent_id(id);
        if parent_id == id {
            break;
        }
        id = parent_id;
        match nodes.kind(id) {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Check if this function is a direct argument to requestAnimationFrame(...)
                let call_id = nodes.parent_id(id);
                if call_id == id {
                    continue;
                }
                if let AstKind::CallExpression(call) = nodes.kind(call_id) {
                    let callee_text =
                        &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
                    if callee_text == "requestAnimationFrame" {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_set_property_in_raf() {
        assert_eq!(
            run(r#"
requestAnimationFrame(() => {
    document.documentElement.style.setProperty('--scroll', window.scrollY);
});
"#)
            .len(),
            1
        );
    }


    #[test]
    fn allows_scoped_set_property_in_raf() {
        assert!(
            run(r#"
requestAnimationFrame(() => {
    element.style.setProperty('--x', value);
});
"#)
            .is_empty()
        );
    }


    #[test]
    fn allows_set_property_outside_raf() {
        assert!(
            run(r#"
document.documentElement.style.setProperty('--theme', 'dark');
"#)
            .is_empty()
        );
    }
}
