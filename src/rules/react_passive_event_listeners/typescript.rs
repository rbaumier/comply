use crate::diagnostic::{Diagnostic, Severity};

const SCROLL_EVENTS: &[&str] = &["touchstart", "touchmove", "wheel", "scroll"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if func.kind() != "member_expression" { return; }
    let prop = match func.child_by_field_name("property") {
        Some(p) => p,
        None => return,
    };
    if prop.utf8_text(source).unwrap_or("") != "addEventListener" { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    let event_arg = match args.named_child(0) {
        Some(a) => a,
        None => return,
    };
    let event_text = event_arg.utf8_text(source).unwrap_or("");
    let event_name = event_text.trim_matches(|c: char| c == '\'' || c == '"');
    if !SCROLL_EVENTS.contains(&event_name) { return; }

    let has_passive = match args.named_child(2) {
        Some(opt) => {
            let t = opt.utf8_text(source).unwrap_or("");
            t.contains("passive: true") || t.contains("passive:true")
        }
        None => false,
    };
    if !has_passive {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("Add `{{ passive: true }}` to `addEventListener('{event_name}', ...)` to avoid jank."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_touchstart_no_options() {
        assert_eq!(
            run("window.addEventListener('touchstart', handler)").len(),
            1
        );
    }

    #[test]
    fn flags_wheel_no_passive() {
        assert_eq!(
            run("el.addEventListener('wheel', handler, { capture: true })").len(),
            1
        );
    }

    #[test]
    fn allows_passive_true() {
        assert!(
            run("window.addEventListener('touchstart', handler, { passive: true })").is_empty()
        );
    }

    #[test]
    fn allows_click_no_passive() {
        assert!(run("btn.addEventListener('click', handler)").is_empty());
    }
}
