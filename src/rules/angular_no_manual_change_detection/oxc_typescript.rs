use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_angular_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@angular/") || crate::oxc_helpers::source_contains(source, "ChangeDetectorRef")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["ChangeDetectorRef"])
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop_text = member.property.name.as_str();
        if !matches!(prop_text, "detectChanges" | "markForCheck") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{prop_text}()` manually triggers change detection — prefer signals or `OnPush` with proper input mutations."
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
    use super::*;
    use crate::rules::test_helpers::{run_rule, run_rule_gated};

    // Issue #2270 — `fixture.detectChanges()` / `markForCheck()` in Angular
    // `.spec.ts` test files are the mandated `ComponentFixture` testing API, not
    // a production anti-pattern. The central `skip_in_test_dir` gate exempts them.
    const SPEC_FIXTURE: &str = r#"
import { TestBed } from '@angular/core/testing';
function markAndDetect() {
  fixture.componentRef.changeDetectorRef.markForCheck();
  fixture.detectChanges();
}
"#;

    #[test]
    fn gated_no_fp_in_spec_file() {
        assert!(
            run_rule_gated(&Check, SPEC_FIXTURE, "modules/component/spec/push/push.pipe.spec.ts")
                .is_empty(),
            "skip_in_test_dir must suppress detectChanges/markForCheck in .spec.ts files"
        );
    }

    // The same calls in a production component must still fire — the exemption is
    // test-directory-specific, not a blanket disable.
    #[test]
    fn gated_still_fires_in_production_component() {
        let src = "import { ChangeDetectorRef } from '@angular/core';\nclass C { constructor(private cdr: ChangeDetectorRef) {} update() { this.cdr.detectChanges(); } }";
        assert_eq!(
            run_rule_gated(&Check, src, "src/app/foo.component.ts").len(),
            1,
            "manual detectChanges() in a production component is still flagged"
        );
    }

    #[test]
    fn flags_mark_for_check_in_production() {
        let src = "import { ChangeDetectorRef } from '@angular/core';\nclass C { constructor(private cdr: ChangeDetectorRef) {} update() { this.cdr.markForCheck(); } }";
        assert_eq!(run_rule(&Check, src, "src/app/foo.component.ts").len(), 1);
    }
}
