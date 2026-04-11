use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Known DOM event names (prefixed with "on" in the source).
const ON_EVENTS: &[&str] = &[
    "onclick", "ondblclick", "onmousedown", "onmouseup", "onmouseover",
    "onmouseout", "onmousemove", "onmouseenter", "onmouseleave",
    "onkeydown", "onkeyup", "onkeypress",
    "onfocus", "onblur", "onchange", "oninput", "onsubmit", "onreset",
    "onscroll", "onresize", "onload", "onunload", "onbeforeunload",
    "onerror", "onabort",
    "ondrag", "ondragstart", "ondragend", "ondragover", "ondragenter",
    "ondragleave", "ondrop",
    "ontouchstart", "ontouchend", "ontouchmove", "ontouchcancel",
    "onpointerdown", "onpointerup", "onpointermove", "onpointerover",
    "onpointerout", "onpointerenter", "onpointerleave",
    "oncontextmenu", "onwheel",
    "onanimationstart", "onanimationend", "onanimationiteration",
    "ontransitionend",
    "onmessage", "onclose", "onopen",
    "onhashchange", "onpopstate",
    "onstorage",
    "onselect", "oncopy", "oncut", "onpaste",
];

/// Detects `something.onclick = handler` style assignments.
/// Flags lines like `element.onclick = fn` where `onclick` is a known DOM event.
fn find_on_event_assignment(line: &str) -> Option<&'static str> {
    // Must contain `=` but not `==` or `===`
    let eq_pos = line.find('=')?;
    if line[eq_pos + 1..].starts_with('=') {
        return None;
    }
    // Check for `!=`
    if eq_pos > 0 && line.as_bytes()[eq_pos - 1] == b'!' {
        return None;
    }

    let before_eq = line[..eq_pos].trim_end();

    for &event in ON_EVENTS {
        if before_eq.ends_with(event) {
            let prefix_end = before_eq.len() - event.len();
            // Must be preceded by `.` (member access)
            if prefix_end > 0 && before_eq.as_bytes()[prefix_end - 1] == b'.' {
                return Some(event);
            }
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if let Some(event) = find_on_event_assignment(trimmed) {
                let event_name = &event[2..]; // strip "on"
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-add-event-listener".into(),
                    message: format!(
                        "Prefer `addEventListener('{event_name}', handler)` over `.{event} = handler`."
                    ),
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
    fn flags_onclick_assignment() {
        assert_eq!(run("element.onclick = handler;").len(), 1);
    }

    #[test]
    fn flags_onkeydown_assignment() {
        assert_eq!(run("document.onkeydown = (e) => {};").len(), 1);
    }

    #[test]
    fn allows_add_event_listener() {
        assert!(run("element.addEventListener('click', handler);").is_empty());
    }

    #[test]
    fn allows_equality_check() {
        assert!(run("if (element.onclick === null) {}").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// element.onclick = handler;").is_empty());
    }
}
