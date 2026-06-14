//! node-hashbang backend — validate hashbang line format.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// JS/TS runtime launchers a hashbang may legitimately invoke, including the
/// TypeScript-aware runners (`tsx`, `ts-node`, `esno`) run directly or via `npx`.
const JS_RUNTIMES: &[&str] = &["node", "bun", "tsx", "ts-node", "esno"];

/// True when an `env` invocation launches a recognized JS/TS runtime.
///
/// `/usr/bin/env` accepts a command followed by arguments, so the invocation can
/// span multiple tokens: a leading `-S`/`-x` flag (split-string form), a launcher
/// like `npx`, and runtime flags. A recognized runtime appearing as any token
/// makes the hashbang valid (e.g. `npx tsx`, `-S node --flag`).
fn env_runs_js_runtime(env_args: &str) -> bool {
    env_args
        .split_whitespace()
        .any(|token| JS_RUNTIMES.contains(&token))
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let first_line = match ctx.source.lines().next() {
            Some(l) => l,
            None => return vec![],
        };

        // Only check files that have a hashbang line.
        if !first_line.starts_with("#!") {
            return vec![];
        }

        // Valid hashbang runs a JS/TS runtime via `env`, which accepts a command
        // plus arguments (e.g. `node`, `npx tsx`, `-S node --flag`).
        if let Some(env_args) = first_line.strip_prefix("#!/usr/bin/env ")
            && env_runs_js_runtime(env_args)
        {
            return vec![];
        }

        // Also accept direct `/usr/bin/node`.
        if first_line.starts_with("#!/usr/bin/node") {
            return vec![];
        }

        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "node-hashbang".into(),
            message: format!("Invalid hashbang: `{first_line}`. Expected `#!/usr/bin/env node`."),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("bin/cli.js"), source))
    }

    #[test]
    fn flags_wrong_hashbang() {
        let d = run("#!/usr/bin/python\nconsole.log('hi');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Invalid hashbang"));
    }

    #[test]
    fn allows_correct_hashbang() {
        assert!(run("#!/usr/bin/env node\nconsole.log('hi');").is_empty());
    }

    // Regression for #278: bun is a legitimate JS runtime.
    #[test]
    fn allows_bun_hashbang() {
        assert!(run("#!/usr/bin/env bun\nconsole.log('hi');").is_empty());
    }

    // Regression for #1699: `npx tsx` is a valid TypeScript runtime launcher.
    #[test]
    fn allows_npx_tsx_hashbang() {
        assert!(run("#!/usr/bin/env npx tsx\nimport path from 'node:path';").is_empty());
    }

    // Other TypeScript-aware runners requested in #1699.
    #[test]
    fn allows_ts_node_and_esno_hashbangs() {
        assert!(run("#!/usr/bin/env ts-node\nconsole.log('hi');").is_empty());
        assert!(run("#!/usr/bin/env esno\nconsole.log('hi');").is_empty());
    }

    // `env -S` split-string form with runtime flags is valid.
    #[test]
    fn allows_env_split_string_with_flags() {
        assert!(run("#!/usr/bin/env -S node --experimental-vm-modules\nx();").is_empty());
    }

    // Negative space: a runtime substring inside an unrelated command must still
    // be flagged — only whole-token runtimes count.
    #[test]
    fn flags_env_with_non_runtime_command() {
        let d = run("#!/usr/bin/env nodemon\nconsole.log('hi');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Invalid hashbang"));
    }

    #[test]
    fn allows_no_hashbang() {
        assert!(run("console.log('hi');").is_empty());
    }
}
