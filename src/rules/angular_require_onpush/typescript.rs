//! angular-require-onpush backend — flag `@Component({...})` decorators that
//! don't set `changeDetection: ChangeDetectionStrategy.OnPush`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DECORATOR: &str = "@Component(";

/// Explicit, performant `ChangeDetectionStrategy` values that opt out of the
/// default re-check-everything behavior: `OnPush` (observable/push patterns) and
/// `Eager` (the Signals-native strategy). Setting either is a deliberate choice.
const ACCEPTED_STRATEGIES: &[&str] = &[
    "ChangeDetectionStrategy.OnPush",
    "ChangeDetectionStrategy.Eager",
];

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Component("])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
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
            if !ACCEPTED_STRATEGIES.iter().any(|s| body.contains(s)) {
                let (line, column) = byte_to_line_col(ctx.source, start);
                out.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "`@Component` is missing `changeDetection: ChangeDetectionStrategy.OnPush`."
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

fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
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
    fn allows_component_with_eager() {
        let src = r#"
import { Component, ChangeDetectionStrategy } from '@angular/core';
@Component({
  selector: 'ngrx-root',
  standalone: true,
  imports: [RouterModule, TestPipe],
  template: `...`,
  changeDetection: ChangeDetectionStrategy.Eager,
})
export class AppComponent {}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_component_files() {
        let src = "export class Service {}";
        assert!(run(src).is_empty());
    }
}
