//! consistent-assert AST backend — prefer `assert.ok(...)` over bare
//! `assert(...)` with `node:assert`.

use crate::diagnostic::{Diagnostic, Severity};

/// Extract the local name from an assert import statement node.
/// Handles `import assert from 'node:assert'`, `import { strict as t } from 'assert'`, etc.
fn extract_assert_import_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    if node.kind() != "import_statement" {
        return None;
    }

    // Check the source module.
    let src_node = node.child_by_field_name("source")?;
    let src_text = src_node.utf8_text(source).unwrap_or("");
    let is_assert_module = src_text.contains("node:assert")
        || src_text.contains("'assert'")
        || src_text.contains("\"assert\"")
        || src_text.contains("assert/strict");
    if !is_assert_module {
        return None;
    }

    // Look for the import clause — default import or named import.
    let count = node.child_count();
    for i in 0..count {
        let child = node.child(i).unwrap();
        // `import assert from '...'` — the default import is an `identifier`.
        if child.kind() == "import_clause" {
            let cc = child.child_count();
            for j in 0..cc {
                let inner = child.child(j).unwrap();
                if inner.kind() == "identifier" {
                    return Some(inner.utf8_text(source).unwrap_or("").to_string());
                }
                // Named imports: `{ strict as t }`
                if inner.kind() == "named_imports" {
                    let nc = inner.named_child_count();
                    for k in 0..nc {
                        let spec = inner.named_child(k).unwrap();
                        if spec.kind() == "import_specifier" {
                            // The alias (if any) is the second identifier.
                            if let Some(alias) = spec.child_by_field_name("alias") {
                                return Some(
                                    alias.utf8_text(source).unwrap_or("").to_string(),
                                );
                            }
                            // No alias — use the name directly.
                            if let Some(name) = spec.child_by_field_name("name") {
                                return Some(
                                    name.utf8_text(source).unwrap_or("").to_string(),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // We operate at the program level: find the import, then scan calls.
    if node.kind() != "program" {
        return;
    }

    // Pass 1: find the assert import name.
    let mut assert_name: Option<String> = None;
    let child_count = node.named_child_count();
    for i in 0..child_count {
        let child = node.named_child(i).unwrap();
        if let Some(name) = extract_assert_import_name(child, source) {
            assert_name = Some(name);
            break;
        }
    }

    let Some(name) = assert_name else { return };

    // Pass 2: find bare `name(` calls by walking the full source text.
    // This is simpler and more reliable than AST-walking for this pattern.
    let text = std::str::from_utf8(source).unwrap_or("");
    let bare_call = format!("{}(", name);

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            continue;
        }

        let mut search_start = 0;
        while let Some(pos) = line[search_start..].find(&bare_call) {
            let abs = search_start + pos;

            // Word boundary check.
            let before = if abs > 0 {
                line.as_bytes().get(abs - 1).copied()
            } else {
                None
            };
            let is_boundary = match before {
                None => true,
                Some(b) => !b.is_ascii_alphanumeric() && b != b'_' && b != b'$',
            };

            // Not a method call (`obj.assert(`).
            let is_method = abs > 0 && line[..abs].trim_end().ends_with('.');

            if is_boundary && !is_method {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: abs + 1,
                    rule_id: "consistent-assert".into(),
                    message: format!("Prefer `{}.ok(…)` over bare `{}(…)`.", name, name),
                    severity: Severity::Warning,
                });
            }

            search_start = abs + bare_call.len();
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
    fn flags_bare_assert_call() {
        let src = "import assert from 'node:assert';\nassert(x === 42);";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "consistent-assert");
    }

    #[test]
    fn flags_bare_assert_strict() {
        let src = "import assert from 'node:assert/strict';\nassert(value);";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_assert_ok() {
        let src = "import assert from 'node:assert';\nassert.ok(value);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_assert_strict_equal() {
        let src = "import assert from 'node:assert';\nassert.strictEqual(x, 42);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_flag_without_import() {
        let src = "assert(true);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn handles_renamed_import() {
        let src = "import { strict as t } from 'assert';\nt(value);";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
