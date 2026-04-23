//! ts-consistent-type-imports backend — flag `import { type A, type B }`
//! where every named specifier uses inline `type`; prefer `import type { A, B }`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    let trimmed = text.trim_start();

    // Already a top-level type import — fine.
    if trimmed.starts_with("import type") {
        return;
    }

    // Extract the named specifiers block `{ ... }`.
    let Some(brace_start) = text.find('{') else { return };
    let Some(brace_end_rel) = text[brace_start..].find('}') else { return };
    let between = &text[brace_start + 1..brace_start + brace_end_rel];

    let specs: Vec<&str> = between.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if specs.is_empty() {
        return;
    }

    // All specifiers must be `type X` (inline type) — then the whole import
    // should be hoisted to `import type { ... }`.
    if !specs.iter().all(|s| s.starts_with("type ")) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-consistent-type-imports".into(),
        message: "All imported specifiers are types — use `import type { ... }` \
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
        let d = run_on("import { type Foo, type Bar } from 'baz';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_single_inline_type() {
        let d = run_on("import { type Foo } from 'baz';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_import_type() {
        assert!(run_on("import type { Foo } from 'baz';").is_empty());
    }

    #[test]
    fn allows_mixed_value_and_type() {
        assert!(run_on("import { Foo, type Bar } from 'baz';").is_empty());
    }

    #[test]
    fn allows_plain_value_import() {
        assert!(run_on("import { foo } from 'baz';").is_empty());
    }
}
