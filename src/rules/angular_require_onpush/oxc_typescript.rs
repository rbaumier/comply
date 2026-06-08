use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const DECORATOR: &str = "@Component(";

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Component("])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains(DECORATOR) {
            return Vec::new();
        }
        let bytes = ctx.source.as_bytes();
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = ctx.source[from..].find(DECORATOR) {
            let start = from + rel;
            let after = start + DECORATOR.len();
            let mut depth = 1;
            let mut i = after;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            let mut body_end = i.saturating_sub(1);
            while body_end > after && !ctx.source.is_char_boundary(body_end) {
                body_end -= 1;
            }
            let body = &ctx.source[after..body_end];
            if !body.contains("ChangeDetectionStrategy.OnPush") {
                let (line, column) = byte_offset_to_line_col(ctx.source, start);
                out.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`@Component` is missing \
                              `changeDetection: ChangeDetectionStrategy.OnPush`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            from = i;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_component_without_onpush() {
        let src = r#"
import { Component } from '@angular/core';
@Component({ selector: 'x', template: '' })
export class X {}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_component_with_onpush() {
        let src = r#"
import { Component, ChangeDetectionStrategy } from '@angular/core';
@Component({
  selector: 'x',
  template: '',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class X {}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_component_files() {
        let src = "export class Service {}";
        assert!(run(src).is_empty());
    }
}
