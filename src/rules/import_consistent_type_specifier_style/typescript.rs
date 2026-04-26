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
            // Split by comma and check if any specifier starts with `type `.
            let has_inline_type = between.split(',')
                .any(|spec| spec.trim().starts_with("type "));

            if has_inline_type {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "import-consistent-type-specifier-style".into(),
                    message: "Prefer using a top-level `import type` instead of inline `type` specifiers.".into(),
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
    fn allows_top_level_type() {
        assert!(run_on("import type { Foo } from 'bar';").is_empty());
    }

    #[test]
    fn allows_normal_import() {
        assert!(run_on("import { foo } from 'bar';").is_empty());
    }
}
