//! Registry helpers + the macros every rule's `register()` and
//! `Check` impl call into.
//!
//! Three pieces:
//!
//! 1. **`RustBinding` enum** + **`build_ts_family_rule` / `build_rust_only_rule` helpers**
//!    — collapse the boilerplate of constructing a `RuleDef` with its `backends` vec.
//!    Without these the four `register_*!` macros below would be Rule-of-Three clones
//!    of each other.
//!
//! 2. **`register_ts_family!` / `register_ts_family_with_rust!` /
//!    `register_rust_only!` / `register_ts_family_with_clippy_marker!`** —
//!    one-liner macros each rule's `mod.rs` calls instead of writing the
//!    full `RuleDef { meta, backends: vec![...] }` shape.
//!
//! 3. **`ast_check!`** — wraps the imports, the `Check` struct, the
//!    `impl AstCheck` block, and the `walk_tree(...)` dispatch that every
//!    tree-sitter rule needs. The rule's body is the closure body inside
//!    `walk_tree`, with `node`, `source`, `ctx`, and `diagnostics`
//!    available as named bindings.
//!
//! Re-exported from `crate::rules` so the macros' `$crate::rules::*` paths
//! resolve transparently.

use crate::diagnostic::Diagnostic;
use crate::files::Language;

use super::RuleDef;
use super::backend::{self, AstCheck, Backend};
use super::meta::RuleMeta;

/// Optional Rust binding for a TS-family rule. Used by `build_ts_family_rule`
/// to decide whether to append a Rust backend after the TS/JS/TSX triple.
#[non_exhaustive]
pub enum RustBinding {
    None,
    /// In-process tree-sitter `Check`.
    TreeSitter(Box<dyn AstCheck>),
    /// Delegate to a clippy lint.
    Clippy(&'static str),
}

// Manual `Debug` impl: `Box<dyn AstCheck>` doesn't implement Debug (the
// trait isn't object-safe with a `Debug` bound), so we render the variant
// label and elide the inner check. Enough for diagnostics + assert
// failure messages, no Debug bound bleeds through to the trait.
impl std::fmt::Debug for RustBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("RustBinding::None"),
            Self::TreeSitter(_) => f.write_str("RustBinding::TreeSitter(<dyn AstCheck>)"),
            Self::Clippy(lint) => write!(f, "RustBinding::Clippy({lint:?})"),
        }
    }
}

/// Build a `RuleDef` for a TS-family rule (TypeScript + JavaScript + TSX
/// share the same `Check`), with an optional Rust binding that's either a
/// custom tree-sitter `Check` or a clippy delegation marker.
#[must_use]
pub fn build_ts_family_rule(
    meta: RuleMeta,
    ts_check: Box<dyn AstCheck>,
    js_check: Box<dyn AstCheck>,
    tsx_check: Box<dyn AstCheck>,
    rust: RustBinding,
) -> RuleDef {
    let mut backends: Vec<(Language, Backend)> = vec![
        (Language::TypeScript, Backend::TreeSitter(ts_check)),
        (Language::JavaScript, Backend::TreeSitter(js_check)),
        (Language::Tsx, Backend::TreeSitter(tsx_check)),
    ];
    match rust {
        RustBinding::None => {}
        RustBinding::TreeSitter(check) => {
            backends.push((Language::Rust, Backend::TreeSitter(check)));
        }
        RustBinding::Clippy(lint) => {
            backends.push((Language::Rust, Backend::Clippy { lint }));
        }
    }
    RuleDef { meta, backends }
}

/// Build a `RuleDef` for a Rust-only rule.
#[must_use]
pub fn build_rust_only_rule(meta: RuleMeta, check: Box<dyn AstCheck>) -> RuleDef {
    RuleDef {
        meta,
        backends: vec![(Language::Rust, Backend::TreeSitter(check))],
    }
}

// Suppress an "unused import" lint that fires because backend / Diagnostic
// are only referenced from inside the macros, which expand at the call
// site, not here.
#[allow(unused_imports)]
const _: () = {
    let _ = std::marker::PhantomData::<Diagnostic>;
    let _ = std::marker::PhantomData::<&dyn backend::AstCheck>;
};

/// `RuleDef` for a TS-only rule (TypeScript + JavaScript + TSX, no Rust).
#[macro_export]
macro_rules! register_ts_family {
    ($meta:expr, $ts_mod:ident) => {
        $crate::rules::build_ts_family_rule(
            $meta,
            Box::new($ts_mod::Check),
            Box::new($ts_mod::Check),
            Box::new($ts_mod::Check),
            $crate::rules::RustBinding::None,
        )
    };
}

/// `RuleDef` for a TS-family rule that ALSO has a custom Rust tree-sitter
/// `Check` in a sibling `rust` module.
#[macro_export]
macro_rules! register_ts_family_with_rust {
    ($meta:expr, $ts_mod:ident, $rust_mod:ident) => {
        $crate::rules::build_ts_family_rule(
            $meta,
            Box::new($ts_mod::Check),
            Box::new($ts_mod::Check),
            Box::new($ts_mod::Check),
            $crate::rules::RustBinding::TreeSitter(Box::new($rust_mod::Check)),
        )
    };
}

/// `RuleDef` for a Rust-only rule.
#[macro_export]
macro_rules! register_rust_only {
    ($meta:expr, $rust_mod:ident) => {
        $crate::rules::build_rust_only_rule($meta, Box::new($rust_mod::Check))
    };
}

/// `RuleDef` for a TS-family rule whose Rust side is covered by a clippy
/// lint delegation rather than a custom check.
#[macro_export]
macro_rules! register_ts_family_with_clippy_marker {
    ($meta:expr, $ts_mod:ident, $clippy_lint:expr) => {
        $crate::rules::build_ts_family_rule(
            $meta,
            Box::new($ts_mod::Check),
            Box::new($ts_mod::Check),
            Box::new($ts_mod::Check),
            $crate::rules::RustBinding::Clippy($clippy_lint),
        )
    };
}

/// Define a tree-sitter `Check` whose `check()` method walks the AST and
/// runs the user-provided body once per node. The caller passes the
/// argument names explicitly so Rust macro hygiene doesn't fight us:
///
/// ```ignore
/// crate::ast_check! { |node, source, ctx, diagnostics|
///     if node.kind() != "throw_statement" { return; }
///     diagnostics.push(Diagnostic::at_node(
///         ctx.path,
///         &node,
///         "no-throw-literal",
///         "throw an Error instance, not a literal".into(),
///         Severity::Warning,
///     ));
/// }
/// ```
///
/// Prefer `Diagnostic::at_node` over a `Diagnostic { ... }` literal: it
/// captures the node's byte range via `node.byte_range()` so the pretty
/// renderer highlights the exact offending expression instead of falling
/// back to whole-line highlighting. The literal form is still appropriate
/// for delegated diagnostics (oxlint/clippy/knip/madge) where only
/// `(line, column)` is available from external JSON output.
///
/// Without this macro, every rule's `typescript.rs` / `rust.rs` carried
/// the same ~13-line preamble (imports + `pub struct Check;` + `impl
/// AstCheck for Check { fn check { let source = ...; let mut diagnostics
/// = ...; walk_tree(...) } }`), which jscpd correctly flagged as a
/// Rule of Three duplication across ~50 files.
#[macro_export]
macro_rules! ast_check {
    (on [$($kind:expr),+ $(,)?] prefilter = [$($lit:expr),+ $(,)?] => |$node:ident, $source:ident, $ctx:ident, $diagnostics:ident| $($body:tt)*) => {
        #[derive(Debug)]
        pub struct Check;

        impl $crate::rules::backend::AstCheck for Check {
            fn interested_kinds(&self) -> Option<&'static [&'static str]> {
                Some(&[$($kind),+])
            }

            fn prefilter(&self) -> Option<&'static [&'static str]> {
                Some(&[$($lit),+])
            }

            fn visit_node(
                &self,
                ast_check_node: tree_sitter::Node,
                ast_check_ctx: &$crate::rules::backend::CheckCtx,
                _ast_check_state: Option<&mut dyn std::any::Any>,
                ast_check_diagnostics: &mut Vec<$crate::diagnostic::Diagnostic>,
            ) {
                #[allow(unused_variables)]
                let $node = ast_check_node;
                #[allow(unused_variables)]
                let $source: &[u8] = ast_check_ctx.source.as_bytes();
                #[allow(unused_variables)]
                let $ctx: &$crate::rules::backend::CheckCtx = ast_check_ctx;
                #[allow(unused_variables)]
                let $diagnostics: &mut Vec<$crate::diagnostic::Diagnostic> = ast_check_diagnostics;
                $($body)*
            }
        }
    };

    (prefilter = [$($lit:expr),+ $(,)?] => |$node:ident, $source:ident, $ctx:ident, $diagnostics:ident| $($body:tt)*) => {
        #[derive(Debug)]
        pub struct Check;

        impl $crate::rules::backend::AstCheck for Check {
            fn prefilter(&self) -> Option<&'static [&'static str]> {
                Some(&[$($lit),+])
            }

            fn check(
                &self,
                ast_check_ctx: &$crate::rules::backend::CheckCtx,
                tree: &tree_sitter::Tree,
            ) -> Vec<$crate::diagnostic::Diagnostic> {
                let ast_check_source = ast_check_ctx.source.as_bytes();
                let mut ast_check_diagnostics: Vec<$crate::diagnostic::Diagnostic> = Vec::new();
                $crate::rules::walker::walk_tree(tree, |ast_check_node| {
                    #[allow(unused_variables)]
                    let $node = ast_check_node;
                    #[allow(unused_variables)]
                    let $source: &[u8] = ast_check_source;
                    #[allow(unused_variables)]
                    let $ctx: &$crate::rules::backend::CheckCtx = ast_check_ctx;
                    #[allow(unused_variables)]
                    let $diagnostics: &mut Vec<$crate::diagnostic::Diagnostic> = &mut ast_check_diagnostics;
                    $($body)*
                });
                ast_check_diagnostics
            }
        }
    };

    (on [$($kind:expr),+ $(,)?] => |$node:ident, $source:ident, $ctx:ident, $diagnostics:ident| $($body:tt)*) => {
        #[derive(Debug)]
        pub struct Check;

        impl $crate::rules::backend::AstCheck for Check {
            fn interested_kinds(&self) -> Option<&'static [&'static str]> {
                Some(&[$($kind),+])
            }

            fn visit_node(
                &self,
                ast_check_node: tree_sitter::Node,
                ast_check_ctx: &$crate::rules::backend::CheckCtx,
                _ast_check_state: Option<&mut dyn std::any::Any>,
                ast_check_diagnostics: &mut Vec<$crate::diagnostic::Diagnostic>,
            ) {
                #[allow(unused_variables)]
                let $node = ast_check_node;
                #[allow(unused_variables)]
                let $source: &[u8] = ast_check_ctx.source.as_bytes();
                #[allow(unused_variables)]
                let $ctx: &$crate::rules::backend::CheckCtx = ast_check_ctx;
                #[allow(unused_variables)]
                let $diagnostics: &mut Vec<$crate::diagnostic::Diagnostic> = ast_check_diagnostics;
                $($body)*
            }
        }
    };

    (|$node:ident, $source:ident, $ctx:ident, $diagnostics:ident| $($body:tt)*) => {
        #[derive(Debug)]
        pub struct Check;

        impl $crate::rules::backend::AstCheck for Check {
            fn check(
                &self,
                ast_check_ctx: &$crate::rules::backend::CheckCtx,
                tree: &tree_sitter::Tree,
            ) -> Vec<$crate::diagnostic::Diagnostic> {
                let ast_check_source = ast_check_ctx.source.as_bytes();
                let mut ast_check_diagnostics: Vec<$crate::diagnostic::Diagnostic> = Vec::new();
                $crate::rules::walker::walk_tree(tree, |ast_check_node| {
                    #[allow(unused_variables)]
                    let $node = ast_check_node;
                    #[allow(unused_variables)]
                    let $source: &[u8] = ast_check_source;
                    #[allow(unused_variables)]
                    let $ctx: &$crate::rules::backend::CheckCtx = ast_check_ctx;
                    #[allow(unused_variables)]
                    let $diagnostics: &mut Vec<$crate::diagnostic::Diagnostic> = &mut ast_check_diagnostics;
                    $($body)*
                });
                ast_check_diagnostics
            }
        }
    };
}

#[cfg(test)]
mod macro_prefilter_tests {
    use crate::rules::backend::AstCheck;

    mod with_prefilter_multiplexed {
        crate::ast_check! {
            on ["call_expression"] prefilter = ["foo", "bar"]
            => |node, source, ctx, diagnostics|
            let _ = (node, source, ctx, diagnostics);
        }
    }

    mod with_prefilter_legacy {
        crate::ast_check! {
            prefilter = ["needle"] => |node, source, ctx, diagnostics|
            let _ = (node, source, ctx, diagnostics);
        }
    }

    mod without_prefilter_multiplexed {
        crate::ast_check! {
            on ["call_expression"] => |node, source, ctx, diagnostics|
            let _ = (node, source, ctx, diagnostics);
        }
    }

    mod without_prefilter_legacy {
        crate::ast_check! {
            |node, source, ctx, diagnostics|
            let _ = (node, source, ctx, diagnostics);
        }
    }

    #[test]
    fn multiplexed_with_prefilter_returns_slice() {
        assert_eq!(
            with_prefilter_multiplexed::Check.prefilter(),
            Some(&["foo", "bar"][..]),
        );
        assert_eq!(
            with_prefilter_multiplexed::Check.interested_kinds(),
            Some(&["call_expression"][..]),
        );
    }

    #[test]
    fn legacy_with_prefilter_returns_slice() {
        assert_eq!(
            with_prefilter_legacy::Check.prefilter(),
            Some(&["needle"][..]),
        );
    }

    #[test]
    fn multiplexed_without_prefilter_returns_none() {
        assert!(without_prefilter_multiplexed::Check.prefilter().is_none());
        assert_eq!(
            without_prefilter_multiplexed::Check.interested_kinds(),
            Some(&["call_expression"][..]),
        );
    }

    #[test]
    fn legacy_without_prefilter_returns_none() {
        assert!(without_prefilter_legacy::Check.prefilter().is_none());
    }
}
