use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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
    if line.contains("extends EventEmitter") {
        // Make sure "EventEmitter" is not part of a larger word
        if let Some(pos) = line.find("extends EventEmitter") {
            let after = &line[pos + "extends EventEmitter".len()..];
            // Next char should not be alphanumeric (e.g. "EventEmitterEx")
            if let Some(ch) = after.chars().next()
                && (ch.is_ascii_alphanumeric() || ch == '_') {
                    return false;
                }
            return true;
        }
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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_event_emitter_usage(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-event-target".into(),
                    message: "Prefer `EventTarget` over `EventEmitter`.".into(),
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
    fn flags_extends_event_emitter() {
        let code = "class MyEmitter extends EventEmitter {}";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_new_event_emitter() {
        let code = "const emitter = new EventEmitter();";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_event_target() {
        let code = "class MyTarget extends EventTarget {}";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_import_from_ignored_package() {
        let code = r#"import { EventEmitter } from "eventemitter3";"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_angular_event_emitter() {
        let code = r#"import { EventEmitter } from "@angular/core";"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn does_not_flag_event_emitter_ex() {
        let code = "class MyEmitter extends EventEmitterEx {}";
        assert!(run(code).is_empty());
    }
}
