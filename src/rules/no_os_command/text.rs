use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PATTERNS: &[&str] = &[
    "exec(",
    "execSync(",
    "spawn(",
    "spawnSync(",
    "execFile(",
    "child_process",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for pattern in PATTERNS {
                if line.contains(pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-os-command".into(),
                        message: format!(
                            "OS command execution via `{}` — potential command-injection vector.",
                            pattern.trim_end_matches('('),
                        ),
                        severity: Severity::Error,
                    });
                    break;
                }
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
    fn flags_exec() {
        assert_eq!(run("const result = exec('ls -la');").len(), 1);
    }

    #[test]
    fn flags_exec_sync() {
        assert_eq!(run("execSync(`rm -rf ${dir}`);").len(), 1);
    }

    #[test]
    fn flags_spawn() {
        assert_eq!(run("const child = spawn('node', ['app.js']);").len(), 1);
    }

    #[test]
    fn flags_child_process_import() {
        assert_eq!(run("import { exec } from 'child_process';").len(), 1);
    }

    #[test]
    fn allows_normal_function_calls() {
        assert!(run("const result = execute(query);").is_empty());
    }
}
