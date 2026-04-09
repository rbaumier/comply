//! Custom lint rules.
//!
//! **Transitional state**: two APIs coexist during the rules-backend refactor.
//! - **Legacy trait** (`Rule`): one type per (rule, language) pair, currently
//!   used by all shipped rules. The engine still dispatches on this.
//! - **New struct** (`RuleDef`): one value per rule concept with a list of
//!   `(Language, Backend)` pairs. Backends are pluggable: tree-sitter,
//!   text, oxlint delegation, clippy delegation, tsc delegation. New rules
//!   will be authored as RuleDef; legacy rules will migrate one at a time
//!   before the old trait is deleted. See TODO.md "Architecture" section.

pub mod backend;
pub mod banned_identifiers;
pub mod max_file_lines;
pub mod max_function_lines;
pub mod meta;
pub mod no_nested_ternary;
pub mod no_throw;
pub mod walker;

use crate::diagnostic::Diagnostic;
use crate::files::Language;
use std::path::Path;

/// New rule shape — a RuleMeta + per-language backends. Will replace the
/// `Rule` trait once every shipped rule has been migrated.
pub struct RuleDef {
    pub meta: meta::RuleMeta,
    pub backends: Vec<(Language, backend::Backend)>,
}

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

/// Test helper — parses TS source with tree-sitter and applies a rule.
#[cfg(test)]
pub fn lint_ts_with<R: Rule>(rule: &R, source: &str) -> Vec<Diagnostic> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("failed to load TypeScript grammar");
    let tree = parser.parse(source, None).expect("failed to parse source");
    rule.check_tree(
        Path::new("test.ts"),
        source.as_bytes(),
        &tree,
        Language::TypeScript,
    )
}

/// All registered legacy-trait rules. Shrinks as rules migrate to RuleDef.
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(max_function_lines::MaxFunctionLines),
        Box::new(no_throw::NoThrow),
        Box::new(no_nested_ternary::NoNestedTernary),
        Box::new(banned_identifiers::BannedIdentifiers),
    ]
}

/// All registered RuleDef rules. Grows as rules migrate from the legacy trait.
pub fn all_rule_defs() -> Vec<RuleDef> {
    vec![max_file_lines::register()]
}
