use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_angular_component(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@Component")
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Component"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_angular_component(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//")
                || trimmed.starts_with("import ")
                || trimmed.starts_with('*')
            {
                continue;
            }
            if let Some(col) = line.find("new BehaviorSubject") {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "Use `signal()` from `@angular/core` instead of `BehaviorSubject` \
                              for component state."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_behavior_subject_in_component() {
        let src = r#"
import { Component } from '@angular/core';
import { BehaviorSubject } from 'rxjs';
@Component({ selector: 'x' })
export class X {
  count = new BehaviorSubject<number>(0);
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_signal() {
        let src = r#"
import { Component, signal } from '@angular/core';
@Component({ selector: 'x' })
export class X {
  count = signal(0);
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_component_files() {
        let src = r#"
import { BehaviorSubject } from 'rxjs';
export class Service { count = new BehaviorSubject<number>(0); }
"#;
        assert!(run(src).is_empty());
    }
}
