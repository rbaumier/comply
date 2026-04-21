//! rust-prefer-channel-over-arc-mutex-vec backend.
//!
//! Flags files that construct `Arc::new(Mutex::new(Vec…))`, then call
//! `.lock()` and `.push(` elsewhere — the "collector Vec shared
//! across worker threads" antipattern. Pre-gating on all three
//! substrings being present keeps the cost low for unrelated files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("Arc::new(Mutex::new(Vec") { return vec![]; }
        if !src.contains(".lock()") { return vec![]; }
        if !src.contains(".push(") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("Arc::new(Mutex::new(Vec") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Use `mpsc::channel` instead of `Arc<Mutex<Vec>>` to collect results from concurrent tasks.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), src))
    }

    #[test]
    fn flags_arc_mutex_vec_with_push() {
        let src = "let results = Arc::new(Mutex::new(Vec::new()));\nlet r = results.clone();\nthread::spawn(move || r.lock().unwrap().push(compute()));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_channel() {
        let src = "let (tx, rx) = mpsc::channel();\nthread::spawn(move || tx.send(compute()).unwrap());\nlet results: Vec<_> = rx.iter().collect();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arc_mutex_without_push() {
        assert!(run("let x = Arc::new(Mutex::new(Vec::new()));").is_empty());
    }
}
