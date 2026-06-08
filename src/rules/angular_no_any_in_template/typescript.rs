//! Flag `$any(` inside `template:` strings of `@Component` decorators.

use crate::diagnostic::{Diagnostic, Severity};

fn is_angular_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@angular/") || crate::oxc_helpers::source_contains(source, "@Component")
}

crate::ast_check! { on ["pair"] prefilter = ["@Component"] => |node, source, ctx, diagnostics|
    if !is_angular_file(ctx.source) { return; }
    let Some(key) = node.child_by_field_name("key") else { return; };
    let key_text = std::str::from_utf8(&source[key.byte_range()]).unwrap_or("");
    if key_text != "template" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if !matches!(value.kind(), "string" | "template_string") { return; }
    let value_text = std::str::from_utf8(&source[value.byte_range()]).unwrap_or("");
    if !value_text.contains("$any(") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &value,
        super::META.id,
        "`$any()` in an Angular template defeats template type checking.".into(),
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
    fn flags_dollar_any_in_template() {
        let src = "import { Component } from '@angular/core';\n@Component({ template: `<p>{{ $any(user).name }}</p>` }) class C {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_typed_template() {
        let src = "import { Component } from '@angular/core';\n@Component({ template: `<p>{{ user.name }}</p>` }) class C {}";
        assert!(run(src).is_empty());
    }
}
