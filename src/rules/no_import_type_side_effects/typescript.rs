//! no-import-type-side-effects backend — flag `import { type A, type B }`
//! where EVERY specifier is a type, since it can be collapsed to
//! `import type { A, B }`.
//!
//! Only matches when all named specifiers carry `type`; mixed imports
//! (`import { type A, b }`) are left alone by this rule — that's the
//! `@typescript-eslint/consistent-type-imports` territory.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    let trimmed = text.trim();

    // Already `import type { ... }` — nothing to do.
    if trimmed.starts_with("import type ") || trimmed.starts_with("import type{") {
        return;
    }

    // We only care about `import { ... } from ...` with named specifiers.
    let Some(brace_start) = text.find('{') else { return };
    let Some(brace_rel) = text[brace_start..].find('}') else { return };
    let between = &text[brace_start + 1..brace_start + brace_rel];

    // Require at least one specifier.
    let specifiers: Vec<&str> = between
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if specifiers.is_empty() {
        return;
    }

    // Every specifier must begin with `type ` for this rule to fire.
    let all_type = specifiers
        .iter()
        .all(|spec| spec.starts_with("type ") || spec.starts_with("type\t"));
    if !all_type {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-import-type-side-effects".into(),
        message: "All specifiers are `type` imports — hoist to `import type { ... }` so the runtime module is dropped.".into(),
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
    fn flags_all_type_specifiers() {
        assert_eq!(run_on("import { type Foo, type Bar } from 'x';").len(), 1);
    }

    #[test]
    fn flags_single_type_specifier() {
        assert_eq!(run_on("import { type Foo } from 'x';").len(), 1);
    }

    #[test]
    fn allows_top_level_type_import() {
        assert!(run_on("import type { Foo, Bar } from 'x';").is_empty());
    }

    #[test]
    fn allows_mixed_import() {
        assert!(run_on("import { type Foo, bar } from 'x';").is_empty());
    }

    #[test]
    fn allows_plain_value_import() {
        assert!(run_on("import { foo } from 'x';").is_empty());
    }

    #[test]
    fn allows_default_import() {
        assert!(run_on("import Foo from 'x';").is_empty());
    }
}
