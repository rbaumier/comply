//! Flag `<obs>.subscribe(...)` calls in Angular files when neither
//! `takeUntil` / `takeUntilDestroyed` / `DestroyRef` nor explicit
//! `.unsubscribe()` appear in the file.

use crate::diagnostic::{Diagnostic, Severity};

fn is_angular_file(source: &str) -> bool {
    source.contains("@angular/") || source.contains("@Component") || source.contains("@Injectable") || source.contains("@Directive")
}

fn file_has_unsubscribe_pattern(source: &str) -> bool {
    source.contains("takeUntilDestroyed")
        || source.contains("takeUntil(")
        || source.contains("DestroyRef")
        || source.contains(".unsubscribe(")
        || source.contains("Subscription")
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
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
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
