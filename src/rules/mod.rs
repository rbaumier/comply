#![allow(dead_code)] // Rules consumed by engine::run_custom_rules (task 12).

//! Custom lint rules — each rule implements the Rule trait and is registered
//! in `all_rules()`. The engine calls every rule on every file whose language
//! matches.
//!
//! Rules that only need source text override `check()`.
//! Rules that need the AST override `check_tree()` — the engine parses each
//! file once with tree-sitter and passes the tree to all rules.

pub mod max_file_lines;
pub mod max_function_lines;

use crate::diagnostic::Diagnostic;
use crate::files::Language;
use std::path::Path;

/// A lint rule that operates on source code, optionally with a tree-sitter AST.
pub trait Rule {
    /// Unique rule identifier (e.g., "max-file-lines").
    fn id(&self) -> &'static str;

    /// Which languages this rule applies to.
    fn languages(&self) -> &[Language];

    /// Run the rule on raw source text. Default: no-op.
    fn check(&self, _path: &Path, _source: &str, _language: Language) -> Vec<Diagnostic> {
        vec![]
    }

    /// Run the rule with a parsed tree-sitter AST. Default: no-op.
    /// The engine calls this after parsing — rules needing the AST override this.
    fn check_tree(
        &self,
        _path: &Path,
        _source: &[u8],
        _tree: &tree_sitter::Tree,
        _language: Language,
    ) -> Vec<Diagnostic> {
        vec![]
    }

    /// Whether this rule needs the tree-sitter AST (controls whether check_tree is called).
    fn needs_tree(&self) -> bool {
        false
    }
}

/// All registered custom rules. Add new rules here.
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(max_file_lines::MaxFileLines),
        Box::new(max_function_lines::MaxFunctionLines),
    ]
}
