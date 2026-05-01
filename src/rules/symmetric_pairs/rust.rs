//! symmetric-pairs Rust backend.
//!
//! Check `pub fn` items for missing symmetric counterparts.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const PAIRS: &[(&str, &str)] = &[
    ("set_", "get_"),
    ("add_", "remove_"),
    ("remove_", "add_"),
    ("open_", "close_"),
    ("close_", "open_"),
    ("start_", "stop_"),
    ("stop_", "start_"),
    ("create_", "delete_"),
    ("delete_", "create_"),
    ("create_", "destroy_"),
];

const PREFIXES: &[&str] = &[
    "get_", "set_", "add_", "remove_", "open_", "close_", "start_", "stop_", "create_", "delete_",
    "destroy_",
];

const KINDS: &[&str] = &["function_item"];

fn split_prefix(name: &str) -> Option<(&str, &str)> {
    for &pfx in PREFIXES {
        if name.len() > pfx.len() && name.starts_with(pfx) {
            return Some((pfx, &name[pfx.len()..]));
        }
    }
    None
}

#[derive(Default)]
struct State {
    /// (name, line, col) for each `pub fn` collected.
    pub_fns: Vec<(String, usize, usize)>,
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
        let Ok(text) = node.utf8_text(source) else {
            return;
        };
        if !text.starts_with("pub ") {
            return;
        }
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source) else {
            return;
        };
        let pos = name_node.start_position();
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        state
            .pub_fns
            .push((name.to_string(), pos.row + 1, pos.column + 1));
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
        let names: Vec<&str> = state.pub_fns.iter().map(|(n, _, _)| n.as_str()).collect();
        for (name, line, col) in &state.pub_fns {
            if name.ends_with("_mut") {
                continue;
            }
            let Some((prefix, suffix)) = split_prefix(name) else {
                continue;
            };
            for &(pfx, counterpart_pfx) in PAIRS {
                if pfx == prefix {
                    if pfx == "get_"
                        && names.iter().any(|n| *n == format!("get_{suffix}_mut"))
                    {
                        break;
                    }
                    let expected = format!("{counterpart_pfx}{suffix}");
                    if pfx == "set_" && names.iter().any(|n| *n == suffix) {
                        break;
                    }
                    if !names.iter().any(|n| *n == expected) {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: *line,
                            column: *col,
                            rule_id: "symmetric-pairs".into(),
                            message: format!("`pub fn {name}` has no `{expected}` counterpart."),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_missing_counterpart() {
        let src = "pub fn open_connection() {}\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("close_connection"));
    }

    #[test]
    fn allows_complete_pair() {
        let src = "pub fn open_connection() {}\npub fn close_connection() {}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_get_with_mut_variant() {
        let src = "pub fn get_value() -> &T {}\npub fn get_value_mut() -> &mut T {}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn skips_mut_functions() {
        let src = "pub fn get_conditions_mut() {}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_getter_without_setter() {
        let src = "pub fn get_name() -> &str {}\npub fn get_id() -> usize {}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_setter_without_getter() {
        let src = "pub fn set_name(n: &str) {}\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("get_name"));
    }

    #[test]
    fn allows_setter_with_bare_getter() {
        let src = "pub fn opacity(&self) -> f32 {}\npub fn set_opacity(&mut self, v: f32) {}\n";
        assert!(run_on(src).is_empty());
    }
}
