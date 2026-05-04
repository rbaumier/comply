//! Registry helpers + the macros every rule's `register()` and
//! `Check` impl call into.
//!
//! Two pieces:
//!
//! 1. **`build_rust_only_rule`** helper — collapses the boilerplate of
//!    constructing a `RuleDef` with its `backends` vec.
//!
//! 2. **`register_rust_only!`** / **`ast_check!`** macros — one-liner
//!    macros each rule's `mod.rs` calls instead of writing the full
//!    `RuleDef { meta, backends: vec![...] }` shape.
//!
//! Re-exported from `crate::rules` so the macros' `$crate::rules::*` paths
//! resolve transparently.

use crate::files::Language;

use super::RuleDef;
use super::backend::{self, AstCheck, Backend};
use super::meta::RuleMeta;

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
    let _ = std::marker::PhantomData::<&dyn backend::AstCheck>;
};

/// `RuleDef` for a Rust-only rule.
#[macro_export]
macro_rules! register_rust_only {
    ($meta:expr, $rust_mod:ident) => {
        $crate::rules::build_rust_only_rule($meta, Box::new($rust_mod::Check))
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
