//! import-consistent-type-specifier-style backend — prefer top-level `import type`.
//!
//! Enforces `prefer-top-level` style: `import type { Foo }` rather than
//! inline `import { type Foo }`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");

    // Already a top-level type import — fine.
    let trimmed = text.trim();
    if trimmed.starts_with("import type ") {
        return;
    }

    // Look for inline `type` specifiers: `import { type Foo, type Bar }`.
    // Detect pattern: `{ type ` inside the import.
    if let Some(brace_start) = text.find('{')
        && let Some(brace_end) = text[brace_start..].find('}') {
            let between = &text[brace_start + 1..brace_start + brace_end];
            let specs: Vec<&str> = between.split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let type_count = specs.iter().filter(|s| s.starts_with("type ")).count();

            if type_count > 0 {
                let pos = node.start_position();
                let message = if type_count == specs.len() {
                    "Prefer using a top-level `import type` instead of inline `type` specifiers."
                } else {
                    "Split mixed imports: use a separate `import type` for type specifiers and a regular `import` for value specifiers."
                };
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "import-consistent-type-specifier-style".into(),
                    message: message.into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_inline_type() {
        let d = run_on("import { type Foo } from 'bar';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("top-level"));
    }

    #[test]
    fn flags_all_inline_types() {
        let d = run_on("import { type Foo, type Bar } from 'bar';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("top-level"));
    }

    #[test]
    fn flags_mixed_import_with_split_message() {
        let d = run_on("import { value, type Foo } from 'bar';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Split mixed imports"));
    }

    #[test]
    fn allows_top_level_type() {
        assert!(run_on("import type { Foo } from 'bar';").is_empty());
    }

    #[test]
    fn allows_normal_import() {
        assert!(run_on("import { foo } from 'bar';").is_empty());
    }
}
