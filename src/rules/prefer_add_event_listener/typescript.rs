//! prefer-add-event-listener AST backend — flag `element.onclick = handler` style assignments.

use crate::diagnostic::{Diagnostic, Severity};

/// Known DOM event names (prefixed with "on" in the source).
const ON_EVENTS: &[&str] = &[
    "onclick",
    "ondblclick",
    "onmousedown",
    "onmouseup",
    "onmouseover",
    "onmouseout",
    "onmousemove",
    "onmouseenter",
    "onmouseleave",
    "onkeydown",
    "onkeyup",
    "onkeypress",
    "onfocus",
    "onblur",
    "onchange",
    "oninput",
    "onsubmit",
    "onreset",
    "onscroll",
    "onresize",
    "onload",
    "onunload",
    "onbeforeunload",
    "onerror",
    "onabort",
    "ondrag",
    "ondragstart",
    "ondragend",
    "ondragover",
    "ondragenter",
    "ondragleave",
    "ondrop",
    "ontouchstart",
    "ontouchend",
    "ontouchmove",
    "ontouchcancel",
    "onpointerdown",
    "onpointerup",
    "onpointermove",
    "onpointerover",
    "onpointerout",
    "onpointerenter",
    "onpointerleave",
    "oncontextmenu",
    "onwheel",
    "onanimationstart",
    "onanimationend",
    "onanimationiteration",
    "ontransitionend",
    "onmessage",
    "onclose",
    "onopen",
    "onhashchange",
    "onpopstate",
    "onstorage",
    "onselect",
    "oncopy",
    "oncut",
    "onpaste",
];

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
    let Some(left) = node.child_by_field_name("left") else { return };
    if left.kind() != "member_expression" {
        return;
    }

    let Some(prop) = left.child_by_field_name("property") else { return };
    let prop_name = prop.utf8_text(source).unwrap_or("");

    if !ON_EVENTS.contains(&prop_name) {
        return;
    }

    let event_name = &prop_name[2..]; // strip "on"
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-add-event-listener".into(),
        message: format!(
            "Prefer `addEventListener('{event_name}', handler)` over `.{prop_name} = handler`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_onclick_assignment() {
        let d = run_on("element.onclick = handler;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-add-event-listener");
    }

    #[test]
    fn flags_onkeydown_assignment() {
        let d = run_on("document.onkeydown = (e) => {};");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_add_event_listener() {
        assert!(run_on("element.addEventListener('click', handler);").is_empty());
    }

    #[test]
    fn allows_equality_check() {
        assert!(run_on("if (element.onclick === null) {}").is_empty());
    }
}
