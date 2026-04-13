use crate::diagnostic::{Diagnostic, Severity};

/// Window-specific APIs that should remain as `window.X`.
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

            let preceded_by_ident = abs > 0 && is_ident_char(bytes[abs - 1]);
            let dot_pos = abs + name.len();

            let followed_by_prop = dot_pos + 1 < bytes.len() && is_ident_char(bytes[dot_pos + 1]);

            if !preceded_by_ident && followed_by_prop {
                if name == "window" && is_window_specific_api(line, dot_pos) {
                    start = abs + pattern.len();
                    continue;
                }

                let before = line[..abs].trim_end();
                if before.ends_with("typeof") {
                    start = abs + pattern.len();
                    continue;
                }

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
                break;
            }
            start = abs + pattern.len();
        }
    }

    violations
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in src.lines().enumerate() {
        for msg in find_global_this_violations(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "prefer-global-this".into(),
                message: msg,
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_window_location() {
        let d = run_ts("const url = window.location;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_self_in_worker() {
        let d = run_ts("self.fetch('/api');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_global_process() {
        let d = run_ts("const env = global.process.env;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn allows_global_this() {
        assert!(run_ts("const url = globalThis.location;").is_empty());
    }

    #[test]
    fn allows_window_specific_close() {
        assert!(run_ts("window.close();").is_empty());
    }

    #[test]
    fn allows_window_specific_inner_width() {
        assert!(run_ts("const w = window.innerWidth;").is_empty());
    }

    #[test]
    fn ignores_typeof_window() {
        assert!(run_ts("if (typeof window.x !== 'undefined') {}").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run_ts("// window.location is the URL").is_empty());
    }
}
