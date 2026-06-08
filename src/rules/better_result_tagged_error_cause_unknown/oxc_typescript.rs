use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["TaggedError"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else {
            return;
        };
        // Check if the class extends TaggedError.
        let Some(super_class) = &class.super_class else {
            return;
        };
        let super_text = &ctx.source[super_class.span().start as usize..super_class.span().end as usize];
        if !super_text.contains("TaggedError") {
            return;
        }
        // Walk the class body looking for a `cause` property.
        for element in &class.body.body {
            let oxc_ast::ast::ClassElement::PropertyDefinition(prop) = element else {
                continue;
            };
            let key_text = &ctx.source[prop.key.span().start as usize..prop.key.span().end as usize];
            if key_text != "cause" {
                continue;
            }
            // Check the type annotation.
            let Some(ts_type) = &prop.type_annotation else {
                continue;
            };
            let ty_text = &ctx.source[ts_type.span.start as usize..ts_type.span.end as usize];
            let ty_text = ty_text.trim().trim_start_matches(':').trim();
            if ty_text != "unknown" {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, prop.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("cause field must be typed `unknown`, found `{ty_text}`."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_cause_error_type() {
        let src = "class E extends TaggedError('E') { cause: Error = new Error(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_cause_unknown() {
        let src = "class E extends TaggedError('E') { cause: unknown = undefined; }";
        assert!(run(src).is_empty());
    }
}
