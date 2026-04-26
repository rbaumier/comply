//! react-no-typos AST backend.
//!
//! Flags probable typos in React static properties and lifecycle methods.
//! Uses edit-distance comparison against known correct names.

use crate::diagnostic::{Diagnostic, Severity};

/// Correct React lifecycle methods and static properties.
const KNOWN_NAMES: &[&str] = &[
    "getDerivedStateFromProps",
    "componentWillMount",
    "UNSAFE_componentWillMount",
    "componentDidMount",
    "componentWillReceiveProps",
    "UNSAFE_componentWillReceiveProps",
    "shouldComponentUpdate",
    "componentWillUpdate",
    "UNSAFE_componentWillUpdate",
    "getSnapshotBeforeUpdate",
    "componentDidUpdate",
    "componentDidCatch",
    "componentWillUnmount",
    "render",
    "defaultProps",
    "displayName",
    "propTypes",
    "contextTypes",
    "childContextTypes",
    "contextType",
];

/// Simple Levenshtein distance (bounded).
fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    if m == 0 { return n; }
    if n == 0 { return m; }

    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];

    for (j, item) in prev.iter_mut().enumerate().take(n + 1) {
        *item = j;
    }

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

fn is_probable_typo(name: &str) -> Option<&'static str> {
    // Must be close to a known name but not match exactly.
    for &known in KNOWN_NAMES {
        if name == known {
            return None; // exact match, no typo
        }
    }
    for &known in KNOWN_NAMES {
        let dist = edit_distance(name, known);
        // Threshold: 1-2 edits for names > 5 chars.
        if known.len() > 5 && dist > 0 && dist <= 2 {
            return Some(known);
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Look at class property definitions and method definitions.
    let is_method = node.kind() == "method_definition";
    let is_prop = node.kind() == "public_field_definition"
        || node.kind() == "property_definition"
        || node.kind() == "field_definition";

    if !is_method && !is_prop {
        return;
    }

    // Get the name.
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    if let Some(correction) = is_probable_typo(name) {
        let pos = name_node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-no-typos".into(),
            message: format!(
                "`{name}` is a probable typo — did you mean `{correction}`?"
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_lifecycle_typo() {
        let src = "class Comp extends React.Component {\n  componentDidMoun() {}\n}";
        // "componentDidMoun" vs "componentDidMount" — 1 edit
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("componentDidMount"));
    }

    #[test]
    fn allows_correct_lifecycle() {
        let src = "class Comp extends React.Component {\n  componentDidMount() {}\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_static_prop_typo() {
        let src = "class Comp extends React.Component {\n  static defautProps = {};\n}";
        // "defautProps" vs "defaultProps" — 1 edit
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("defaultProps"));
    }
}
