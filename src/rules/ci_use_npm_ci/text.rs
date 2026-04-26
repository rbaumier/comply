//! Flag `run: npm install` in GitHub Actions workflows. `npm ci` should be used instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{pair_key_text, pair_scalar_value};

fn is_npm_install(cmd: &str) -> bool {
    // Accept leading `sudo` or env prefix like `CI=1`. Split on whitespace and
    // look for the first `npm` token followed by `install` / `i`.
    let mut tokens = cmd.split_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        if tok == "npm" {
            match tokens.peek() {
                Some(&"install") | Some(&"i") => return true,
                _ => return false,
            }
        }
    }
    false
}

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("run") { return; }
    let Some(cmd) = pair_scalar_value(node, source) else { return; };
    if !is_npm_install(&cmd) { return; }
    let Some(value) = node.named_child(1) else { return; };
    let pos = value.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ci-use-npm-ci".into(),
        message: "CI install step uses `npm install` — use `npm ci` for \
                  reproducible, lockfile-exact installs."
            .into(),
        severity: Severity::Warning,
        span: Some((value.byte_range().start, value.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_yaml_with_path;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_yaml_with_path(source, &Check, ".github/workflows/ci.yml")
    }

    #[test]
    fn flags_npm_install() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - run: npm install";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_npm_i_shorthand() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - run: npm i";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_npm_ci() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - run: npm ci";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_pnpm_install() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - run: pnpm install";
        assert!(run(yaml).is_empty());
    }
}
