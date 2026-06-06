//! Flag `@Component({...})` decorators where the config object does not set
//! `standalone: true` and does not set `standalone: false` explicitly.

use crate::diagnostic::{Diagnostic, Severity};

fn is_angular_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@angular/") || crate::oxc_helpers::source_contains(source, "@Component")
}

crate::ast_check! { on ["decorator"] prefilter = ["@Component"] => |node, source, ctx, diagnostics|
    if !is_angular_file(ctx.source) { return; }
    let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
    if !text.starts_with("@Component") { return; }
    if text.contains("standalone") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`@Component` without `standalone: true` — prefer standalone components over NgModule declarations (Angular 15+).".into(),
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
    fn flags_component_without_standalone() {
        let src = "import { Component } from '@angular/core';\n@Component({ template: 'x' }) class C {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_standalone_component() {
        let src = "import { Component } from '@angular/core';\n@Component({ standalone: true, template: 'x' }) class C {}";
        assert!(run(src).is_empty());
    }
}
