//! Flag `<obs>.subscribe(...)` calls in Angular files when neither
//! `takeUntil` / `takeUntilDestroyed` / `DestroyRef` nor explicit
//! `.unsubscribe()` appear in the file.

use crate::diagnostic::{Diagnostic, Severity};

fn is_angular_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@angular/") || crate::oxc_helpers::source_contains(source, "@Component") || crate::oxc_helpers::source_contains(source, "@Injectable") || crate::oxc_helpers::source_contains(source, "@Directive")
}

fn file_has_unsubscribe_pattern(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "takeUntilDestroyed")
        || crate::oxc_helpers::source_contains(source, "takeUntil(")
        || crate::oxc_helpers::source_contains(source, "DestroyRef")
        || crate::oxc_helpers::source_contains(source, ".unsubscribe(")
        || crate::oxc_helpers::source_contains(source, "Subscription")
}

crate::ast_check! { on ["call_expression"] prefilter = [".unsubscribe("] => |node, source, ctx, diagnostics|
    if !is_angular_file(ctx.source) { return; }
    if file_has_unsubscribe_pattern(ctx.source) { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let prop_text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
    if prop_text != "subscribe" { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`.subscribe()` without `takeUntilDestroyed` / `DestroyRef` / explicit unsubscribe leaks the subscription.".into(),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_bare_subscribe() {
        let src = "import { Component } from '@angular/core';\n@Component({}) class C { f() { obs.subscribe(v => v); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_take_until_destroyed() {
        let src = "import { Component, takeUntilDestroyed } from '@angular/core';\n@Component({}) class C { f() { obs.pipe(takeUntilDestroyed()).subscribe(v => v); } }";
        assert!(run(src).is_empty());
    }
}
