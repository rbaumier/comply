//! Post-filter for `typescript/no-unsafe-type-assertion` false positives when
//! casting a runtime value to a generic utility type parameterized by an
//! in-scope generic type parameter, e.g. React Hook Form's
//! `field as Path<TFields>`.
//!
//! `Path<TFields>` is a compile-time, recursive string-literal union derived
//! from the generic field shape. Field names arrive from the backend as plain
//! `string` at runtime; there is no value-level narrowing to such a type-level
//! union, so `as Path<TFields>` is the canonical structural bridge (RHF's own
//! docs use it). This mirrors the native `no-type-assertion` exemption for
//! `expr as Foo<TParam>`. (Closes #571)
//!
//! Drop `typescript/no-unsafe-type-assertion` diagnostics whose source line is
//! a cast to `Ident<…>` where a top-level type argument matches the generic-
//! parameter naming convention (a single uppercase letter, or `T`-prefixed
//! PascalCase such as `TFields`/`TData`).

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
        !is_generic_param_bridge_cast(src, d.line)
    });
}

fn is_generic_param_bridge_cast(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    line_has_generic_param_bridge_cast(lines[line_1based - 1])
}

/// True when the line contains a cast `… as Ident<…>` whose type arguments
/// include an in-scope-looking generic parameter.
fn line_has_generic_param_bridge_cast(line: &str) -> bool {
    let mut from = 0;
    while let Some(rel) = line[from..].find(" as ") {
        let after_as = line[from + rel + 4..].trim_start();
        // Skip the (possibly dotted) type-name identifier.
        let cut = after_as
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(after_as.len());
        let after_ident = after_as[cut..].trim_start();
        if let Some(inner) = balanced_angle_inner(after_ident)
            && split_top_level(inner).iter().any(|a| is_generic_param_name(a))
        {
            return true;
        }
        from += rel + 4;
    }
    false
}

/// Given a string starting at `<`, return the content up to the matching `>`.
fn balanced_angle_inner(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[1..i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a type-argument list on top-level commas (ignoring nested brackets).
fn split_top_level(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '<' | '(' | '[' | '{' => depth += 1,
            '>' | ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&inner[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

/// True for generic-parameter-style names: a single uppercase letter (`T`,
/// `K`, `U`, …) or `T`-prefixed PascalCase (`TFields`, `TData`).
fn is_generic_param_name(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_uppercase() {
        return false;
    }
    if s.len() == 1 {
        return true;
    }
    first == 'T' && chars.next().is_some_and(|c| c.is_ascii_uppercase())
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
            column: 12,
            rule_id: Cow::Borrowed("typescript/no-unsafe-type-assertion"),
            message: "Unsafe assertion to error typed detected.".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-no-unsafe-generic-bridge-post-filter-tests");
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

    // Regression for #571: RHF `field as Path<TFields>`.
    #[test]
    fn drops_rhf_path_generic_bridge() {
        let src = "setError(field as Path<TFields>, { message });\n";
        let path = write_temp("drops_rhf.ts", src);
        let line = line_of(src, "as Path");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn drops_single_letter_generic_arg() {
        let src = "const p = field as Path<T>;\n";
        let path = write_temp("drops_single_letter.ts", src);
        let line = line_of(src, "as Path");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn keeps_cast_to_generic_with_concrete_arg() {
        // `as Path<string>` / `as Foo<Bar>` are not generic-param bridges.
        let src = "const p = field as Path<string>;\n";
        let path = write_temp("keeps_concrete.ts", src);
        let line = line_of(src, "as Path");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_cast_to_pascal_concrete_arg() {
        let src = "const p = field as Box<Bar>;\n";
        let path = write_temp("keeps_pascal.ts", src);
        let line = line_of(src, "as Box");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_plain_cast_without_generics() {
        let src = "const x = foo as SomeType;\n";
        let path = write_temp("keeps_plain.ts", src);
        let line = line_of(src, "as SomeType");
        let mut diags = vec![fake_diag(&path, line)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = "const p = field as Path<TFields>;\n";
        let path = write_temp("other_rule.ts", src);
        let line = line_of(src, "as Path");
        let mut diags = vec![Diagnostic {
            path: std::sync::Arc::from(path.as_path()),
            line,
            column: 1,
            rule_id: Cow::Borrowed("no-explicit-any"),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent =
            std::env::temp_dir().join("does-not-exist-comply-generic-bridge.ts");
        let mut diags = vec![fake_diag(&nonexistent, 1)];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }
}
