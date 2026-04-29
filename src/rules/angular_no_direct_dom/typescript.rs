//! Flag `document.getElementById/querySelector/...` calls in Angular files.

use crate::diagnostic::{Diagnostic, Severity};

fn is_angular_file(source: &str) -> bool {
    source.contains("@angular/") || source.contains("@Component") || source.contains("@Directive")
}

crate::ast_check! { on ["call_expression"] prefilter = ["@Component", "@Directive"] => |node, source, ctx, diagnostics|
    if !is_angular_file(ctx.source) { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(obj) = callee.child_by_field_name("object") else { return; };
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let obj_text = std::str::from_utf8(&source[obj.byte_range()]).unwrap_or("");
    let prop_text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
    if obj_text != "document" { return; }
    if !matches!(prop_text, "getElementById" | "querySelector" | "querySelectorAll" | "getElementsByClassName" | "getElementsByTagName") {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Direct DOM access via `document.{prop_text}` bypasses Angular's rendering — use `Renderer2` or `@ViewChild` instead."),
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
    fn flags_get_element_by_id_in_component() {
        let src = "import { Component } from '@angular/core';\n@Component({})\nclass C { ngOnInit() { document.getElementById('x'); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_query_selector_in_component() {
        let src = "import { Directive } from '@angular/core';\n@Directive({}) class D { f() { document.querySelector('.a'); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_angular_files() {
        let src = "function f() { document.getElementById('x'); }";
        assert!(run(src).is_empty());
    }
}
