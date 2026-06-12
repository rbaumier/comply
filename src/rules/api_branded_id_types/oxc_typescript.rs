//! api-branded-id-types OxcCheck backend — flag function parameters named
//! `*Id` / `*_id` typed as bare `string` or `number` in exported functions.
//!
//! Relaxation: when the parameter is used exclusively as an equality
//! comparison operand inside the function body (and never returned, stored,
//! or passed on), the rule does not flag — the value flows nowhere
//! downstream so the brand would buy nothing.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, BinaryOperator, TSType};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FormalParameter]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FormalParameter(param) = node.kind() else {
            return;
        };

        // Extract parameter name
        let BindingPattern::BindingIdentifier(ident) = &param.pattern else {
            return;
        };
        let name = ident.name.as_str();
        if !name_looks_like_id(name) {
            return;
        }

        // Check type annotation is bare `string` or `number`
        let Some(type_ann) = &param.type_annotation else {
            return;
        };
        let Some(kind) = bare_primitive_kind(&type_ann.type_annotation) else {
            return;
        };

        // Check if in exported context
        if !is_in_exported_context(node.id(), semantic) {
            return;
        }

        // Published-library entry points (package.json declares `main`/`module`/
        // `exports`) have public signatures fixed by an external contract — e.g.
        // Azure SDK clients whose ID params mirror the REST spec. Branding those
        // IDs would be a breaking change for consumers, so the smell does not apply.
        if ctx
            .project
            .nearest_package_json(ctx.path)
            .is_some_and(|pkg| pkg.is_library)
        {
            return;
        }

        if is_comparison_only_usage(ident.symbol_id.get(), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Parameter `{name}: {kind}` uses a raw primitive — use a branded ID type so unrelated IDs can't be swapped at call sites."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn name_looks_like_id(name: &str) -> bool {
    if name == "id" {
        return true;
    }
    if name.ends_with("_id") && name.len() > 3 {
        return true;
    }
    // camelCase: ends with "Id" and preceded by lowercase
    if name.ends_with("Id") && name.len() > 2 {
        let prev = name.as_bytes()[name.len() - 3];
        if prev.is_ascii_lowercase() {
            return true;
        }
    }
    false
}

fn bare_primitive_kind(ts_type: &TSType<'_>) -> Option<&'static str> {
    match ts_type {
        TSType::TSStringKeyword(_) => Some("string"),
        TSType::TSNumberKeyword(_) => Some("number"),
        _ => None,
    }
}

/// Returns `true` when every resolved reference to `symbol_id` is the direct
/// operand of an equality comparison (`===`, `!==`, `==`, `!=`) and there is
/// at least one such reference. Parenthesised wrappers are transparent.
///
/// When `symbol_id` is `None` (no resolved binding), returns `false` so the
/// caller falls back to flagging.
fn is_comparison_only_usage(
    symbol_id: Option<oxc_semantic::SymbolId>,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let Some(symbol_id) = symbol_id else {
        return false;
    };
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();

    let mut has_reference = false;
    for reference in scoping.get_resolved_references(symbol_id) {
        has_reference = true;
        let ref_id = reference.node_id();
        if !is_equality_operand(ref_id, nodes) {
            return false;
        }
    }
    has_reference
}

/// Returns `true` when `ref_id` is a direct operand of an equality comparison
/// (`===`, `!==`, `==`, `!=`), transparent to parenthesised wrappers.
fn is_equality_operand(ref_id: oxc_semantic::NodeId, nodes: &oxc_semantic::AstNodes) -> bool {
    let mut current = ref_id;
    loop {
        let parent_id = nodes.parent_id(current);
        if parent_id == current {
            return false;
        }
        match nodes.kind(parent_id) {
            AstKind::ParenthesizedExpression(_) => {
                current = parent_id;
            }
            AstKind::BinaryExpression(bin) => {
                return matches!(
                    bin.operator,
                    BinaryOperator::Equality
                        | BinaryOperator::StrictEquality
                        | BinaryOperator::Inequality
                        | BinaryOperator::StrictInequality
                );
            }
            _ => return false,
        }
    }
}

fn is_in_exported_context(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node_id;

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            AstKind::Function(_) => {
                // Check if this function is exported
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id
                    && let AstKind::ExportNamedDeclaration(_) = nodes.get_node(gp_id).kind() {
                        return true;
                    }
                return false;
            }
            AstKind::ArrowFunctionExpression(_) => {
                // Check parent chain: VariableDeclarator -> VariableDeclaration -> ExportNamedDeclaration
                let mut up_id = nodes.parent_id(parent_id);
                loop {
                    if up_id == nodes.parent_id(up_id) {
                        return false;
                    }
                    match nodes.get_node(up_id).kind() {
                        AstKind::VariableDeclarator(_)
                        | AstKind::VariableDeclaration(_) => {
                            up_id = nodes.parent_id(up_id);
                        }
                        AstKind::ExportNamedDeclaration(_) => return true,
                        _ => return false,
                    }
                }
            }
            AstKind::MethodDefinition(_) => {
                // Check if the enclosing class is exported
                let mut up_id = nodes.parent_id(parent_id);
                loop {
                    if up_id == nodes.parent_id(up_id) {
                        return false;
                    }
                    match nodes.get_node(up_id).kind() {
                        AstKind::ClassBody(_) => {
                            up_id = nodes.parent_id(up_id);
                        }
                        AstKind::Class(_) => {
                            let class_parent_id = nodes.parent_id(up_id);
                            if class_parent_id != up_id
                                && let AstKind::ExportNamedDeclaration(_) =
                                    nodes.get_node(class_parent_id).kind()
                                {
                                    return true;
                                }
                            return false;
                        }
                        _ => return false,
                    }
                }
            }
            _ => {}
        }
        current_id = parent_id;
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    /// Run the check against `source` with a real `ProjectCtx` rooted at a
    /// tempdir whose `package.json` is `pkg_json` — exercises the
    /// published-library relaxation, which depends on `nearest_package_json`.
    fn run_with_pkg(pkg_json: &str, source: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use oxc_allocator::Allocator;
        use oxc_parser::Parser as OxcParser;
        use oxc_semantic::SemanticBuilder;
        use oxc_span::SourceType;

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join("src/api/context.ts");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::TypeScript,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = std::fs::canonicalize(&file_path).unwrap();

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, source, &project);

        let mut diagnostics = Vec::new();
        let kinds = Check.interested_kinds();
        for node in semantic.nodes().iter() {
            if kinds.contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    #[test]
    fn flags_raw_string_id_in_exported_function() {
        let d = run("export function getOrder(orderId: string) { return orderId; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("orderId"));
    }

    #[test]
    fn allows_branded_id_type() {
        assert!(run("export function getOrder(orderId: OrderId) { return orderId; }").is_empty());
    }

    // --- Issue #184 regression: comparison-only usage ---

    #[test]
    fn allows_comparison_only_usage_in_exported_function() {
        // The user's exact repro from issue #184.
        let src = r#"
            export function invalidateCachedSessionsByUserId(userId: string): void {
                for (const [key, entry] of cache) {
                    if (entry.data.session.userId === userId) {
                        cache.delete(key);
                    }
                }
            }
        "#;
        let diagnostics = run(src);
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    }

    #[test]
    fn allows_comparison_only_usage_with_loose_equality() {
        let src = r#"
            export function matchById(userId: string): boolean {
                return current.userId == userId;
            }
        "#;
        let diagnostics = run(src);
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    }

    #[test]
    fn allows_comparison_only_usage_with_inequality() {
        let src = r#"
            export function differs(userId: string): boolean {
                return other.userId !== userId;
            }
        "#;
        let diagnostics = run(src);
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    }

    #[test]
    fn allows_comparison_only_usage_with_parenthesised_operand() {
        let src = r#"
            export function check(userId: string): boolean {
                return ((entry.userId) === (userId));
            }
        "#;
        let diagnostics = run(src);
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    }

    #[test]
    fn flags_when_parameter_is_returned() {
        let src = r#"
            export function check(userId: string): string {
                if (entry.userId === userId) {
                    return userId;
                }
                return "";
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_parameter_is_stored_on_object() {
        let src = r#"
            export function check(userId: string): void {
                obj.id = userId;
                if (entry.userId === userId) {
                    return;
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_parameter_is_passed_to_another_function() {
        let src = r#"
            export function check(userId: string): void {
                doStuff(userId);
                if (entry.userId === userId) {
                    return;
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_parameter_is_used_in_template_literal() {
        let src = r#"
            export function check(userId: string): void {
                log(`looking up ${userId}`);
                if (entry.userId === userId) {
                    return;
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_parameter_is_member_accessed() {
        // Member access escapes comparison-only — widened-string params still flag.
        let src = r#"
            export function check(userId: string): number {
                return userId.length;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_simple_positive_case_with_find() {
        let src = r#"
            export function load(userId: string) {
                return db.users.find(userId);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // --- Issue #1083 regression: published-library entry points ---

    #[test]
    fn allows_id_param_in_published_library_entry_point() {
        // The user's exact repro from issue #1083: an Azure SDK client function
        // whose `subscriptionId: string` mirrors the ARM REST spec. The package
        // is published (declares `main`/`module`/`exports`), so branding the ID
        // would be a breaking change for consumers and the rule must not fire.
        let pkg = r#"{
            "name": "@azure/arm-maps",
            "main": "./dist/commonjs/index.js",
            "module": "./dist/esm/index.js",
            "exports": { ".": "./dist/esm/index.js" }
        }"#;
        let src = r#"
            export function createAzureMapsManagement(
                credential: TokenCredential,
                subscriptionId: string,
                options: AzureMapsManagementClientOptionalParams = {},
            ): AzureMapsManagementContext {
                return getClient(credential, subscriptionId, options);
            }
        "#;
        let diagnostics = run_with_pkg(pkg, src);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn flags_id_param_in_non_library_application_package() {
        // The same signature in an application package (no `main`/`module`/
        // `exports`) is the rule's legitimate target — the export is the app's
        // own code, so branding the ID is safe and the smell still fires.
        let pkg = r#"{ "name": "my-app", "private": true }"#;
        let src = r#"
            export function createClient(
                subscriptionId: string,
            ): unknown {
                return getClient(subscriptionId);
            }
        "#;
        assert_eq!(run_with_pkg(pkg, src).len(), 1);
    }
}
