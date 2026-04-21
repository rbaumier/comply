//! rust-unsafe-ffi-isolation backend.
//!
//! Line scanner with a tiny brace-tracker: when we see `mod sys` /
//! `mod ffi` / `mod raw` / `mod bindings`, we enter "safe mod" mode
//! until the matching closing brace. Any `extern "C"` / `extern "system"`
//! block declared outside such a module is flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const SAFE_MOD_NAMES: &[&str] = &["mod sys", "mod ffi", "mod raw", "mod bindings"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let mut in_safe_mod = false;
        let mut depth: usize = 0;

        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if SAFE_MOD_NAMES.iter().any(|m| t.starts_with(m)) {
                in_safe_mod = true;
            }
            depth += t.chars().filter(|&c| c == '{').count();
            let closing = t.chars().filter(|&c| c == '}').count();
            if closing >= depth {
                depth = 0;
                in_safe_mod = false;
            } else {
                depth -= closing;
            }
            if (t.starts_with("extern \"C\"") || t.starts_with("extern \"system\"")) && !in_safe_mod {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Isolate `extern \"C\"` inside `mod sys { ... }` or `mod ffi { ... }`.".into(),
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
    fn flags_extern_c_at_root() {
        assert_eq!(run(r#"extern "C" { fn foo(); }"#).len(), 1);
    }

    #[test]
    fn allows_extern_c_in_sys_mod() {
        assert!(run("mod sys {\n    extern \"C\" { fn foo(); }\n}").is_empty());
    }

    #[test]
    fn allows_extern_c_in_ffi_mod() {
        assert!(run("mod ffi {\n    extern \"C\" { fn bar(); }\n}").is_empty());
    }
}
