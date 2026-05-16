//! Post-filter for `no-unnecessary-condition` false positives in Elysia
//! lifecycle hooks.
//!
//! Elysia's plugin system types fields added via `.derive` / `.resolve` as
//! always-set on the request context. At runtime, framework-level errors
//! (PARSE, VALIDATION, NOT_FOUND) short-circuit the request lifecycle before
//! `.derive` runs. `.mapResponse`, `.onError`, `.afterHandle` and friends
//! still fire for those responses, so any `.derive` field destructured from
//! the context is runtime-`undefined` on those paths even though TypeScript
//! believes it is set.
//!
//! Drop `no-unnecessary-condition` diagnostics whose source line contains a
//! nullish-coalesce (`??`) inside an Elysia lifecycle-hook callback, in a
//! file that imports Elysia. The check is intentionally narrow: only `??`
//! lines, only inside a hook-callback opener, only in Elysia-importing files.

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

const ELYSIA_HOOK_OPENERS: &[&str] = &[
    ".mapResponse(",
    ".onError(",
    ".onResponse(",
    ".onAfterResponse(",
    ".onRequest(",
    ".onTransform(",
    ".onParse(",
    ".onBeforeHandle(",
    ".onAfterHandle(",
    ".beforeHandle(",
    ".afterHandle(",
];

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "no-unnecessary-condition" {
            return true;
        }
        let path: &Path = &d.path;
        let entry = file_cache
            .entry(path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(path).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !is_elysia_lifecycle_nullish_fp(src, d.line)
    });
}

/// True when the diagnostic at `line_1based` is a `??` inside an Elysia
/// lifecycle-hook callback in a file that imports Elysia.
fn is_elysia_lifecycle_nullish_fp(src: &str, line_1based: usize) -> bool {
    if !imports_elysia(src) {
        return false;
    }
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    let line_text = lines[line_1based - 1];
    if !line_text.contains("??") {
        return false;
    }
    // Walk backwards up to 100 lines looking for a hook-opener. The bound
    // is enough for any realistic callback body without scanning the entire
    // file for unrelated `??` lines.
    let start = line_1based.saturating_sub(100).max(1);
    for i in (start..line_1based).rev() {
        let l = lines[i - 1];
        if ELYSIA_HOOK_OPENERS.iter().any(|h| l.contains(h)) {
            return true;
        }
    }
    false
}

fn imports_elysia(src: &str) -> bool {
    src.contains("from \"elysia\"") || src.contains("from 'elysia'")
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
        let dir = std::env::temp_dir().join("comply-elysia-post-filter-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, src).unwrap();
        path
    }

    #[test]
    fn drops_map_response_nullish_in_elysia_file() {
        let src = r#"import { Elysia } from "elysia";
import { uuidv7 } from "uuidv7";

const UNKNOWN_REQUEST_ID = "unknown";

new Elysia()
  .derive({ as: "global" }, () => ({ requestId: uuidv7() }))
  .mapResponse({ as: "global" }, ({ requestId, set }) => {
    set.headers["x-request-id"] = requestId ?? UNKNOWN_REQUEST_ID;
  });
"#;
        let path = write_temp("drops_map_response.ts", src);
        let line = line_of(src, "requestId ?? UNKNOWN_REQUEST_ID");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-condition")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    #[test]
    fn drops_on_error_nullish_in_elysia_file() {
        let src = r#"import { Elysia } from "elysia";
const UNKNOWN_REQUEST_ID = "unknown";
new Elysia()
  .derive({ as: "global" }, () => ({ requestId: "rid" }))
  .onError(({ requestId, error }) => {
    return { id: requestId ?? UNKNOWN_REQUEST_ID, error: error.message };
  });
"#;
        let path = write_temp("drops_on_error.ts", src);
        let line = line_of(src, "requestId ?? UNKNOWN_REQUEST_ID");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-condition")];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn keeps_nullish_outside_lifecycle_hook() {
        // `??` in an Elysia file but not inside a lifecycle hook callback —
        // the runtime-undefined argument does not apply; keep the diagnostic.
        let src = r#"import { Elysia } from "elysia";
const x: string = "set";
const y = x ?? "fallback";
new Elysia();
"#;
        let path = write_temp("keeps_outside_hook.ts", src);
        let line = line_of(src, "?? \"fallback\"");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-condition")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_nullish_in_non_elysia_file() {
        // Same shape as the FP but the file doesn't import Elysia — keep.
        let src = r#"const x: string = "set";
function handle({ requestId }: { requestId: string }) {
  return requestId ?? "unknown";
}
"#;
        let path = write_temp("keeps_non_elysia.ts", src);
        let line = line_of(src, "requestId ?? \"unknown\"");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-condition")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_touch_other_rules() {
        let src = r#"import { Elysia } from "elysia";
new Elysia()
  .mapResponse(({ requestId }) => {
    return requestId ?? "unknown";
  });
"#;
        let path = write_temp("other_rule.ts", src);
        let line = line_of(src, "requestId ?? \"unknown\"");
        let mut diags = vec![
            fake_diag(&path, line, "no-unnecessary-condition"),
            fake_diag(&path, line, "no-explicit-any"),
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "expected only no-explicit-any to remain");
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        // If the source file can't be read, keep the diagnostic (the filter
        // is best-effort and must never silently drop on I/O errors).
        let nonexistent = std::env::temp_dir().join("does-not-exist-comply-test.ts");
        let mut diags = vec![fake_diag(&nonexistent, 42, "no-unnecessary-condition")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }
}
