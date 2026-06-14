use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["const "])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };

        // Story files (a `*.stories.*` name, or any file inside a `stories/` or
        // `storybook/` directory) hold story-argument fixtures, option lists, and
        // framework-magic names like `__namedExportsOrder` — local story data
        // following camelCase by convention, not application-wide compile-time
        // invariants (issue #1668).
        if ctx.file.path_segments.in_storybook {
            return;
        }

        if !decl.kind.is_const() {
            return;
        }

        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::Program(_) | AstKind::ExportNamedDeclaration(_)) {
            return;
        }

        // SvelteKit route modules (`+page.ts`, `+layout.server.ts`, `+server.ts`,
        // …) expose page options through `const` exports whose names are the
        // framework's protocol: `export const prerender = true`, `ssr`, `csr`.
        // SvelteKit reads them by exact lowercase name, so they cannot be
        // SCREAMING_SNAKE_CASE (issue #1586). The route-file gate keeps the
        // exemption from covering an ordinary lowercase const in the same file.
        let in_svelte_route = ctx
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(crate::rules::path_utils::is_sveltekit_route_file);

        for declarator in &decl.declarations {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                continue;
            };

            let name = id.name.as_str();

            if in_svelte_route && is_sveltekit_page_option(name) {
                continue;
            }

            if !is_primitive_init(declarator) {
                continue;
            }

            if super::is_screaming_snake(name) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, id.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Top-level constant `{name}` is not in `SCREAMING_SNAKE_CASE`."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// SvelteKit's documented page-option export names. In a SvelteKit route module
/// these `const` exports form the framework's protocol — SvelteKit reads them by
/// exact lowercase name, so they cannot be renamed to SCREAMING_SNAKE_CASE.
/// See <https://svelte.dev/docs/kit/page-options>.
fn is_sveltekit_page_option(name: &str) -> bool {
    matches!(
        name,
        "prerender" | "ssr" | "csr" | "trailingSlash" | "config" | "actions" | "load" | "entries"
    )
}

fn is_primitive_init(declarator: &oxc_ast::ast::VariableDeclarator) -> bool {
    let Some(init) = &declarator.init else {
        return false;
    };
    matches!(
        init,
        oxc_ast::ast::Expression::NumericLiteral(_)
            | oxc_ast::ast::Expression::BooleanLiteral(_)
    ) || is_unary_numeric(init)
        || is_array_of_literals(init)
}

fn is_unary_numeric(expr: &oxc_ast::ast::Expression) -> bool {
    if let oxc_ast::ast::Expression::UnaryExpression(u) = expr {
        return matches!(u.argument, oxc_ast::ast::Expression::NumericLiteral(_));
    }
    false
}

/// Treats an array as a magic-constant literal only when every element is a
/// numeric or boolean literal. Arrays containing string literals are named
/// configuration lists (Vite `optimizeDeps`, allowed-origin lists, feature-flag
/// keys) that follow camelCase by ecosystem convention, so they are exempt.
fn is_array_of_literals(expr: &oxc_ast::ast::Expression) -> bool {
    let oxc_ast::ast::Expression::ArrayExpression(arr) = expr else {
        return false;
    };
    if arr.elements.is_empty() {
        return false;
    }
    arr.elements.iter().all(|el| {
        matches!(
            el,
            oxc_ast::ast::ArrayExpressionElement::NumericLiteral(_)
                | oxc_ast::ast::ArrayExpressionElement::BooleanLiteral(_)
        )
    })
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
