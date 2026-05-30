//! Post-filter for `typescript/no-unsafe-type-assertion` false positives when
//! casting an object with CSS custom properties to `React.CSSProperties`.
//!
//! `@types/react@19` removed the index signature from `React.CSSProperties`
//! intentionally ("The index signature was removed to enable closed typing for
//! style using CSSType"). Consequently `satisfies React.CSSProperties` fails to
//! compile when any key starts with `--`, making `as React.CSSProperties` the
//! only valid approach — and one explicitly documented by @types/react itself.
//!
//! Drop `typescript/no-unsafe-type-assertion` diagnostics whose source window
//! (the diagnostic line and the following 14 lines) contains both
//! `as React.CSSProperties` and a CSS custom property key (`"--` or `'--`).

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "typescript/no-unsafe-type-assertion" {
            return true;
        }
        let entry = file_cache
            .entry(d.path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !is_css_custom_prop_cast_fp(src, d.line)
    });
}

/// True when the source window around `line_1based` contains both
/// `as React.CSSProperties` and a CSS custom property key.
fn is_css_custom_prop_cast_fp(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    // Window: diagnostic line + up to 14 lines ahead (the `as React.CSSProperties`
    // closing is typically a few lines after the opening `{`).
    let start = line_1based - 1;
    let end = (line_1based + 14).min(lines.len());
    let window = lines[start..end].join("\n");

    window.contains("as React.CSSProperties")
        && (window.contains("\"--") || window.contains("'--"))
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
            column: 3,
            rule_id: Cow::Borrowed("typescript/no-unsafe-type-assertion"),
            message: "Unsafe assertion to error typed detected.".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("comply-no-unsafe-type-assertion-css-post-filter-tests");
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

    // Regression test for #569: single-line case.
    #[test]
    fn drops_single_line_css_custom_prop_cast() {
        let src = r#"import type React from 'react';
const style = { "--my-var": "100px" } as React.CSSProperties;
"#;
        let path = write_temp("drops_single_line.ts", src);
        let line = line_of(src, "\"--my-var\"");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    // Regression test for #569: multi-line JSX case from sidebar.tsx.
    #[test]
    fn drops_multiline_jsx_css_custom_prop_cast() {
        let src = r#"import type React from 'react';
function Sidebar({ style }: { style?: React.CSSProperties }) {
  return (
    <div
      style={
        {
          "--sidebar-width": "200px",
          "--sidebar-width-icon": "48px",
          ...style,
        } as React.CSSProperties
      }
    />
  );
}
"#;
        let path = write_temp("drops_multiline.tsx", src);
        let line = line_of(src, "\"--sidebar-width\":");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn keeps_unsafe_assertion_without_css_custom_props() {
        let src = r#"const x = { color: "red" } as React.CSSProperties;
"#;
        let path = write_temp("keeps_no_custom_props.ts", src);
        let line = line_of(src, "color");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_unsafe_assertion_to_other_type() {
        let src = r#"const x = { "--my-var": "100px" } as SomeOtherType;
"#;
        let path = write_temp("keeps_other_type.ts", src);
        let line = line_of(src, "\"--my-var\"");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = r#"const style = { "--my-var": "100px" } as React.CSSProperties;
"#;
        let path = write_temp("other_rule.ts", src);
        let line = line_of(src, "\"--my-var\"");
        let mut diags = vec![
            Diagnostic {
                path: std::sync::Arc::from(path.as_path()),
                line,
                column: 1,
                rule_id: Cow::Borrowed("typescript/no-unsafe-type-assertion"),
                message: String::new(),
                severity: Severity::Error,
                span: None,
            },
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
        assert_eq!(diags.len(), 1, "expected only no-explicit-any to remain");
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent =
            std::env::temp_dir().join("does-not-exist-comply-no-unsafe-css-test.ts");
        let mut diags = vec![fake_diag(&nonexistent, 1)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }
}
