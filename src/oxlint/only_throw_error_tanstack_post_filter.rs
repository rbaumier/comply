//! Post-filter for `only-throw-error` false positives on TanStack Router
//! control-flow markers.
//!
//! TanStack Router's `notFound()` and `redirect()` return marker objects
//! (`NotFoundError` / `Redirect`), not `Error` subclasses. The router
//! intercepts these objects when thrown — `throw notFound({ routeId })` and
//! `throw redirect({ to })` are the framework's documented control-flow idiom.
//!
//! Drop `only-throw-error` diagnostics whose source line contains `throw` with
//! a call to `notFound(` or `redirect(`, in a file that imports from
//! `@tanstack/react-router` or `@tanstack/router-core`.

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "only-throw-error" {
            return true;
        }
        let path: &Path = &d.path;
        let entry = file_cache
            .entry(path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(path).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !is_tanstack_control_flow_fp(src, d.line)
    });
}

/// True when the diagnostic at `line_1based` is a `throw notFound(` or
/// `throw redirect(` in a file that imports from a TanStack Router package.
fn is_tanstack_control_flow_fp(src: &str, line_1based: usize) -> bool {
    if !imports_tanstack_router(src) {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    let line_text = lines[line_1based - 1];
    line_text.contains("throw")
        && (line_text.contains("notFound(") || line_text.contains("redirect("))
}

fn imports_tanstack_router(src: &str) -> bool {
    src.contains("from \"@tanstack/react-router\"")
        || src.contains("from '@tanstack/react-router'")
        || src.contains("from \"@tanstack/router-core\"")
        || src.contains("from '@tanstack/router-core'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn line_of(src: &str, needle: &str) -> usize {
        src.lines()
            .enumerate()
            .find(|(_, l)| l.contains(needle))
            .map(|(i, _)| i + 1)
            .expect("needle not in source")
    }

    fn fake_diag(path: &Path, line: usize, rule: &'static str) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed(rule),
            message: String::new(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-tanstack-throw-filter-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, src).unwrap();
        path
    }

    #[test]
    fn drops_not_found_in_tanstack_file() {
        let src = r#"import { notFound } from "@tanstack/react-router";
export function loader({ params }) {
  if (params.id === null) {
    throw notFound({ routeId: "/detail" });
  }
  return params.id;
}
"#;
        let path = write_temp("drops_not_found.ts", src);
        let line = line_of(src, "throw notFound(");
        let mut diags = vec![fake_diag(&path, line, "only-throw-error")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn drops_redirect_in_tanstack_file() {
        let src = r#"import { redirect } from "@tanstack/react-router";
export function loader({ params }) {
  if (!params.id) {
    throw redirect({ to: "/login" });
  }
  return params.id;
}
"#;
        let path = write_temp("drops_redirect.ts", src);
        let line = line_of(src, "throw redirect(");
        let mut diags = vec![fake_diag(&path, line, "only-throw-error")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn drops_with_router_core_import() {
        let src = r#"import { notFound, redirect } from "@tanstack/router-core";
export function action() {
  throw redirect({ to: "/home" });
}
"#;
        let path = write_temp("drops_router_core.ts", src);
        let line = line_of(src, "throw redirect(");
        let mut diags = vec![fake_diag(&path, line, "only-throw-error")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn keeps_throw_string_literal() {
        let src = r#"import { notFound } from "@tanstack/react-router";
export function loader() {
  throw "boom";
}
"#;
        let path = write_temp("keeps_throw_string.ts", src);
        let line = line_of(src, "throw \"boom\"");
        let mut diags = vec![fake_diag(&path, line, "only-throw-error")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_local_not_found_without_tanstack_import() {
        let src = r#"function notFound(opts) { return { type: "not-found", ...opts }; }
export function loader({ params }) {
  throw notFound({ routeId: "/detail" });
}
"#;
        let path = write_temp("keeps_local_not_found.ts", src);
        let line = line_of(src, "throw notFound(");
        let mut diags = vec![fake_diag(&path, line, "only-throw-error")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent =
            std::env::temp_dir().join("does-not-exist-comply-tanstack-throw-test.ts");
        let mut diags = vec![fake_diag(&nonexistent, 42, "only-throw-error")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = r#"import { notFound } from "@tanstack/react-router";
export function loader() {
  throw notFound({ routeId: "/x" });
}
"#;
        let path = write_temp("other_rule_tanstack.ts", src);
        let line = line_of(src, "throw notFound(");
        let mut diags = vec![
            fake_diag(&path, line, "only-throw-error"),
            fake_diag(&path, line, "no-explicit-any"),
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "expected only no-explicit-any to remain");
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }
}
