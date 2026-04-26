//! Flag `POSTGRES_PASSWORD: <literal>` — the value must come from `${{ secrets.* }}`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::yaml_k8s_helpers::{pair_key_text, pair_scalar_value};

crate::ast_check! { on ["block_mapping_pair"] => |node, source, ctx, diagnostics|
    if pair_key_text(node, source).as_deref() != Some("POSTGRES_PASSWORD") { return; }
    let Some(value) = pair_scalar_value(node, source) else { return; };
    let value = value.split('#').next().unwrap_or("").trim();
    if value.is_empty() { return; }
    if value.contains("${{") { return; }

    let Some(value_node) = node.named_child(1) else { return; };
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: 1,
        rule_id: "ci-no-hardcoded-db-password".into(),
        message: "POSTGRES_PASSWORD is a literal value — reference \
                  `${{ secrets.POSTGRES_PASSWORD }}` instead."
            .into(),
        severity: Severity::Error,
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
    fn flags_literal_password() {
        let yaml = "on: push\njobs:\n  db:\n    env:\n      POSTGRES_PASSWORD: hunter2";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn flags_quoted_literal() {
        let yaml = "on: push\njobs:\n  db:\n    env:\n      POSTGRES_PASSWORD: 'postgres'";
        assert_eq!(run(yaml).len(), 1);
    }

    #[test]
    fn allows_secret_reference() {
        let yaml = "on: push\njobs:\n  db:\n    env:\n      POSTGRES_PASSWORD: ${{ secrets.PG_PW }}";
        assert!(run(yaml).is_empty());
    }
}
