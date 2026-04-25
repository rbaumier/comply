//! Flags consecutive overload signatures for the same function name
//! where an earlier signature has fewer required parameters than a
//! later one (i.e. the more general comes first).

use crate::diagnostic::{Diagnostic, Severity};

fn signature_name<'a>(node: tree_sitter::Node<'a>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "function_signature" | "function_declaration" => {
            let name = node.child_by_field_name("name")?;
            std::str::from_utf8(&source[name.byte_range()]).ok().map(str::to_string)
        }
        _ => None,
    }
}

fn required_param_count(node: tree_sitter::Node) -> Option<usize> {
    let params = node.child_by_field_name("parameters")?;
    let mut cursor = params.walk();
    let mut required = 0;
    for p in params.named_children(&mut cursor) {
        // optional_parameter and rest_parameter are not "required"
        if p.kind() == "required_parameter" {
            required += 1;
        }
    }
    Some(required)
}

fn has_implementation_body(node: tree_sitter::Node) -> bool {
    node.kind() == "function_declaration" && node.child_by_field_name("body").is_some()
}

/// Score for a parameter type — lower means more specific. Conservative
/// heuristic: literal types beat their primitives, primitives beat unions,
/// smaller unions beat larger ones, anything else stays neutral.
fn type_specificity_score(ty: tree_sitter::Node<'_>, source: &[u8]) -> u32 {
    match ty.kind() {
        "literal_type" | "template_literal_type" => 0,
        "predefined_type" => {
            let text = std::str::from_utf8(&source[ty.byte_range()]).unwrap_or("");
            // `any` / `unknown` are widening — most general.
            if text == "any" || text == "unknown" { 1000 } else { 10 }
        }
        "union_type" => {
            // tree-sitter-typescript builds unions right-associatively, so
            // `A | B | C` is `union_type(A, union_type(B, C))`. Count leaves.
            100 + count_union_leaves(ty)
        }
        // Other types (type_identifier, generic_type, ...) are neutral.
        _ => 50,
    }
}

fn count_union_leaves(ty: tree_sitter::Node<'_>) -> u32 {
    if ty.kind() != "union_type" { return 1; }
    let mut cursor = ty.walk();
    let mut total = 0;
    for c in ty.named_children(&mut cursor) {
        total += count_union_leaves(c);
    }
    total
}

fn signature_param_types<'a>(
    sig: tree_sitter::Node<'a>,
    _source: &'a [u8],
) -> Vec<Option<tree_sitter::Node<'a>>> {
    let Some(params) = sig.child_by_field_name("parameters") else { return Vec::new(); };
    let mut cursor = params.walk();
    let mut out = Vec::new();
    for p in params.named_children(&mut cursor) {
        if p.kind() != "required_parameter" && p.kind() != "optional_parameter" { continue; }
        // Find the `type_annotation` child, then its first named child (the type).
        let mut child_cursor = p.walk();
        let ann = p
            .named_children(&mut child_cursor)
            .find(|c| c.kind() == "type_annotation");
        let ty = ann.and_then(|ann| ann.named_child(0));
        out.push(ty);
    }
    out
}

/// True when any parameter of `a` is strictly more general than the
/// corresponding parameter of `b` (and none is strictly more specific).
fn earlier_param_types_more_general(
    a: tree_sitter::Node<'_>,
    b: tree_sitter::Node<'_>,
    source: &[u8],
) -> bool {
    let ta = signature_param_types(a, source);
    let tb = signature_param_types(b, source);
    if ta.len() != tb.len() || ta.is_empty() { return false; }
    let mut a_more_general = false;
    for (ax, bx) in ta.iter().zip(tb.iter()) {
        let (Some(ax), Some(bx)) = (ax, bx) else { return false; };
        let sa = type_specificity_score(*ax, source);
        let sb = type_specificity_score(*bx, source);
        if sa < sb { return false; } // earlier is already more specific somewhere
        if sa > sb { a_more_general = true; }
    }
    a_more_general
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if !matches!(kind, "program" | "statement_block" | "module") {
        return;
    }

    let mut cursor = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor).collect();

    // Group consecutive signatures by name.
    let mut i = 0;
    while i < children.len() {
        let Some(name) = signature_name(children[i], source) else {
            i += 1;
            continue;
        };

        let mut group: Vec<tree_sitter::Node> = Vec::new();
        let mut j = i;
        while j < children.len() {
            let Some(n) = signature_name(children[j], source) else { break };
            if n != name {
                break;
            }
            // Skip the implementation (has a body) — overloads are the signatures above it.
            if has_implementation_body(children[j]) {
                break;
            }
            group.push(children[j]);
            j += 1;
        }

        if group.len() >= 2 {
            let counts: Vec<Option<usize>> = group.iter().map(|n| required_param_count(*n)).collect();
            // Flag if any earlier overload has strictly fewer required params than a later one.
            'outer: for a in 0..counts.len() {
                for b in (a + 1)..counts.len() {
                    if let (Some(ca), Some(cb)) = (counts[a], counts[b])
                        && ca < cb
                    {
                        diagnostics.push(Diagnostic::at_node(
                            ctx.path,
                            &group[a],
                            super::META.id,
                            format!("Overload of `{name}` is less specific ({ca} params) than a later one ({cb} params); reorder specific-to-general."),
                            Severity::Warning,
                        ));
                        continue 'outer;
                    }
                }
                // Same arity — compare type specificity per parameter.
                for b in (a + 1)..counts.len() {
                    if counts[a] != counts[b] { continue; }
                    if earlier_param_types_more_general(group[a], group[b], source) {
                        diagnostics.push(Diagnostic::at_node(
                            ctx.path,
                            &group[a],
                            super::META.id,
                            format!("Overload of `{name}` uses more general parameter types than a later one; reorder specific-to-general."),
                            Severity::Warning,
                        ));
                        continue 'outer;
                    }
                }
            }
        }

        i = j.max(i + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_general_before_specific() {
        let src = "function f(a: string): void;\nfunction f(a: string, b: number): void;\nfunction f(a: string, b?: number): void {}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_specific_before_general() {
        let src = "function f(a: string, b: number): void;\nfunction f(a: string): void;\nfunction f(a: string, b?: number): void {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_overloads() {
        let src = "function f(a: string): void {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_general_string_before_literal() {
        let src = "function f(a: string): void;\nfunction f(a: 'x'): void;\nfunction f(a: string): void {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_literal_before_general_string() {
        let src = "function f(a: 'x'): void;\nfunction f(a: string): void;\nfunction f(a: string): void {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_wide_union_before_narrow_union() {
        let src = "function f(a: 'x' | 'y' | 'z'): void;\nfunction f(a: 'x' | 'y'): void;\nfunction f(a: string): void {}";
        assert_eq!(run(src).len(), 1);
    }
}
