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
            for a in 0..counts.len() {
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
                        break;
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
}
