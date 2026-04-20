//! Post-filter for `id-length` — allow `t` from `react-i18next`.
//!
//! `useTranslation()` returns `{ t, i18n }` by convention; renaming `t`
//! to `translate` costs the entire i18n ecosystem's pattern-match. This
//! filter drops the oxlint `id-length` diagnostic when the offending
//! identifier is exactly `t` AND the file imports `useTranslation`
//! from `react-i18next`.
//!
//! Scoped narrowly on purpose: `t` anywhere else (a local `const t = …`,
//! or `t` in a non-i18n file) is still flagged. We fix the rule's
//! heuristic, we don't disable the rule.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;

/// Drop `id-length` diagnostics on the `t` identifier when the file
/// imports `useTranslation` from `react-i18next`. Mutates in place.
pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut sources: HashMap<PathBuf, Option<String>> = HashMap::new();
    diagnostics.retain(|d| {
        if d.rule_id != "id-length" {
            return true;
        }
        let source = sources
            .entry(d.path.clone())
            .or_insert_with(|| std::fs::read_to_string(&d.path).ok());
        let Some(source) = source.as_deref() else {
            return true;
        };
        if !imports_use_translation(source) {
            return true;
        }
        if identifier_at(source, d.line, d.column) != Some("t") {
            return true;
        }
        false
    });
}

/// True if the file imports `useTranslation` from `react-i18next`.
/// Checks both `import { useTranslation } from "react-i18next"` and
/// the aliased `import { useTranslation as …}` forms. Doesn't follow
/// re-exports — if a project wraps the hook, add the wrapper's module
/// here.
fn imports_use_translation(source: &str) -> bool {
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("import") {
            continue;
        }
        if !trimmed.contains("useTranslation") {
            continue;
        }
        if trimmed.contains("\"react-i18next\"") || trimmed.contains("'react-i18next'") {
            return true;
        }
    }
    false
}

/// Extract the identifier starting at `(line, column)` in `source`.
/// Both `line` and `column` are 1-based, matching diagnostic semantics.
/// Returns `None` if the position is out of bounds or doesn't start an
/// identifier.
fn identifier_at(source: &str, line: usize, column: usize) -> Option<&str> {
    let line_text = source.lines().nth(line.checked_sub(1)?)?;
    let col = column.checked_sub(1)?;
    if col >= line_text.len() {
        return None;
    }
    let rest = &line_text[col..];
    let end = rest
        .find(|c: char| !is_ident_char(c))
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// Unused but kept for future extensions — e.g. also allowlist `i18n`.
#[allow(dead_code)]
fn is_typescript_family(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" | "cts" | "cjs")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;

    fn id_length(path: &str, line: usize, column: usize) -> Diagnostic {
        Diagnostic {
            path: PathBuf::from(path),
            line,
            column,
            rule_id: "id-length".into(),
            message: "Identifier name is too short (< 2).".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    #[test]
    fn detects_use_translation_import_double_quotes() {
        let src = "import { useTranslation } from \"react-i18next\";\n";
        assert!(imports_use_translation(src));
    }

    #[test]
    fn detects_use_translation_import_single_quotes() {
        let src = "import { useTranslation } from 'react-i18next';\n";
        assert!(imports_use_translation(src));
    }

    #[test]
    fn rejects_import_from_other_package() {
        let src = "import { useTranslation } from \"my-i18n\";\n";
        assert!(!imports_use_translation(src));
    }

    #[test]
    fn rejects_when_no_import_at_all() {
        let src = "const t = 1;\n";
        assert!(!imports_use_translation(src));
    }

    #[test]
    fn identifier_at_extracts_t() {
        let src = "  const { t } = useTranslation();\n";
        assert_eq!(identifier_at(src, 1, 11), Some("t"));
    }

    #[test]
    fn identifier_at_extracts_multichar() {
        let src = "  const { foo } = bar;\n";
        assert_eq!(identifier_at(src, 1, 11), Some("foo"));
    }

    #[test]
    fn identifier_at_out_of_bounds() {
        assert_eq!(identifier_at("x\n", 99, 1), None);
        assert_eq!(identifier_at("x\n", 1, 99), None);
    }

    #[test]
    fn apply_keeps_unrelated_diagnostics() {
        let mut diags = vec![Diagnostic {
            path: PathBuf::from("/tmp/nope.tsx"),
            line: 1,
            column: 1,
            rule_id: "other-rule".into(),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn apply_keeps_id_length_when_file_missing() {
        let mut diags = vec![id_length("/does-not-exist.tsx", 1, 1)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn apply_drops_t_in_i18n_file() {
        let dir = tempdir();
        let file = dir.join("f.tsx");
        std::fs::write(
            &file,
            "import { useTranslation } from \"react-i18next\";\nfn(() => { const { t } = useTranslation(); });\n",
        )
        .unwrap();
        let col = "fn(() => { const { ".len() + 1;
        let mut diags = vec![id_length(file.to_str().unwrap(), 2, col)];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected `t` to be dropped");
    }

    #[test]
    fn apply_keeps_t_without_i18n_import() {
        let dir = tempdir();
        let file = dir.join("g.tsx");
        std::fs::write(&file, "const t = 1;\n").unwrap();
        let mut diags = vec![id_length(file.to_str().unwrap(), 1, 7)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "expected `t` kept without i18n import");
    }

    #[test]
    fn apply_keeps_other_short_identifier_in_i18n_file() {
        let dir = tempdir();
        let file = dir.join("h.tsx");
        std::fs::write(
            &file,
            "import { useTranslation } from \"react-i18next\";\nconst x = 1;\n",
        )
        .unwrap();
        let mut diags = vec![id_length(file.to_str().unwrap(), 2, 7)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "only `t` is exempted, not `x`");
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "comply-id-length-i18n-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
