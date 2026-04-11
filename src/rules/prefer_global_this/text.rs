use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Window-specific APIs that should remain as `window.X`.
/// These are browser APIs that are conceptually tied to the window object.
const WINDOW_SPECIFIC: &[&str] = &[
    "close",
    "closed",
    "stop",
    "focus",
    "blur",
    "frames",
    "length",
    "top",
    "opener",
    "parent",
    "frameElement",
    "open",
    "postMessage",
    "navigation",
    "name",
    "locationbar",
    "menubar",
    "personalbar",
    "scrollbars",
    "statusbar",
    "toolbar",
    "status",
    "originAgentCluster",
    // CSSOM View
    "screen",
    "visualViewport",
    "moveTo",
    "moveBy",
    "resizeTo",
    "resizeBy",
    "innerWidth",
    "innerHeight",
    "outerWidth",
    "outerHeight",
    "scrollX",
    "pageXOffset",
    "scrollY",
    "pageYOffset",
    "scroll",
    "scrollTo",
    "scrollBy",
    "screenX",
    "screenLeft",
    "screenY",
    "screenTop",
    "devicePixelRatio",
    // Events
    "addEventListener",
    "removeEventListener",
    "dispatchEvent",
    "onresize",
    "onblur",
    "onfocus",
    "onload",
    "onscroll",
    "onbeforeunload",
    "onmessage",
    "onpagehide",
    "onpageshow",
    "onunload",
];

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// Check if a `window.X` usage is window-specific (should NOT be flagged).
fn is_window_specific_api(line: &str, dot_pos: usize) -> bool {
    let after = &line[dot_pos + 1..];
    // Extract the property name after the dot
    let prop: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    WINDOW_SPECIFIC.iter().any(|&api| api == prop)
}

/// Find usages of `window.`, `self.`, or `global.` that should be `globalThis.`.
fn find_global_this_violations(line: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let globals = &["window", "self", "global"];

    for &name in globals {
        let pattern = format!("{name}.");
        let bytes = line.as_bytes();
        let mut start = 0;

        while start + pattern.len() <= bytes.len() {
            let Some(rel) = line[start..].find(&pattern) else {
                break;
            };
            let abs = start + rel;

            // Must not be part of a larger identifier
            let preceded_by_ident = abs > 0 && is_ident_char(bytes[abs - 1]);
            let dot_pos = abs + name.len();

            // After `name.`, there should be an identifier char (property access)
            let followed_by_prop = dot_pos + 1 < bytes.len() && is_ident_char(bytes[dot_pos + 1]);

            if !preceded_by_ident && followed_by_prop {
                // Skip window-specific APIs
                if name == "window" && is_window_specific_api(line, dot_pos) {
                    start = abs + pattern.len();
                    continue;
                }

                // Skip `typeof window` / `typeof self` / `typeof global`
                let before = line[..abs].trim_end();
                if before.ends_with("typeof") {
                    start = abs + pattern.len();
                    continue;
                }

                // Skip comments
                let trimmed = line.trim_start();
                if trimmed.starts_with("//")
                    || trimmed.starts_with('*')
                    || trimmed.starts_with("/*")
                {
                    break;
                }

                violations.push(format!(
                    "Prefer `globalThis` over `{name}`. Replace `{name}.` with `globalThis.`."
                ));
                break; // one diagnostic per global per line
            }
            start = abs + pattern.len();
        }
    }

    violations
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for msg in find_global_this_violations(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-global-this".into(),
                    message: msg,
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_window_location() {
        let d = run("const url = window.location;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_self_postmessage_in_worker() {
        // In a worker context, `self.fetch` should be `globalThis.fetch`
        let d = run("self.fetch('/api');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_global_process() {
        let d = run("const env = global.process.env;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn allows_global_this() {
        assert!(run("const url = globalThis.location;").is_empty());
    }

    #[test]
    fn allows_window_specific_close() {
        assert!(run("window.close();").is_empty());
    }

    #[test]
    fn allows_window_specific_inner_width() {
        assert!(run("const w = window.innerWidth;").is_empty());
    }

    #[test]
    fn ignores_typeof_window() {
        assert!(run("if (typeof window.x !== 'undefined') {}").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// window.location is the URL").is_empty());
    }

    #[test]
    fn does_not_flag_partial_match() {
        // `mywindow.foo` should not be flagged
        assert!(run("mywindow.foo = 1;").is_empty());
    }
}
