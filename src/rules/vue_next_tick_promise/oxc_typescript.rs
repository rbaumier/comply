//! vue-next-tick-promise oxc backend for TypeScript / JavaScript / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, IdentifierReference};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["nextTick", "$nextTick"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // The Promise form takes no arguments — only a callback-form call can
        // fire. Require the first argument to be a function expression.
        if !first_arg_is_function(call) {
            return;
        }

        if !callee_is_vue_next_tick(unwrap_parens(&call.callee), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Vue `nextTick` was called with a callback — await its returned Promise \
                      instead (`await nextTick()`)."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the call's first argument is an arrow or `function` expression
/// (looking through wrapping parentheses), e.g. `nextTick(() => {})` or
/// `nextTick(function () {})`.
fn first_arg_is_function(call: &oxc_ast::ast::CallExpression) -> bool {
    let Some(first) = call.arguments.first() else {
        return false;
    };
    let Some(expr) = first.as_expression() else {
        return false;
    };
    matches!(
        unwrap_parens(expr),
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
    )
}

/// True when `callee` is a reference to Vue's `nextTick`:
///
/// - identifier bound to a named import `nextTick` from `vue` (alias-aware),
/// - `<obj>.nextTick` where `<obj>` is the `Vue` global or a `* as Vue`
///   namespace import from `vue`,
/// - `this.$nextTick`.
fn callee_is_vue_next_tick(callee: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    match callee {
        Expression::Identifier(ident) => {
            reference_is_vue_named_import(ident, "nextTick", semantic)
        }
        Expression::StaticMemberExpression(member) => {
            let object = unwrap_parens(&member.object);
            // `this.$nextTick(...)` — the Options-API instance method.
            if matches!(object, Expression::ThisExpression(_)) {
                return member.property.name.as_str() == "$nextTick";
            }
            // `Vue.nextTick(...)` — namespace member.
            if member.property.name.as_str() != "nextTick" {
                return false;
            }
            let Expression::Identifier(obj) = object else {
                return false;
            };
            object_is_vue_namespace(obj, semantic)
        }
        _ => false,
    }
}

/// True when `ident` resolves to a named import whose **imported** name is
/// `imported_name` and whose source module is `vue`. Alias-aware: the local
/// binding may be renamed (`import { nextTick as nt } from 'vue'`). A local
/// variable, parameter, or import from another module resolves to a different
/// declaration and returns false.
fn reference_is_vue_named_import(
    ident: &IdentifierReference,
    imported_name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::ImportSpecifier;

    let Some(decl) = resolve_import_specifier(ident, semantic) else {
        return false;
    };
    let AstKind::ImportSpecifier(ImportSpecifier { imported, .. }) = decl else {
        return false;
    };
    imported
        .identifier_name()
        .is_some_and(|name| name.as_str() == imported_name)
        && import_source_is_vue(decl, semantic)
}

/// True when `ident` is the `Vue` framework global (an unbound reference named
/// `Vue`) or resolves to a `* as Vue` namespace import from `vue`.
fn object_is_vue_namespace(ident: &IdentifierReference, semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_ast::AstKind;

    match resolve_import_specifier(ident, semantic) {
        Some(decl @ AstKind::ImportNamespaceSpecifier(_)) => import_source_is_vue(decl, semantic),
        // No resolvable binding: treat a bare `Vue` reference as the global.
        None => ident.name.as_str() == "Vue",
        Some(_) => false,
    }
}

/// Resolve `ident` to the `AstKind` of its declaration node when that
/// declaration is an import specifier (named or namespace). Returns `None` for
/// any other binding or an unresolved reference.
fn resolve_import_specifier<'a>(
    ident: &IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<AstKind<'a>> {
    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    Some(semantic.nodes().kind(decl_node_id))
}

/// True when the import declaration enclosing `specifier` imports from `"vue"`.
fn import_source_is_vue(specifier: AstKind, semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;

    let specifier_span = specifier.span();
    semantic.nodes().iter().any(|node| {
        let AstKind::ImportDeclaration(decl) = node.kind() else {
            return false;
        };
        decl.source.value.as_str() == "vue"
            && decl.span.start <= specifier_span.start
            && specifier_span.end <= decl.span.end
    })
}

fn unwrap_parens<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    while let Expression::ParenthesizedExpression(paren) = current {
        current = &paren.expression;
    }
    current
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- Invalid (Biome `invalid.js`) ---

    #[test]
    fn flags_named_import_arrow_callback() {
        let src = "import { nextTick } from \"vue\";\nnextTick(() => { updateDom(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_named_import_function_callback() {
        let src = "import { nextTick } from \"vue\";\nnextTick(function () { updateDom(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_namespace_member_callback() {
        let src = "import * as Vue from \"vue\";\nVue.nextTick(() => { updateDom(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_this_next_tick_callback() {
        let src = "export default {\n  mounted() {\n    this.$nextTick(() => { updateDom(); });\n  },\n};";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- Valid (Biome `valid.js`) ---

    #[test]
    fn allows_await_next_tick() {
        let src = "import { nextTick } from \"vue\";\nawait nextTick();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_named_import_then() {
        let src = "import { nextTick } from \"vue\";\nnextTick().then(() => { updateDom(); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_namespace_member_then() {
        let src = "import * as Vue from \"vue\";\nVue.nextTick().then(() => { updateDom(); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_await_this_next_tick() {
        let src = "export default {\n  async mounted() {\n    await this.$nextTick();\n  },\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_local_next_tick_callback() {
        let src = "const localNextTick = (callback) => callback();\nlocalNextTick(() => { updateDom(); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_next_tick_with_non_callback_arg() {
        let src = "import { nextTick } from \"vue\";\nnextTick(\"not a callback\");";
        assert!(run_on(src).is_empty());
    }

    // --- Extra guards ---

    #[test]
    fn ignores_next_tick_without_vue_import() {
        // No `vue` import: a bare `nextTick(cb)` resolves to no vue binding.
        let src = "nextTick(() => { updateDom(); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_next_tick_imported_from_other_module() {
        let src = "import { nextTick } from \"./local\";\nnextTick(() => {});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_aliased_named_import() {
        let src = "import { nextTick as nt } from \"vue\";\nnt(() => { updateDom(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_non_vue_namespace_member() {
        let src = "import * as Other from \"other\";\nOther.nextTick(() => {});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_this_other_method() {
        let src = "export default {\n  mounted() {\n    this.doSomething(() => {});\n  },\n};";
        assert!(run_on(src).is_empty());
    }
}
