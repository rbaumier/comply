//! ts-no-unsafe-declaration-merging backend — collect class and interface names,
//! flag overlaps.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["class_declaration", "interface_declaration"];

#[derive(Default)]
struct State {
    class_names: Vec<(String, usize, usize)>,
    interface_names: Vec<(String, usize, usize)>,
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::<State>::default())
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let kind = match node.kind() {
            "class_declaration" => "class",
            "interface_declaration" => "interface",
            _ => return,
        };
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = &source[name_node.byte_range()];
        let Ok(name_str) = std::str::from_utf8(name) else {
            return;
        };
        let pos = name_node.start_position();
        let entry = (name_str.to_string(), pos.row + 1, pos.column + 1);
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        if kind == "class" {
            state.class_names.push(entry);
        } else {
            state.interface_names.push(entry);
        }
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        // Flag interfaces that share a name with a class
        for (iface_name, line, col) in &state.interface_names {
            if state.class_names.iter().any(|(c, _, _)| c == iface_name) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: *line,
                    column: *col,
                    rule_id: "ts-no-unsafe-declaration-merging".into(),
                    message: format!(
                        "Unsafe declaration merging — interface `{iface_name}` \
                         shares a name with a class."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        // Also flag classes that share a name with an interface
        for (class_name, line, col) in &state.class_names {
            if state.interface_names.iter().any(|(i, _, _)| i == class_name) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: *line,
                    column: *col,
                    rule_id: "ts-no-unsafe-declaration-merging".into(),
                    message: format!(
                        "Unsafe declaration merging — class `{class_name}` \
                         shares a name with an interface."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_class_and_interface_same_name() {
        let diags = run_on("interface Foo {} class Foo {}");
        assert_eq!(diags.len(), 2); // one for each declaration
    }

    #[test]
    fn allows_different_names() {
        assert!(run_on("interface Foo {} class Bar {}").is_empty());
    }

    #[test]
    fn allows_class_only() {
        assert!(run_on("class Foo {}").is_empty());
    }

    #[test]
    fn allows_interface_only() {
        assert!(run_on("interface Foo { x: number }").is_empty());
    }
}
