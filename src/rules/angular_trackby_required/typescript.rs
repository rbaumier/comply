//! Flag `*ngFor` template directives that don't include `trackBy`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_angular_file(source: &str) -> bool {
    source.contains("@angular/") || source.contains("@Component")
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !is_angular_file(ctx.source) { return; }
    let Some(key) = node.child_by_field_name("key") else { return; };
    let key_text = std::str::from_utf8(&source[key.byte_range()]).unwrap_or("");
    if key_text != "template" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if !matches!(value.kind(), "string" | "template_string") { return; }
    let value_text = std::str::from_utf8(&source[value.byte_range()]).unwrap_or("");
    for (idx, _) in value_text.match_indices("*ngFor") {
        let tail = &value_text[idx..];
        let end = tail.find(['"', '\'']).map(|p| p + 1).unwrap_or(tail.len());
        let attr_section_end = tail[end..].find(['"', '\'']).map(|p| end + p).unwrap_or(tail.len());
        let attr_section = &tail[..attr_section_end];
        if !attr_section.contains("trackBy") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &value,
                super::META.id,
                "`*ngFor` without `trackBy` causes Angular to recreate every DOM node when the array reference changes.".into(),
                Severity::Warning,
            ));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_ngfor_without_trackby() {
        let src = "import { Component } from '@angular/core';\n@Component({ template: `<li *ngFor=\"let it of items\">{{it}}</li>` }) class C {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ngfor_with_trackby() {
        let src = "import { Component } from '@angular/core';\n@Component({ template: `<li *ngFor=\"let it of items; trackBy: trackById\">{{it}}</li>` }) class C {}";
        assert!(run(src).is_empty());
    }
}
