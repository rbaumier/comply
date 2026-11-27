//! testing-no-shared-state OXC backend.
//! Flags program-level `let`/`var` assigned inside `test`/`it`.
//! In test files, also flags writes on named/default imports.
//! A hook writing the same binding or path suppresses it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentTarget, BindingPattern, Expression, UnaryOperator, VariableDeclarationKind,
};
use oxc_semantic::{AstNode, AstNodes, NodeId};
use oxc_span::{GetSpan, Span};
use rustc_hash::FxHashSet;
use std::sync::Arc;

/// Mutating array/collection methods.
const MUTATING_METHODS: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
    "set",
    "delete",
    "clear",
    "add",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[] // full-program analysis
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let bindings = program_bindings(semantic.nodes());
        let is_test_file = ctx.file.path_segments.in_test_dir;
        if bindings.is_empty() && !is_test_file {
            return Vec::new();
        }
        let names: FxHashSet<&str> = bindings.iter().map(|(name, _)| *name).collect();

        let mut mutated_in_tests = FxHashSet::default();
        let mut reset_in_hooks = FxHashSet::default();
        let mut import_writes_in_tests = Vec::new();
        let mut import_writes_in_hooks = Vec::new();
        for (call_id, is_test) in semantic.nodes().iter().filter_map(test_or_hook_call) {
            let (names_found, imports_found) = if is_test {
                (&mut mutated_in_tests, &mut import_writes_in_tests)
            } else {
                (&mut reset_in_hooks, &mut import_writes_in_hooks)
            };
            collect_mutations_in_call(semantic, call_id, &names, names_found, imports_found);
        }

        let mut diagnostics = Vec::new();
        if is_test_file {
            let reset_paths: FxHashSet<&str> = import_writes_in_hooks
                .iter()
                .map(|(path, _)| *path)
                .collect();
            import_writes_in_tests.sort_unstable_by_key(|(_, offset)| *offset);
            import_writes_in_tests.dedup();
            diagnostics.extend(
                import_writes_in_tests
                    .into_iter()
                    .filter(|(path, _)| !reset_paths.contains(path))
                    .map(|(path, offset)| {
                        diagnostic(ctx, offset, format!(
                            "Imported '{path}' is mutated inside test() without being reset in beforeEach/afterEach/afterAll — the change leaks into later tests."
                        ))
                    }),
            );
        }
        diagnostics.extend(
            bindings
                .iter()
                .filter(|(name, _)| mutated_in_tests.contains(name) && !reset_in_hooks.contains(name))
                .map(|(name, offset)| {
                    diagnostic(ctx, *offset, format!(
                        "Top-level '{name}' is mutated inside test() without being reset in beforeEach/afterEach/afterAll — tests become order-dependent."
                    ))
                }),
        );
        diagnostics
    }
}

fn diagnostic(ctx: &CheckCtx, offset: u32, message: String) -> Diagnostic {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
    Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message,
        severity: Severity::Error,
        span: None,
    }
}

/// Program-level `let`/`var` names with their declarator offsets.
fn program_bindings<'a>(nodes: &AstNodes<'a>) -> Vec<(&'a str, u32)> {
    nodes
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::VariableDeclaration(decl)
                if matches!(
                    decl.kind,
                    VariableDeclarationKind::Let | VariableDeclarationKind::Var
                ) && is_at_program_level(nodes, node.id()) =>
            {
                Some(decl)
            }
            _ => None,
        })
        .flat_map(|decl| &decl.declarations)
        .filter_map(|declarator| match &declarator.id {
            BindingPattern::BindingIdentifier(ident) => {
                Some((ident.name.as_str(), declarator.span.start))
            }
            _ => None,
        })
        .collect()
}

fn is_at_program_level(nodes: &AstNodes, node_id: NodeId) -> bool {
    nodes
        .ancestor_kinds(node_id)
        .find(|kind| {
            !matches!(
                kind,
                AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
            )
        })
        .is_some_and(|kind| matches!(kind, AstKind::Program(_)))
}

/// A `test`/`it` call (`true`) or a reset hook call (`false`).
fn test_or_hook_call(node: &AstNode) -> Option<(NodeId, bool)> {
    let AstKind::CallExpression(call) = node.kind() else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    match callee.name.as_str() {
        "test" | "it" => Some((node.id(), true)),
        "beforeEach" | "afterEach" | "afterAll" => Some((node.id(), false)),
        _ => None,
    }
}

/// Collects writes inside the call at `call_id`.
/// Pre-order ids keep a call's descendants contiguous after it.
/// Program-level names go to `found`.
/// Import writes go to `import_writes` as (path text, offset).
fn collect_mutations_in_call<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    call_id: NodeId,
    names: &FxHashSet<&str>,
    found: &mut FxHashSet<&'a str>,
    import_writes: &mut Vec<(&'a str, u32)>,
) {
    let source = semantic.source_text();
    let nodes = semantic.nodes();
    let end = nodes.kind(call_id).span().end;
    let descendants = nodes
        .iter()
        .skip(call_id.index())
        .take_while(|node| node.kind().span().start < end);
    for node in descendants {
        if let Some((object, target_span)) = import_write_target(node.kind())
            && is_imported_object(object, semantic)
        {
            import_writes.push((target_span.source_text(source), target_span.start));
        }
        let mutated_name = match node.kind() {
            AstKind::AssignmentExpression(assign) => root_name_of_target(&assign.left),
            AstKind::CallExpression(call) => mutating_method_receiver(&call.callee),
            _ => None,
        };
        if let Some(name) = mutated_name.filter(|name| names.contains(name)) {
            found.insert(name);
        }
    }
}

/// Object and target span of a member or Object/Reflect write.
fn import_write_target<'a>(kind: AstKind<'a>) -> Option<(&'a Expression<'a>, Span)> {
    match kind {
        AstKind::AssignmentExpression(assign) => assign
            .left
            .as_member_expression()
            .map(|m| (m.object(), assign.left.span())),
        AstKind::UpdateExpression(update) => update
            .argument
            .as_member_expression()
            .map(|m| (m.object(), update.argument.span())),
        AstKind::UnaryExpression(unary) if unary.operator == UnaryOperator::Delete => unary
            .argument
            .as_member_expression()
            .map(|m| (m.object(), unary.argument.span())),
        AstKind::CallExpression(call) if is_object_write_call(&call.callee) => call
            .arguments
            .first()
            .and_then(|arg| arg.as_expression())
            .map(|arg| (arg, arg.span())),
        _ => None,
    }
}

/// Receiver name of a mutating method call (`list.push(x)`).
fn mutating_method_receiver<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    let Expression::StaticMemberExpression(member) = callee else {
        return None;
    };
    if !MUTATING_METHODS.contains(&member.property.name.as_str()) {
        return None;
    }
    root_name_of_expr(&member.object)
}

/// Extract the root identifier name from an assignment target.
fn root_name_of_target<'a>(target: &'a AssignmentTarget<'a>) -> Option<&'a str> {
    match target {
        AssignmentTarget::AssignmentTargetIdentifier(ident) => Some(ident.name.as_str()),
        AssignmentTarget::StaticMemberExpression(member) => root_name_of_expr(&member.object),
        AssignmentTarget::ComputedMemberExpression(member) => root_name_of_expr(&member.object),
        _ => None,
    }
}

fn root_name_of_expr<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

fn is_object_write_call(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    let Expression::Identifier(object) = &member.object else {
        return false;
    };
    matches!(
        (object.name.as_str(), member.property.name.as_str()),
        ("Object", "assign" | "defineProperty") | ("Reflect", "set" | "deleteProperty")
    )
}

/// True when the chain root is a named/default import.
/// Namespace imports stay with `no-import-assign`.
fn is_imported_object(mut expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    while let Some(member) = expr.as_member_expression() {
        expr = member.object();
    }
    let Expression::Identifier(ident) = expr else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|id| scoping.get_reference(id).symbol_id())
    else {
        return false;
    };
    matches!(
        semantic.nodes().kind(scoping.symbol_declaration(symbol_id)),
        AstKind::ImportSpecifier(_) | AstKind::ImportDefaultSpecifier(_)
    )
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
    use crate::files::Language;
    use crate::rules::file_ctx::FileCtx;

    fn run(src: &str, path: &str) -> Vec<Diagnostic> {
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(
            std::path::Path::new(path),
            src,
            Language::TypeScript,
            project,
        );
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, path, project, &file)
    }

    #[test]
    fn flags_each_mutation_form_on_import() {
        let src = r#"import { config } from "./config";
it("x", () => {
  config.apiUrl = "y";
  Object.assign(config, { a: 1 });
  Object.defineProperty(config, "b", { value: 1 });
  config.count++;
  config.count--;
  delete config.a;
  Reflect.set(config, "a", 1);
  Reflect.deleteProperty(config, "a");
});
"#;
        let diagnostics = run(src, "src/config.test.ts");
        let lines: Vec<usize> = diagnostics.iter().map(|d| d.line).collect();
        assert_eq!(lines, vec![3, 4, 5, 6, 7, 8, 9, 10]);
        assert!(diagnostics[0].message.contains("'config.apiUrl'"));
    }

    const IMPORT_WRITE: &str = r#"import { config } from "./config";
it("x", () => { config.apiUrl = "y" });
"#;

    fn with_hook(hook: &str, body: &str) -> String {
        format!("{IMPORT_WRITE}{hook}(() => {{ {body} }});\n")
    }

    #[test]
    fn reset_of_same_path_in_any_hook_suppresses() {
        for hook in ["beforeEach", "afterEach", "afterAll"] {
            let src = with_hook(hook, "config.apiUrl = original");
            assert!(run(&src, "src/config.test.ts").is_empty(), "{hook}");
        }
    }

    #[test]
    fn reset_of_other_path_does_not_suppress() {
        let src = with_hook("afterEach", "config.other = 1");
        let diagnostics = run(&src, "src/config.test.ts");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("'config.apiUrl'"));
    }

    #[test]
    fn namespace_and_local_shadow_not_reported() {
        let src = r#"import * as ns from "./m"; import { config } from "./c"; it("x", () => { ns.x = 1; const config = { a: 0 }; config.a = 1 })"#;
        assert!(run(src, "src/config.test.ts").is_empty());
    }

    #[test]
    fn import_mutation_in_non_test_file_not_reported() {
        assert!(run(IMPORT_WRITE, "src/config-user.ts").is_empty());
    }

    #[test]
    fn top_level_let_reported_unless_reset_in_before_each() {
        let src = "let n = 0; it(\"a\", () => { n = 1 });\n";
        let diagnostics = run(src, "src/n.test.ts");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 1);
        assert!(diagnostics[0].message.contains("'n'"));
        let reset = format!("{src}beforeEach(() => {{ n = 0 }});\n");
        assert!(run(&reset, "src/n.test.ts").is_empty());
    }
}
