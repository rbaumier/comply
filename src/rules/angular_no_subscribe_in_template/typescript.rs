//! Detect `.subscribe(` inside `template:` string literals of `@Component`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_angular_file(source: &str) -> bool {
    source.contains("@angular/") || source.contains("@Component")
}

crate::ast_check! { on ["pair"] prefilter = ["@Component"] => |node, source, ctx, diagnostics|
    if !is_angular_file(ctx.source) { return; }
    let Some(key) = node.child_by_field_name("key") else { return; };
    let key_text = std::str::from_utf8(&source[key.byte_range()]).unwrap_or("");
    if key_text != "template" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if !matches!(value.kind(), "string" | "template_string") { return; }
    let value_text = std::str::from_utf8(&source[value.byte_range()]).unwrap_or("");
    if !value_text.contains(".subscribe(") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &value,
        super::META.id,
        "`.subscribe()` inside an Angular template fires on every change-detection cycle — use the `async` pipe instead.".into(),
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
