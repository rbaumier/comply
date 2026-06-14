//! node-no-sync OXC backend — flag synchronous Node.js method calls (`*Sync()`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use oxc_semantic::{NodeId, Semantic};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Walks the ancestors of `start` until the innermost enclosing function-like
/// node, returning true when that function's name ends in `Sync`.
///
/// Covers `function fooSync(){}`, `const fooSync = () => {}` / `= function(){}`,
/// and method/property forms `fooSync(){}` / `fooSync: () => {}`. The innermost
/// function defines the synchronous contract, so nested non-Sync functions are
/// still flagged.
fn enclosing_function_is_sync(start: NodeId, semantic: &Semantic, source: &str) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(start) {
        if ancestor.id() == start {
            continue;
        }
        let name = match ancestor.kind() {
            AstKind::Function(func) => match &func.id {
                Some(id) => Some(id.name.as_str().to_string()),
                // Unnamed function expression: name comes from how it is bound.
                None => binding_name(ancestor.id(), semantic, source),
            },
            AstKind::ArrowFunctionExpression(_) => binding_name(ancestor.id(), semantic, source),
            _ => continue,
        };
        return name.is_some_and(|name| super::function_name_is_sync(&name));
    }
    false
}

/// Resolves the name a function expression / arrow is bound to, via its parent:
/// a `*Sync` `const`/variable, or an object property / class method whose key
/// ends in `Sync`.
fn binding_name(func_id: NodeId, semantic: &Semantic, source: &str) -> Option<String> {
    let nodes = semantic.nodes();
    let parent = nodes.parent_node(func_id);
    match parent.kind() {
        AstKind::VariableDeclarator(decl) => match &decl.id {
            BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_string()),
            _ => None,
        },
        AstKind::MethodDefinition(method) => key_name(method.key.span(), source),
        AstKind::PropertyDefinition(prop) => key_name(prop.key.span(), source),
        AstKind::ObjectProperty(prop) => key_name(prop.key.span(), source),
        _ => None,
    }
}

fn key_name(span: oxc_span::Span, source: &str) -> Option<String> {
    source
        .get(span.start as usize..span.end as usize)
        .map(|s| s.trim_matches(['\'', '"', '`']).to_string())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Sync"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if super::allows_sync_node_api(ctx.path, ctx.source) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let (method_name, full_name) = match &call.callee {
            oxc_ast::ast::Expression::Identifier(id) => {
                let name = id.name.as_str();
                (name, name.to_string())
            }
            oxc_ast::ast::Expression::StaticMemberExpression(member) => {
                let prop = member.property.name.as_str();
                let full =
                    &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
                (prop, full.to_string())
            }
            _ => return,
        };

        // Only flag genuine Node.js core synchronous I/O methods, not arbitrary
        // identifiers ending in `Sync` (e.g. `flushSync`, `batchSync`).
        if !super::is_node_sync_io_method(method_name) {
            return;
        }

        // A sync call inside a function whose name ends in `Sync` is intentional:
        // the function's contract is to be synchronous (Node convention).
        if enclosing_function_is_sync(node.id(), semantic, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Unexpected sync method: `{full_name}()`. Use the async variant instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::{run_rule, run_rule_gated};

    // ── Part 1: test directories (central skip_in_test_dir gate) ──────────

    #[test]
    fn skips_sync_io_in_test_file() {
        // Issue #1344 — test helpers intentionally use sync IO for setup.
        let src = r#"
            const cliExists = fs.existsSync(CLI);
            function run(args) { return execSync(`node ${CLI}`); }
        "#;
        assert!(run_rule_gated(&Check, src, "packages/turbo-gen/__tests__/cli.test.ts").is_empty());
    }

    #[test]
    fn flags_sync_io_in_production_file() {
        // Negative space: production source is still gated-in and flagged.
        let d = run_rule_gated(&Check, "const data = fs.readFileSync(p);", "src/loader.ts");
        assert_eq!(d.len(), 1);
    }

    // ── Part 2: functions explicitly named *Sync ─────────────────────────

    #[test]
    fn allows_sync_io_in_sync_named_function_declaration() {
        // Issue #1344 — a `*Sync` function advertises its synchronous contract.
        let src = "function readConfigSync() { return fs.readFileSync(p); }";
        assert!(run_rule(&Check, src, "src/config.ts").is_empty());
    }

    #[test]
    fn allows_sync_io_in_sync_named_arrow() {
        let src = "const readConfigSync = () => fs.readFileSync(p);";
        assert!(run_rule(&Check, src, "src/config.ts").is_empty());
    }

    #[test]
    fn allows_sync_io_in_sync_named_function_expression() {
        let src = "const readConfigSync = function() { return fs.readFileSync(p); };";
        assert!(run_rule(&Check, src, "src/config.ts").is_empty());
    }

    #[test]
    fn allows_sync_io_in_sync_named_method() {
        let src = "class C { readConfigSync() { return fs.readFileSync(p); } }";
        assert!(run_rule(&Check, src, "src/config.ts").is_empty());
    }

    #[test]
    fn allows_sync_io_in_sync_named_object_property() {
        let src = "const o = { readConfigSync: () => fs.readFileSync(p) };";
        assert!(run_rule(&Check, src, "src/config.ts").is_empty());
    }

    #[test]
    fn flags_sync_io_in_non_sync_named_function() {
        // Negative space: a function NOT ending in Sync is still flagged.
        let d = run_rule(&Check, "function readConfig() { return fs.readFileSync(p); }", "src/config.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_sync_io_at_top_level() {
        let d = run_rule(&Check, "const data = fs.readFileSync(p);", "src/config.ts");
        assert_eq!(d.len(), 1);
    }
}
