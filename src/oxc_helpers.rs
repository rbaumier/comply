//! Thin wrapper around oxc_parser + oxc_semantic for rules that need
//! true scope analysis (cross-scope reference tracking, shadowing,
//! unused symbols) instead of the heuristic tree-sitter walks.
//!
//! `oxc_ast` borrows from a bump `Allocator` for the whole AST lifetime,
//! so we expose a closure-based API instead of returning the `Semantic`:
//! the allocator lives on the stack of `with_semantic` and gets dropped
//! when the closure returns.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::SourceType;

/// Pick the right `SourceType` based on file extension. Defaults to `tsx()`
/// for unknown extensions — it's the most permissive (accepts JSX +
/// TypeScript syntax).
pub fn source_type_for_path(path: &Path) -> SourceType {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ts") => SourceType::ts(),
        Some("tsx") => SourceType::tsx(),
        Some("mjs") => SourceType::mjs(),
        Some("cjs") => SourceType::cjs(),
        Some("jsx") => SourceType::jsx(),
        _ => SourceType::tsx(),
    }
}

/// Parse `source` and run semantic analysis, then hand the resulting
/// `Semantic` to `f`. Both the allocator and the AST are dropped after `f`
/// returns, so callers must extract whatever they need (diagnostics, lists
/// of names, …) into owned values inside the closure.
pub fn with_semantic<F, R>(source: &str, source_type: SourceType, f: F) -> R
where
    F: for<'a> FnOnce(&'a Semantic<'a>) -> R,
{
    let allocator = Allocator::default();
    let parse_ret = Parser::new(&allocator, source, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    f(&semantic)
}

/// Convert an oxc byte offset into 1-based `(line, column)`.
///
/// Shared across all `OxcCheck` rules that emit diagnostics — avoids the
/// copy-pasted per-rule helper that was duplicated in 15+ tree-sitter rules.
pub fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\r' {
            continue;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Parse `source` with oxc_parser using the source type inferred from `path`,
/// build semantic analysis, then hand the `Semantic` to `f`. The allocator
/// and AST are dropped after `f` returns.
///
/// Used by the engine hot path for `Backend::Oxc` dispatch.
pub fn with_oxc_parse<F, R>(source: &str, path: &Path, f: F) -> R
where
    F: for<'a> FnOnce(&'a Semantic<'a>) -> R,
{
    let source_type = source_type_for_path(path);
    let allocator = Allocator::default();
    let parse_ret = Parser::new(&allocator, source, source_type).parse();
    let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
    f(&semantic)
}
