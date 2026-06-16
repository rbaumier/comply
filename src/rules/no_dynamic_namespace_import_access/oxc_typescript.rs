//! OXC backend for no-dynamic-namespace-import-access.
//!
//! Flags dynamic (computed) member reads on a namespace-import binding —
//! `import * as ns from "m"; ns["foo"]`, `ns[0]`, `ns[key]`. Computed access
//! defeats a bundler's tree-shaking: the accessed property is not statically
//! known, so the whole namespace must stay in the bundle.
//!
//! Static access (`ns.foo`) is left to `import-namespace`, which checks it
//! against the module's real exports. Computed *assignment* targets
//! (`ns[key] = …`, `ns[k]++`, `for (ns[k] of …)`, `[ns[k]] = …`) write to a
//! slot rather than reading the namespace, so they are not flagged. Type-only
//! namespace imports (`import type * as ns`) are erased at compile time and
//! never reach a bundle, so they are exempt.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::{GetSpan, Span};

/// True when `obj` resolves to a value namespace-import binding
/// (`import * as ns from "m"`). A local binding that shadows the name, an
/// unresolved reference, or a type-only import (`import type * as ns`) all
/// return false: the access then does not point at a runtime namespace object
/// that would bloat the bundle.
fn refers_to_namespace_import(
    obj: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = obj.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl = scoping.symbol_declaration(sym_id);
    matches!(semantic.nodes().kind(decl), AstKind::ImportNamespaceSpecifier(_))
}

/// True when the computed member at `member_span` occupies an assignment-target
/// position — the left side of an assignment (`ns[k] = …`, `ns[k] += …`), an
/// update target (`ns[k]++`), a for-in/of binding (`for (ns[k] of …)`), or a
/// destructuring target (`[ns[k]] = …`). In those positions the namespace
/// binding is written through, not read, so tree-shaking is unaffected.
fn is_assignment_target(parent: AstKind, member_span: Span) -> bool {
    match parent {
        AstKind::UpdateExpression(_)
        | AstKind::ArrayAssignmentTarget(_)
        | AstKind::ObjectAssignmentTarget(_)
        | AstKind::AssignmentTargetWithDefault(_) => true,
        AstKind::AssignmentExpression(assign) => assign.left.span() == member_span,
        AstKind::ForOfStatement(stmt) => stmt.left.span() == member_span,
        AstKind::ForInStatement(stmt) => stmt.left.span() == member_span,
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ComputedMemberExpression(member) = node.kind() else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        let parent = semantic.nodes().parent_kind(node.id());
        if is_assignment_target(parent, member.span) {
            return;
        }
        if !refers_to_namespace_import(obj, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid accessing namespace imports dynamically — it prevents tree shaking \
                      and increases bundle size. Use a static property access or a named import."
                .into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // ── Invalid cases (mirror Biome's invalid.js) ──────────────────────────

    #[test]
    fn flags_string_literal_computed_key() {
        let d = run_on("import * as foo from \"foo\";\nfoo[\"bar\"];");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].rule_id, "no-dynamic-namespace-import-access");
    }

    #[test]
    fn flags_numeric_computed_key() {
        let d = run_on("import * as foo from \"foo\";\nfoo[1];");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_variable_computed_key() {
        let d = run_on("import * as foo from \"foo\";\nconst key = \"bar\";\nfoo[key];");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_each_occurrence() {
        // Biome's invalid.js fixture: three diagnostics.
        let d = run_on(
            "import * as foo from \"foo\";\n\
             foo[\"bar\"];\n\
             foo[1];\n\
             const key = \"bar\";\n\
             foo[key];",
        );
        assert_eq!(d.len(), 3, "{d:?}");
    }

    #[test]
    fn flags_optional_chained_computed_access() {
        // `foo?.[k]` is still a dynamic read of the namespace.
        let d = run_on("import * as foo from \"foo\";\nfoo?.[\"bar\"];");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_computed_member_on_assignment_rhs() {
        // The namespace is read on the right-hand side, not written.
        let d = run_on("import * as foo from \"foo\";\nlet x;\nx = foo[\"bar\"];");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // ── Valid cases (mirror Biome's valid.js) ──────────────────────────────

    #[test]
    fn allows_static_member_access() {
        assert!(run_on("import * as foo from \"foo\";\nfoo.bar;").is_empty());
    }

    #[test]
    fn allows_named_import_usage() {
        assert!(run_on("import { bar } from \"foo\";\nbar;").is_empty());
    }

    #[test]
    fn allows_computed_assignment_target() {
        // `foo[key] = "bar"` writes to a slot; Biome treats the LHS as a
        // computed *assignment*, not a computed member read, so it is valid.
        let d = run_on("import * as foo from \"foo\";\nconst key = \"bar\";\nfoo[key] = \"bar\";");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_computed_compound_assignment_target() {
        let d = run_on("import * as foo from \"foo\";\nfoo[\"n\"] += 1;");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_computed_update_target() {
        let d = run_on("import * as foo from \"foo\";\nfoo[\"n\"]++;");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_computed_for_of_target() {
        let d = run_on("import * as foo from \"foo\";\nfor (foo[\"k\"] of [1]) {}");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_computed_destructuring_target() {
        let d = run_on("import * as foo from \"foo\";\n[foo[\"k\"]] = [1];");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_computed_access_on_non_namespace_binding() {
        // Biome's third valid example: a plain object indexed dynamically.
        let d = run_on(
            "import messages from \"i18n\";\n\
             const map = { hello: messages.hello, goodbye: messages.goodbye };\n\
             const dynamicKey = \"hello\";\n\
             map[dynamicKey];",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn ignores_type_only_namespace_import() {
        // `import type * as foo` is erased at compile time — never bundled.
        let d = run_on("import type * as foo from \"foo\";\ntype T = (typeof foo)[\"bar\"];");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn ignores_local_binding_shadowing() {
        let d = run_on(
            "import * as foo from \"foo\";\n\
             function f() { const foo = { bar: 1 }; return foo[\"bar\"]; }\n\
             f();",
        );
        assert!(d.is_empty(), "{d:?}");
    }
}
