//! Flag `uses: actions/checkout@v3` (or lower) and floating refs (@main, @master).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{pair_key_text, pair_scalar_value};

fn is_forbidden_ref(r: &str) -> bool {
    if r == "main" || r == "master" {
        return true;
    }
    // @v1, @v2, @v3 are forbidden. @v4+ is OK. Non-version refs (SHAs) are OK.
    if let Some(rest) = r.strip_prefix('v')
        && let Ok(major) = rest.split('.').next().unwrap_or("").parse::<u32>()
    {
        return major < 4;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "block_mapping_pair" { return; }
    if pair_key_text(node, source).as_deref() != Some("uses") { return; }
    let Some(value) = pair_scalar_value(node, source) else { return; };
    let Some((name, at_ref)) = value.split_once('@') else { return; };
    if name.trim() != "actions/checkout" { return; }
    // Strip any inline comment.
    let at_ref = at_ref.split('#').next().unwrap_or(at_ref).trim();
    if !is_forbidden_ref(at_ref) { return; }
    let Some(value_node) = node.named_child(1) else { return; };
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ci-checkout-action-pinned".into(),
        message: format!(
            "actions/checkout is pinned to `@{at_ref}` — pin to `@v4` or a commit SHA."
        ),
        severity: Severity::Warning,
        span: Some((value_node.byte_range().start, value_node.byte_range().len())),
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
    fn flags_v3() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - uses: actions/checkout@v3";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_main() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - uses: actions/checkout@main";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_v4() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - uses: actions/checkout@v4";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn ignores_other_actions() {
        let yaml = "on: push\njobs:\n  build:\n    steps:\n    - uses: actions/setup-node@v3";
        assert!(run(yaml).is_empty());
    }
}
