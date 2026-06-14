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
        self.check(&CheckCtx::for_test_full(path, src, project, file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_rule_gated;
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

    // Test fixture component in a `.spec.ts` file — the gate (`skip_in_test_dir`)
    // suppresses the rule. Uses `run_rule_gated` so `applies_to_file` runs;
    // plain `run_rule`/`run` bypasses the gate.
    const SPEC_FIXTURE: &str = r#"
import { Component } from '@angular/core';
@Component({
  template: `<greet [firstName]="firstName" />`,
  imports: [GreetComponent],
})
class TestCmp {
  clickCount = 0;
  firstName = 'Initial';
}
"#;

    #[test]
    fn skips_test_fixture_component_in_spec_file() {
        assert!(
            run_rule_gated(&Check, SPEC_FIXTURE, "src/app/greet.component.spec.ts").is_empty(),
            "test fixture components in .spec.ts files must not be flagged"
        );
    }

    #[test]
    fn still_flags_component_in_non_test_file() {
        assert_eq!(
            run_rule_gated(&Check, SPEC_FIXTURE, "src/app/greet.component.ts").len(),
            1,
            "the same component in a production file must still be flagged"
        );
    }
}
