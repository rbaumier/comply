//! no-identical-functions Rust backend.
//!
//! Flag `fn` items with identical bodies. Methods inside trait impls
//! (`impl Trait for Type`) are exempt: identical bodies there are forced by
//! the trait contract (you cannot call across impl blocks for different
//! types, and differing argument types block a shared generic helper).
//! Inherent impl methods and free functions are still flagged.

use crate::diagnostic::{Diagnostic, Severity};

fn normalize_body(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    // Only process at the root (source_file) to collect all functions once.
    let mut functions: Vec<(String, usize, String)> = Vec::new();

    let child_count = node.named_child_count();
    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };
        collect_functions(child, source, &mut functions);
    }

    for i in 1..functions.len() {
        for j in 0..i {
            if functions[i].2 == functions[j].2 {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: functions[i].1,
                    column: 1,
                    rule_id: "no-identical-functions".into(),
                    message: format!(
                        "Function `{}` has an identical body to `{}` (line {}). Extract the duplicated logic into a shared helper.",
                        functions[i].0,
                        functions[j].0,
                        functions[j].1,
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                break;
            }
        }
    }
}

fn collect_functions(
    node: tree_sitter::Node,
    source: &[u8],
    functions: &mut Vec<(String, usize, String)>,
) {
    match node.kind() {
        "function_item" => {
            if let Some((name, line, body)) = extract_function_info(node, source) {
                let normalized = normalize_body(&body);
                // Only flag functions with >3 lines to avoid trivial matches.
                if body.lines().count() > 3 {
                    functions.push((name, line, normalized));
                }
            }
        }
        "impl_item" | "mod_item" => {
            // Trait impl methods (`impl Trait for Type`) are forced by the
            // trait contract and cannot share a helper — skip them entirely.
            // Same trait-ness test as `rust_helpers::is_in_trait_impl`, but
            // applied top-down on the impl_item (we prune the whole subtree)
            // rather than walking up from each method.
            if node.kind() == "impl_item" && node.child_by_field_name("trait").is_some() {
                return;
            }
            let count = node.named_child_count();
            for i in 0..count {
                if let Some(child) = node.named_child(i) {
                    collect_functions(child, source, functions);
                }
            }
        }
        "declaration_list" => {
            let count = node.named_child_count();
            for i in 0..count {
                if let Some(child) = node.named_child(i) {
                    collect_functions(child, source, functions);
                }
            }
        }
        _ => {}
    }
}

fn extract_function_info(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<(String, usize, String)> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;
    let body_node = node.child_by_field_name("body")?;
    let body = body_node.utf8_text(source).ok()?;
    let line = name_node.start_position().row + 1;
    Some((name.to_string(), line, body.to_string()))
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_identical_functions() {
        let src = r#"
fn foo(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

fn bar(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bar"));
        assert!(d[0].message.contains("foo"));
    }

    #[test]
    fn allows_different_functions() {
        let src = r#"
fn foo(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

fn bar(x: i32) -> i32 {
    let a = x - 1;
    let b = a / 2;
    println!("{}", b);
    b
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_short_identical_bodies() {
        let src = r#"
fn foo() -> i32 {
    1
}

fn bar() -> i32 {
    1
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_identical_trait_methods_across_impl_blocks() {
        let src = r#"
struct A;
struct B;

impl De for A {
    fn deserialize_enum<V>(self, name: &str, visitor: V) -> R {
        let _ = name;
        let _ = visitor;
        visitor.visit_enum(self)
    }
}

impl De for B {
    fn deserialize_enum<V>(self, name: &str, visitor: V) -> R {
        let _ = name;
        let _ = visitor;
        visitor.visit_enum(self)
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_identical_trait_methods_within_one_impl_block() {
        let src = r#"
struct IgnoredAny;

impl Visitor for IgnoredAny {
    fn visit_bool(self, x: bool) -> Result<IgnoredAny, E> {
        let _ = x;
        let ack = ();
        Ok(IgnoredAny)
    }

    fn visit_i64(self, x: i64) -> Result<IgnoredAny, E> {
        let _ = x;
        let ack = ();
        Ok(IgnoredAny)
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_identical_inherent_impl_methods() {
        let src = r#"
struct Foo;

impl Foo {
    fn a(&self, x: i32) -> i32 {
        let a = x + 1;
        let b = a * 2;
        b
    }

    fn b(&self, x: i32) -> i32 {
        let a = x + 1;
        let b = a * 2;
        b
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`b`"));
        assert!(d[0].message.contains("`a`"));
    }
}
