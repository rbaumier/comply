//! Post-filter for `no-misused-spread` false positives when spreading a class
//! instance into a plain object passed to a library error constructor.
//!
//! Better Auth's `APIError(code, body)` (and similar library error
//! constructors) take a plain `Record<string, unknown>` body. Spreading our
//! typed error class instance into `{ ...apiError }` deliberately forwards its
//! enumerable data fields to that constructor — the prototype methods are
//! irrelevant here and the library does not accept the typed instance.
//! tsgolint correctly sees a class-instance spread, but in this interop context
//! it is intentional. (Closes #554)
//!
//! Drop `no-misused-spread` diagnostics whose source window (a few lines back
//! to one line ahead) shows an object spread forwarded into a `new <X>Error(...)`
//! constructor call.

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "no-misused-spread" {
            return true;
        }
        let entry = file_cache
            .entry(d.path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !is_error_constructor_interop_spread(src, d.line)
    });
}

/// True when the diagnostic line holds an object spread (`...`) and the
/// surrounding window forwards it into a `new <Something>Error(...)` call.
fn is_error_constructor_interop_spread(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    // The spread itself must be on the reported line.
    if !lines[line_1based - 1].contains("...") {
        return false;
    }
    // The `new <X>Error(` opening is on the reported line or a few lines above.
    let start = line_1based.saturating_sub(8);
    let end = (line_1based + 1).min(lines.len());
    let window = lines[start..end].join("\n");
    window.contains("new ") && window.contains("Error(")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use std::borrow::Cow;
    use std::path::{Path, PathBuf};

    fn fake_diag(path: &Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 5,
            rule_id: Cow::Borrowed("no-misused-spread"),
            message: "Using the spread operator on a class instance loses methods.".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-no-misused-spread-post-filter-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, src).unwrap();
        path
    }

    fn line_of(src: &str, needle: &str) -> usize {
        src.lines()
            .enumerate()
            .find(|(_, l)| l.contains(needle))
            .map(|(i, _)| i + 1)
            .expect("needle not in source")
    }

    // Regression for #554: spread forwarded to Better Auth's APIError constructor.
    #[test]
    fn drops_spread_into_error_constructor() {
        let src = r#"
            throw new APIError(
                apiError.status === 403 ? 'FORBIDDEN' : 'INTERNAL_SERVER_ERROR',
                { ...apiError },
            );
        "#;
        let path = write_temp("drops_error_interop.ts", src);
        let line = line_of(src, "{ ...apiError }");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn drops_all_duplicate_diagnostics_on_same_line() {
        // tsgolint reports once per spread property — all must be dropped.
        let src = r#"
            throw new APIError('FORBIDDEN', { ...apiError });
        "#;
        let path = write_temp("drops_dupes.ts", src);
        let line = line_of(src, "...apiError");
        let mut diags = vec![fake_diag(&path, line), fake_diag(&path, line), fake_diag(&path, line)];
        apply(&mut diags);
        assert!(diags.is_empty(), "all duplicates must be dropped, got: {diags:?}");
    }

    #[test]
    fn keeps_spread_not_into_error_constructor() {
        let src = r#"
            const merged = { ...someClassInstance };
        "#;
        let path = write_temp("keeps_plain_spread.ts", src);
        let line = line_of(src, "...someClassInstance");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "plain class-instance spread must still flag");
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = r#"
            throw new APIError('X', { ...apiError });
        "#;
        let path = write_temp("other_rule_spread.ts", src);
        let line = line_of(src, "...apiError");
        let mut diags = vec![
            fake_diag(&path, line),
            Diagnostic {
                path: std::sync::Arc::from(path.as_path()),
                line,
                column: 1,
                rule_id: Cow::Borrowed("no-explicit-any"),
                message: String::new(),
                severity: Severity::Error,
                span: None,
            },
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "only no-explicit-any should remain");
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent = std::env::temp_dir().join("does-not-exist-comply-misused-spread.ts");
        let mut diags = vec![fake_diag(&nonexistent, 1)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }
}
