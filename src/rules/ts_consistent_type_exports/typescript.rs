//! ts-consistent-type-exports backend — flag `export { type A, type B }`
//! where every specifier uses inline `type`; prefer `export type { A, B }`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["export_statement"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    let trimmed = text.trim_start();

    // Already a top-level type export — fine.
    if trimmed.starts_with("export type") {
        return;
    }

    // Must be a named export — find braces.
    let Some(brace_start) = text.find('{') else { return };
    let Some(brace_end_rel) = text[brace_start..].find('}') else { return };
    let between = &text[brace_start + 1..brace_start + brace_end_rel];

    let specs: Vec<&str> = between.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if specs.is_empty() {
        return;
    }

    if !specs.iter().all(|s| s.starts_with("type ")) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-consistent-type-exports".into(),
        message: "All exported specifiers are types — use `export type { ... }` \
                  at the top level instead of inline `type` markers.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_all_inline_type_specifiers() {
        let d = run_on("export { type Foo, type Bar };");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_inline_type_reexport() {
        let d = run_on("export { type Foo } from './baz';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_export_type() {
        assert!(run_on("export type { Foo } from './baz';").is_empty());
    }

    #[test]
    fn allows_mixed_value_and_type() {
        assert!(run_on("export { Foo, type Bar } from './baz';").is_empty());
    }

    #[test]
    fn allows_plain_value_export() {
        assert!(run_on("export { foo } from './baz';").is_empty());
    }
}
