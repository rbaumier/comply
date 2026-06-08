use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_angular_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@angular/") || crate::oxc_helpers::source_contains(source, "@Component")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Component"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_angular_file(ctx.source) {
            return;
        }
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };
        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_name != "template" {
            return;
        }
        let value_text = match &prop.value {
            Expression::StringLiteral(s) => s.value.as_str(),
            Expression::TemplateLiteral(t) => {
                if t.quasis.len() == 1 {
                    t.quasis[0].value.raw.as_str()
                } else {
                    return;
                }
            }
            _ => return,
        };
        if !value_text.contains(".subscribe(") {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, prop.value.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.subscribe()` inside an Angular template fires on every change-detection cycle — use the `async` pipe instead.".into(),
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_subscribe_in_inline_template() {
        let src = "import { Component } from '@angular/core';\n@Component({ template: `<p>{{ data$.subscribe(v => v) }}</p>` }) class C {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_async_pipe_in_template() {
        let src = "import { Component } from '@angular/core';\n@Component({ template: `<p>{{ data$ | async }}</p>` }) class C {}";
        assert!(run(src).is_empty());
    }
}
