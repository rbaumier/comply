//! Flag pure black values in CSS declarations — `#000` / `#000000`,
//! `rgb(0, 0, 0)`, or the bare `black` keyword.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "declaration" { return; }
    let Some((offender, label)) = find_pure_black(node, source) else { return };
    let pos = offender.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Pure black (`{label}`) is visually harsh — use a near-black like `#0a0a0a` instead."
        ),
        severity: Severity::Warning,
        span: Some((offender.byte_range().start, offender.byte_range().len())),
    });
}

fn find_pure_black<'t>(
    decl: tree_sitter::Node<'t>,
    source: &[u8],
) -> Option<(tree_sitter::Node<'t>, String)> {
    let mut c = decl.walk();
    let children: Vec<_> = decl.children(&mut c).collect();
    // Skip past the property_name + colon — everything after is value position.
    let after_colon = children.iter()
        .position(|n| n.kind() == ":")
        .map_or(0, |i| i + 1);
    for n in children.iter().skip(after_colon) {
        if let Some(label) = classify_pure_black(*n, source) {
            return Some((*n, label));
        }
        // Descend into call_expression's arguments to catch `rgb(0,0,0)`.
        if n.kind() == "call_expression"
            && let Some(label) = classify_call_expr(*n, source) {
                return Some((*n, label));
            }
    }
    None
}

fn classify_pure_black(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "color_value" => {
            let Ok(t) = node.utf8_text(source) else { return None };
            let hex = t.trim_start_matches('#');
            if matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b == b'0') {
                Some(t.to_string())
            } else {
                None
            }
        }
        "plain_value" => {
            let Ok(t) = node.utf8_text(source) else { return None };
            if t.trim().eq_ignore_ascii_case("black") {
                Some("black".into())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn classify_call_expr(call: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut c = call.walk();
    let kids: Vec<_> = call.children(&mut c).collect();
    let name = kids.iter().find(|n| n.kind() == "function_name")?;
    let Ok(fn_name) = name.utf8_text(source) else { return None };
    let fn_lower = fn_name.to_ascii_lowercase();
    if fn_lower != "rgb" && fn_lower != "rgba" { return None; }

    let args = kids.iter().find(|n| n.kind() == "arguments")?;
    let mut ac = args.walk();
    let nums: Vec<String> = args.children(&mut ac)
        .filter(|n| matches!(n.kind(), "integer_value" | "float_value"))
        .filter_map(|n| n.utf8_text(source).ok().map(|t| t.trim().to_string()))
        .collect();
    if nums.len() < 3 { return None; }
    if nums[..3].iter().all(|s| s == "0" || s == "0.0") {
        Some(format!("{fn_name}(0, 0, 0)"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_css(source, &Check)
    }

    #[test]
    fn flags_hex_000() {
        assert_eq!(run(".t { color: #000; }").len(), 1);
    }

    #[test]
    fn flags_hex_000000() {
        assert_eq!(run(".t { color: #000000; }").len(), 1);
    }

    #[test]
    fn flags_rgb_zero() {
        assert_eq!(run(".t { color: rgb(0, 0, 0); }").len(), 1);
    }

    #[test]
    fn flags_black_keyword() {
        assert_eq!(run(".t { color: black; }").len(), 1);
    }

    #[test]
    fn allows_near_black() {
        assert!(run(".t { color: #0a0a0a; }").is_empty());
    }

    #[test]
    fn allows_blackish_compound_class() {
        assert!(run(".blackboard { color: red; }").is_empty());
    }
}
