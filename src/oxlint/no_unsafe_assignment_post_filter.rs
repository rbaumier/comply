//! Post-filter for `no-unsafe-assignment` false positives when casting to
//! Vite's `PluginOption` type.
//!
//! `rollup-plugin-visualizer` returns Rollup's `Plugin` type, but Vite's
//! `PluginOption` is `Plugin | Plugin[] | false | null | undefined | …`. The
//! assignment is structurally valid at runtime — `PluginOption` explicitly
//! documents Rollup plugins as acceptable values. The FP arises because the
//! two packages ship slightly different versions of Rollup's `Plugin` type
//! definition, causing the type checker to flag the `as PluginOption` cast.
//!
//! Drop `no-unsafe-assignment` diagnostics whose source line contains
//! `as PluginOption` in a file that imports `PluginOption` from `"vite"`.

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "no-unsafe-assignment" {
            return true;
        }
        let entry = file_cache
            .entry(d.path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !is_plugin_option_cast_fp(src, d.line)
    });
}

/// True when the diagnostic line contains `as PluginOption` and the file
/// imports `PluginOption` from `"vite"` or `'vite'`.
fn is_plugin_option_cast_fp(src: &str, line_1based: usize) -> bool {
    if !imports_plugin_option_from_vite(src) {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    lines[line_1based - 1].contains("as PluginOption")
}

fn imports_plugin_option_from_vite(src: &str) -> bool {
    src.lines().any(|line| {
        line.contains("PluginOption") && (line.contains("from \"vite\"") || line.contains("from 'vite'"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use std::path::{Path, PathBuf};

    fn fake_diag(path: &Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 3,
            rule_id: Cow::Borrowed("no-unsafe-assignment"),
            message: "Unsafe assignment of an error typed value.".into(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-no-unsafe-assignment-post-filter-tests");
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

    // Regression test for issue #380: visualizer() as PluginOption
    #[test]
    fn drops_visualizer_as_plugin_option() {
        let src = r#"import visualizer from "rollup-plugin-visualizer";
import type { PluginOption } from "vite";

const plugins: PluginOption[] = [
  visualizer({ open: true }) as PluginOption,
];
"#;
        let path = write_temp("drops_visualizer_as_plugin_option.ts", src);
        let line = line_of(src, "as PluginOption");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn drops_ternary_as_plugin_option() {
        // Original issue #32 reproducer shape
        let src = r#"import { defineConfig, type PluginOption } from "vite";
import { visualizer } from "rollup-plugin-visualizer";

export default defineConfig(() => {
  const analyzePlugin: PluginOption = false ? visualizer({ open: true }) as PluginOption : false;
});
"#;
        let path = write_temp("drops_ternary_as_plugin_option.ts", src);
        let line = line_of(src, "as PluginOption");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn keeps_unsafe_assignment_without_plugin_option() {
        let src = r#"import { something } from "vite";
const x: string = unknownAny as string;
"#;
        let path = write_temp("keeps_no_plugin_option.ts", src);
        let line = line_of(src, "as string");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_as_plugin_option_without_vite_import() {
        // PluginOption is not from vite here — keep the diagnostic.
        let src = r#"import type { PluginOption } from "some-other-lib";
const x = foo() as PluginOption;
"#;
        let path = write_temp("keeps_not_vite_import.ts", src);
        let line = line_of(src, "as PluginOption");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = r#"import type { PluginOption } from "vite";
const x = foo() as PluginOption;
"#;
        let path = write_temp("other_rule.ts", src);
        let line = line_of(src, "as PluginOption");
        let mut diags = vec![
            Diagnostic {
                path: std::sync::Arc::from(path.as_path()),
                line,
                column: 1,
                rule_id: Cow::Borrowed("no-unsafe-assignment"),
                message: String::new(),
                severity: crate::diagnostic::Severity::Error,
                span: None,
            },
            Diagnostic {
                path: std::sync::Arc::from(path.as_path()),
                line,
                column: 1,
                rule_id: Cow::Borrowed("no-explicit-any"),
                message: String::new(),
                severity: crate::diagnostic::Severity::Error,
                span: None,
            },
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "expected only no-explicit-any to remain");
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent = std::env::temp_dir().join("does-not-exist-comply-no-unsafe-test.ts");
        let mut diags = vec![fake_diag(&nonexistent, 1)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }
}
