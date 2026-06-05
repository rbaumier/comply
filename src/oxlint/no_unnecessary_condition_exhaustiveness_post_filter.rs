//! Post-filter for `no-unnecessary-condition` false positives on
//! discriminated-union exhaustiveness gates.
//!
//! When a discriminated union has a single literal variant, TypeScript sees
//! the branch condition as always-true and `no-unnecessary-condition` flags it.
//! But the comparison is intentional: it is an exhaustiveness gate kept so the
//! union can be widened later without missing a branch.
//!
//! Detection: suppress the diagnostic when the flagged line contains a `===` or
//! `!==` comparison and, within the next 50 lines, there is a `: never = <lhs>`
//! binding using the same discriminant expression.

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    let mut file_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "no-unnecessary-condition" {
            return true;
        }
        let path: &Path = &d.path;
        let entry = file_cache
            .entry(path.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(path).ok());
        let Some(src) = entry.as_deref() else {
            return true;
        };
        !is_exhaustiveness_gate_fp(src, d.line)
    });
}

/// True when the diagnostic on `line_1based` is a discriminant comparison
/// followed by a `: never = <discriminant>` exhaustiveness gate.
fn is_exhaustiveness_gate_fp(src: &str, line_1based: usize) -> bool {
    let lines: Vec<&str> = src.lines().collect();
    if line_1based == 0 || line_1based > lines.len() {
        return false;
    }
    let flagged = lines[line_1based - 1];
    let Some(lhs) = extract_comparison_lhs(flagged) else {
        return false;
    };
    if lhs.is_empty() {
        return false;
    }
    let needle = format!(": never = {lhs}");
    let window_start = line_1based; // first line after the flagged one (0-indexed)
    let window_end = (window_start + 50).min(lines.len());
    lines[window_start..window_end]
        .iter()
        .any(|l| l.contains(&needle))
}

/// Extract the left-hand side of a `===` or `!==` comparison on `line`.
///
/// Returns `None` when no such operator is present.  Returns `Some("")` when
/// the operator is found but the LHS cannot be parsed to an identifier.
fn extract_comparison_lhs(line: &str) -> Option<String> {
    let op_idx = find_first_op(line)?;
    let raw = line[..op_idx].trim();
    if raw.is_empty() {
        return Some(String::new());
    }
    // Take the rightmost contiguous run of identifier / member-expression chars
    // (alphanumeric, `_`, `$`, `.`).  This strips leading keywords like `if (`.
    let start = raw
        .char_indices()
        .rev()
        .skip_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '.')
        .next()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    Some(raw[start..].to_owned())
}

/// Return the byte index of the first `===` or `!==` operator in `line`,
/// or `None` if neither is present.
fn find_first_op(line: &str) -> Option<usize> {
    let eq3 = find_substr(line, "===");
    let neq = find_substr(line, "!==");
    match (eq3, neq) {
        (None, None) => None,
        (Some(i), None) => Some(i),
        (None, Some(j)) => Some(j),
        (Some(i), Some(j)) => Some(i.min(j)),
    }
}

fn find_substr(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn fake_diag(path: &Path, line: usize, rule: &'static str) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(path),
            line,
            column: 1,
            rule_id: Cow::Borrowed(rule),
            message: String::new(),
            severity: crate::diagnostic::Severity::Error,
            span: None,
        }
    }

    fn write_temp(name: &str, src: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("comply-no-unnecessary-cond-filter-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, src).unwrap();
        path
    }

    fn line_of(src: &str, needle: &str) -> usize {
        src.lines()
            .enumerate()
            .find(|(_, l)| l.contains(needle))
            .map(|(i, _)| i + 1)
            .expect("needle not in source")
    }

    // Test 1: reproducer from the issue — should be suppressed
    #[test]
    fn drops_exhaustiveness_gate_fp() {
        let src = r#"type Props = { action: "create"; onClose: () => void };

function NetworkFormDialog(props: Props): ReactElement {
  if (props.action === "create") {
    return <NetworkCreateContent onClose={props.onClose} />;
  }
  const _exhaustive: never = props.action;
  return _exhaustive;
}
"#;
        let path = write_temp("drops_exhaustiveness_gate.tsx", src);
        let line = line_of(src, "props.action === \"create\"");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-condition")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }

    // Test 2: always-true comparison without a `: never` gate — should be kept
    #[test]
    fn keeps_always_true_without_gate() {
        let src = r#"type Props = { action: "create" };

function foo(props: Props) {
  if (props.action === "create") {
    return 1;
  }
  return 0;
}
"#;
        let path = write_temp("keeps_no_gate.tsx", src);
        let line = line_of(src, "props.action === \"create\"");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-condition")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "expected diagnostic kept");
    }

    // Test 3: gate on different discriminant — should be kept
    #[test]
    fn keeps_gate_on_different_discriminant() {
        let src = r#"function foo(props) {
  if (props.action === "create") {
    return 1;
  }
  const _exhaustive: never = props.status;
  return _exhaustive;
}
"#;
        let path = write_temp("keeps_gate_other_discriminant.tsx", src);
        let line = line_of(src, "props.action === \"create\"");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-condition")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "expected diagnostic kept");
    }

    // Test 4: unreadable file — should keep the diagnostic
    #[test]
    fn keeps_diagnostic_on_unreadable_file() {
        let nonexistent =
            std::env::temp_dir().join("does-not-exist-comply-no-unnecessary-cond-test.ts");
        let mut diags = vec![fake_diag(&nonexistent, 5, "no-unnecessary-condition")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    // Test 5: different rule on same line — should not be touched
    #[test]
    fn does_not_touch_other_rules() {
        let src = r#"function foo(props) {
  if (props.action === "create") {
    return 1;
  }
  const _exhaustive: never = props.action;
  return _exhaustive;
}
"#;
        let path = write_temp("other_rule_on_same_line.tsx", src);
        let line = line_of(src, "props.action === \"create\"");
        let mut diags = vec![
            fake_diag(&path, line, "no-unnecessary-condition"),
            fake_diag(&path, line, "no-explicit-any"),
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "expected only no-explicit-any to remain");
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }

    // Test 6: `!==` variant — should be suppressed
    #[test]
    fn drops_not_equal_variant() {
        let src = r#"function foo(props) {
  if (props.action !== "edit") {
    return 1;
  }
  const _exhaustive: never = props.action;
  return _exhaustive;
}
"#;
        let path = write_temp("drops_neq_variant.tsx", src);
        let line = line_of(src, "props.action !== \"edit\"");
        let mut diags = vec![fake_diag(&path, line, "no-unnecessary-condition")];
        apply(&mut diags);
        assert!(diags.is_empty(), "expected diagnostic dropped, got: {diags:?}");
    }
}
