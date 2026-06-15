//! typescript-eslint plugin rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::backend::{Backend, PostFilter};
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_delegate};
use std::sync::Arc;

pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry(
            "typescript/no-explicit-any",
            "typescript/no-explicit-any",
            Severity::Error,
            "Using `any` defeats the type system.",
            "Replace `any` with a concrete type. When the shape is genuinely \
             unknown at the boundary, use `unknown` and narrow it before use.",
        ),
        entry_with_filter(
            "typescript/no-unsafe-type-assertion",
            "typescript/no-unsafe-type-assertion",
            Severity::Error,
            "Unsafe `as` assertions bypass the type checker.",
            "Replace the assertion with a proper type guard or narrow via \
             runtime validation before treating the value as the target type.",
            Some(Arc::new(NoUnsafeTypeAssertionFilter)),
        ),
        entry(
            "typescript/array-type",
            "typescript/array-type",
            Severity::Error,
            "Use `T[]` consistently for arrays.",
            "Prefer `T[]` over `Array<T>`. Mixing the two styles creates \
             pointless review churn.",
        ),
        entry(
            "typescript/consistent-type-imports",
            "typescript/consistent-type-imports",
            Severity::Error,
            "Import types with `import type` so the bundler can strip them.",
            "Prefix the import with `import type` when every binding is only \
             used as a type. This lets the bundler elide the import entirely.",
        ),
        entry(
            "typescript/prefer-as-const",
            "typescript/prefer-as-const",
            Severity::Error,
            "Use `as const` to pin literal types.",
            "Replace `as 'literal'` with `as const` — more concise and \
             preserves the literal type across refactors.",
        ),
        entry(
            "typescript/prefer-ts-expect-error",
            "typescript/prefer-ts-expect-error",
            Severity::Error,
            "Use `@ts-expect-error` instead of `@ts-ignore`.",
            "Replace `@ts-ignore` with `@ts-expect-error`. The latter errors \
             when the suppressed issue is fixed, preventing bit-rot.",
        ),
        entry(
            "typescript/no-unsafe-function-type",
            "typescript/no-unsafe-function-type",
            Severity::Error,
            "The bare `Function` type accepts any signature.",
            "Replace `Function` with a specific function type like \
             `(arg: X) => Y`. Bare `Function` offers no type safety.",
        ),
        entry(
            "typescript/no-require-imports",
            "typescript/no-require-imports",
            Severity::Error,
            "Use ES module imports, not CommonJS `require`.",
            "Replace `const x = require('x')` with `import x from 'x'`. \
             require() bypasses the type system and tree-shaking.",
        ),
        entry(
            "typescript/explicit-member-accessibility",
            "typescript/explicit-member-accessibility",
            Severity::Warning,
            "Class members should declare their accessibility explicitly.",
            "Add an explicit `public`/`private`/`protected` modifier to \
             class properties and methods. Stating visibility documents the \
             intended API surface.",
        ),
    ]
}

// Entry-builder helper used by `register_all` above.

fn entry(
    id: &'static str,
    oxlint_key: &'static str,
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    oxlint_delegate(
        RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
            categories: &["typescript"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        oxlint_key,
        TS_FAMILY,
    )
}

fn entry_with_filter(
    id: &'static str,
    oxlint_key: &'static str,
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
    post_filter: Option<Arc<dyn PostFilter>>,
) -> RuleDef {
    RuleDef {
        meta: RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
            categories: &["typescript"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxlint { rule: oxlint_key, post_filter: post_filter.as_ref().map(Arc::clone) }))
            .collect(),
    }
}

// ── typescript/no-unsafe-type-assertion post-filter ────────────────────────
//
// Composite filter covering four FP shapes for this rule:
// 1. Assertion *from* `any` to a concrete type — adds type info, never unsafe.
//    (Closes #572)
// 2. Test files — idiomatic stubs and mock casts, same exemption as native
//    assertion rules. (Closes #573)
// 3. CSS custom properties cast to `React.CSSProperties` — required because
//    @types/react@19 removed the index signature. (Closes #569)
// 4. Generic-parameter bridge cast (`field as Path<TFields>`) — structural
//    bridge from string to a type-level union. (Closes #571)

struct NoUnsafeTypeAssertionFilter;

impl PostFilter for NoUnsafeTypeAssertionFilter {
    fn keep(&self, diag: &crate::diagnostic::Diagnostic, source: Option<&str>) -> bool {
        if is_assertion_from_any(&diag.message) {
            return false;
        }
        if is_nuta_test_path(&diag.path) {
            return false;
        }
        if let Some(src) = source {
            if is_css_custom_prop_cast_fp(src, diag.line) {
                return false;
            }
            if is_generic_param_bridge_cast(src, diag.line) {
                return false;
            }
        }
        true
    }
}

fn is_assertion_from_any(message: &str) -> bool {
    let Some(rest) = message.strip_prefix("Unsafe assertion from ") else {
        return false;
    };
    let Some(end) = rest.find(" detected") else {
        return false;
    };
    rest[..end].trim().trim_matches('`').trim() == "any"
}

fn is_nuta_test_path(path: &std::path::Path) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/");
    lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains("/__tests__/")
        || lower.starts_with("__tests__/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.starts_with("tests/")
        || lower.starts_with("test/")
}

fn is_css_custom_prop_cast_fp(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    let start = line_1based - 1;
    let end = (line_1based + 14).min(lines.len());
    let window = lines[start..end].join("\n");
    window.contains("as React.CSSProperties")
        && (window.contains("\"--") || window.contains("'--"))
}

fn is_generic_param_bridge_cast(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    line_has_generic_param_bridge_cast(lines[line_1based - 1])
}

fn line_has_generic_param_bridge_cast(line: &str) -> bool {
    let mut from = 0;
    while let Some(rel) = line[from..].find(" as ") {
        let after_as = line[from + rel + 4..].trim_start();
        let cut = after_as
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(after_as.len());
        let after_ident = after_as[cut..].trim_start();
        if let Some(inner) = balanced_angle_inner(after_ident) {
            if split_top_level(inner).iter().any(|a| is_generic_param_name(a)) {
                return true;
            }
        }
        from += rel + 4;
    }
    false
}

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
    use crate::diagnostic::{Diagnostic, Severity};
    use std::borrow::Cow;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn diag_msg(path: &Path, message: &str) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line: 1,
            column: 1,
            rule_id: Cow::Borrowed("typescript/no-unsafe-type-assertion"),
            message: message.to_string(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn diag_at(path: &Path, line: usize) -> Diagnostic {
        Diagnostic {
            path: Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed("typescript/no-unsafe-type-assertion"),
            message: "Unsafe assertion to Foo detected.".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-no-unsafe-type-assertion-filter-tests");
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

    const NUTA: &str = "typescript/no-unsafe-type-assertion";
    const f: NoUnsafeTypeAssertionFilter = NoUnsafeTypeAssertionFilter;

    // ── from-any (Closes #572) ────────────────────────────────────────────

    #[test]
    fn drops_assertion_from_any() {
        let d = diag_msg(
            Path::new("/tmp/x.ts"),
            "Unsafe assertion from any detected: consider using type guards.",
        );
        assert!(!f.keep(&d, None));
    }

    #[test]
    fn keeps_assertion_from_non_any() {
        let d = diag_msg(
            Path::new("/tmp/x.ts"),
            "Unsafe assertion from string detected: consider using type guards.",
        );
        assert!(f.keep(&d, None));
    }

    #[test]
    fn keeps_assertion_to_unsafe() {
        let d = diag_msg(
            Path::new("/tmp/x.ts"),
            "Unsafe assertion to Foo detected: consider a more specific type.",
        );
        assert!(f.keep(&d, None));
    }

    // ── test files (Closes #573) ──────────────────────────────────────────

    #[test]
    fn drops_in_test_file() {
        let d = diag_at(Path::new("src/api/foo.test.ts"), 1);
        assert!(!f.keep(&d, None));
    }

    #[test]
    fn keeps_in_production_file() {
        let d = diag_at(Path::new("src/api/foo.ts"), 1);
        // No source needed — test-path check comes first.
        assert!(f.keep(&d, None));
    }

    // ── CSS custom properties (Closes #569) ──────────────────────────────

    #[test]
    fn drops_css_custom_prop_cast() {
        let src = "const style = { \"--my-var\": \"100px\" } as React.CSSProperties;\n";
        let path = write_temp("css_cast.ts", src);
        let line = line_of(src, "\"--my-var\"");
        let src_content = std::fs::read_to_string(&path).unwrap();
        let d = diag_at(&path, line);
        assert!(!f.keep(&d, Some(&src_content)));
    }

    #[test]
    fn keeps_cast_without_css_custom_props() {
        let src = "const x = { color: \"red\" } as React.CSSProperties;\n";
        let path = write_temp("no_css_custom.ts", src);
        let src_content = std::fs::read_to_string(&path).unwrap();
        let d = diag_at(&path, 1);
        assert!(f.keep(&d, Some(&src_content)));
    }

    // ── generic-parameter bridge (Closes #571) ───────────────────────────

    #[test]
    fn drops_generic_bridge_cast() {
        let src = "setError(field as Path<TFields>, { message });\n";
        let path = write_temp("generic_bridge.ts", src);
        let src_content = std::fs::read_to_string(&path).unwrap();
        let d = diag_at(&path, 1);
        assert!(!f.keep(&d, Some(&src_content)));
    }

    #[test]
    fn keeps_cast_with_concrete_generic_arg() {
        let src = "const p = field as Path<string>;\n";
        let path = write_temp("concrete_generic.ts", src);
        let src_content = std::fs::read_to_string(&path).unwrap();
        let d = diag_at(&path, 1);
        assert!(f.keep(&d, Some(&src_content)));
    }
}
