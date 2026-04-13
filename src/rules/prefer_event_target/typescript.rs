use crate::diagnostic::{Diagnostic, Severity};

/// Packages whose `EventEmitter` should be ignored (Angular, eventemitter3).
const IGNORED_PACKAGES: &[&str] = &["@angular/core", "eventemitter3"];

/// Detect `extends EventEmitter` or `new EventEmitter` usage, excluding
/// lines that import from ignored packages.
fn has_event_emitter_usage(line: &str) -> bool {
    // Skip import/require lines from ignored packages
    for pkg in IGNORED_PACKAGES {
        if line.contains(pkg) {
            return false;
        }
    }

    // Case 1: class extends EventEmitter
    if line.contains("extends EventEmitter")
        && let Some(pos) = line.find("extends EventEmitter") {
            let after = &line[pos + "extends EventEmitter".len()..];
            // Next char should not be alphanumeric (e.g. "EventEmitterEx")
            if let Some(ch) = after.chars().next()
                && (ch.is_ascii_alphanumeric() || ch == '_')
            {
                return false;
            }
            return true;
        }

    // Case 2: new EventEmitter(
    if line.contains("new EventEmitter(") || line.contains("new EventEmitter;") {
        return true;
    }
    // `new EventEmitter` at end of line (no parens yet — multi-line)
    if line.trim_end().ends_with("new EventEmitter") {
        return true;
    }

    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in src.lines().enumerate() {
        if has_event_emitter_usage(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "prefer-event-target".into(),
                message: "Prefer `EventTarget` over `EventEmitter`.".into(),
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
    fn flags_extends_event_emitter() {
        let d = run_ts("class MyEmitter extends EventEmitter {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_new_event_emitter() {
        let d = run_ts("const emitter = new EventEmitter();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_event_target() {
        assert!(run_ts("class MyTarget extends EventTarget {}").is_empty());
    }

    #[test]
    fn allows_import_from_ignored_package() {
        assert!(run_ts(r#"import { EventEmitter } from "eventemitter3";"#).is_empty());
    }

    #[test]
    fn allows_angular_event_emitter() {
        assert!(run_ts(r#"import { EventEmitter } from "@angular/core";"#).is_empty());
    }

    #[test]
    fn does_not_flag_event_emitter_ex() {
        assert!(run_ts("class MyEmitter extends EventEmitterEx {}").is_empty());
    }
}
