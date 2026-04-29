//! Flag `<x>.detectChanges()` and `<x>.markForCheck()` calls in Angular files.

use crate::diagnostic::{Diagnostic, Severity};

fn is_angular_file(source: &str) -> bool {
    source.contains("@angular/") || source.contains("ChangeDetectorRef")
}

crate::ast_check! { on ["call_expression"] prefilter = ["ChangeDetectorRef"] => |node, source, ctx, diagnostics|
    if !is_angular_file(ctx.source) { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let prop_text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
    if !matches!(prop_text, "detectChanges" | "markForCheck") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`{prop_text}()` manually triggers change detection — prefer signals or `OnPush` with proper input mutations."),
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
    fn flags_detect_changes() {
        let src = "import { ChangeDetectorRef } from '@angular/core';\nfunction f(cdr: ChangeDetectorRef) { cdr.detectChanges(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_mark_for_check() {
        let src = "import { ChangeDetectorRef } from '@angular/core';\nfunction f(cdr: ChangeDetectorRef) { cdr.markForCheck(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_angular_files() {
        let src = "function f(x: any) { x.detectChanges(); }";
        assert!(run(src).is_empty());
    }
}
