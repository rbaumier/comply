use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, ExportDefaultDeclarationKind, Expression, ModuleExportName,
    VariableDeclarationKind,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

const COLLECTION_CTORS: &[&str] = &["Map", "Set", "Array", "WeakMap", "WeakSet"];
const WRITE_METHODS: &[&str] = &["push", "add", "set", "unshift", "splice"];

/// Names of local bindings re-exported via `export { name }` / `export { name as X }`
/// or `export default name`. An `export { x } from "mod"` re-export names a binding
/// of "mod", not a local one, so it is ignored.
fn specifier_exported_names<'a>(semantic: &'a oxc_semantic::Semantic<'a>) -> FxHashSet<&'a str> {
    let mut names = FxHashSet::default();
    for node in semantic.nodes().iter() {
        match node.kind() {
            AstKind::ExportNamedDeclaration(decl) if decl.source.is_none() => {
                for spec in &decl.specifiers {
                    let local = match &spec.local {
                        ModuleExportName::IdentifierReference(reference) => {
                            Some(reference.name.as_str())
                        }
                        ModuleExportName::IdentifierName(identifier) => {
                            Some(identifier.name.as_str())
                        }
                        ModuleExportName::StringLiteral(_) => None,
                    };
                    if let Some(local) = local {
                        names.insert(local);
                    }
                }
            }
            AstKind::ExportDefaultDeclaration(decl) => {
                if let ExportDefaultDeclarationKind::Identifier(reference) = &decl.declaration {
                    names.insert(reference.name.as_str());
                }
            }
            _ => {}
        }
    }
    names
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let nodes = semantic.nodes();

        // An exported collection cannot be proven unread from one file's AST —
        // another module may import and read it. Bindings exported via a later
        // `export { col }` / `export default col` are collected here; inline
        // `export const col = …` is detected per-declaration below.
        let exported_names = specifier_exported_names(semantic);

        // Pass 1: collect collection declarations (name, declaration span start).
        let mut collections: Vec<(&str, u32)> = Vec::new();

        for node in nodes.iter() {
            let AstKind::VariableDeclarator(decl) = node.kind() else {
                continue;
            };
            let parent_id = nodes.parent_id(node.id());
            let AstKind::VariableDeclaration(var_decl) = nodes.kind(parent_id) else {
                continue;
            };
            if var_decl.kind != VariableDeclarationKind::Const {
                continue;
            }
            // Inline `export const col = …`: the declaration is wrapped in an
            // `ExportNamedDeclaration`, so it is reachable from other modules.
            let grandparent_id = nodes.parent_id(parent_id);
            if matches!(
                nodes.kind(grandparent_id),
                AstKind::ExportNamedDeclaration(_)
            ) {
                continue;
            }
            let BindingPattern::BindingIdentifier(id) = &decl.id else {
                continue;
            };
            let Some(init) = &decl.init else { continue };
            let is_collection = match init.without_parentheses() {
                Expression::ArrayExpression(_) => true,
                Expression::NewExpression(new_expr) => {
                    if let Expression::Identifier(ctor) = &new_expr.callee {
                        COLLECTION_CTORS.contains(&ctor.name.as_str())
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if is_collection {
                collections.push((id.name.as_str(), id.span.start));
            }
        }

        if collections.is_empty() {
            return diagnostics;
        }

        // Pass 2: for each collection, classify all identifier references as
        // write (mutation method call) or read (anything else).
        for &(name, decl_start) in &collections {
            // Re-exported via `export { name }` / `export default name`:
            // another module may read it, so a single-file pass cannot prove
            // it unread.
            if exported_names.contains(name) {
                continue;
            }

            let mut is_written = false;
            let mut is_read = false;

            for node in nodes.iter() {
                let AstKind::IdentifierReference(id_ref) = node.kind() else {
                    continue;
                };
                if id_ref.name.as_str() != name {
                    continue;
                }
                if id_ref.span.start == decl_start {
                    continue;
                }
                // Check if this is a write: parent is StaticMemberExpression
                // that is callee of a CallExpression with a write method.
                let parent_id = nodes.parent_id(node.id());
                let is_write =
                    if let AstKind::StaticMemberExpression(member) = nodes.kind(parent_id) {
                        // Identifier must be the object, not the property.
                        let is_object = member.object.span().start == id_ref.span.start;
                        if is_object && WRITE_METHODS.contains(&member.property.name.as_str()) {
                            let grandparent_id = nodes.parent_id(parent_id);
                            matches!(nodes.kind(grandparent_id), AstKind::CallExpression(_))
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                if is_write {
                    is_written = true;
                } else {
                    is_read = true;
                }
            }

            if is_written && !is_read {
                let (line, column) = byte_offset_to_line_col(ctx.source, decl_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Collection `{name}` is populated but never read."),
                    severity: super::META.severity,
                    span: None,
                });
            }
        }

        diagnostics
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

    #[test]
    fn flags_pushed_but_never_read() {
        let src = r#"
const items = [];
items.push(1);
items.push(2);
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_set_add_but_never_read() {
        let src = r#"
const seen = new Set();
seen.add("a");
seen.add("b");
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_pushed_and_iterated() {
        let src = r#"
const items = [];
items.push(1);
items.forEach(x => console.log(x));
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pushed_and_returned() {
        let src = r#"
const items = [];
items.push(1);
return items;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_collection_passed_as_arg() {
        let src = r#"
const items = [];
items.push(1);
doSomething(items);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_exported_map_populated_but_not_locally_read() {
        let src = r#"
export const old_values = new Map();
old_values.set(source, value);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_exported_set_populated_but_not_locally_read() {
        let src = r#"
export const all_registered_events = new Set();
all_registered_events.add("click");
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_exported_array_populated_but_not_locally_read() {
        let src = r#"
export const role_schemas = [];
role_schemas.push({ name: "button" });
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_collection_exported_via_specifier() {
        let src = r#"
const seen = new Set();
seen.add("a");
export { seen };
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_collection_exported_via_renamed_specifier() {
        let src = r#"
const seen = new Set();
seen.add("a");
export { seen as visited };
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_collection_exported_as_default() {
        let src = r#"
const seen = new Set();
seen.add("a");
export default seen;
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_local_collection_populated_but_unread_despite_other_exports() {
        // A sibling export must not blanket-suppress a genuinely dead local
        // collection: only the exported binding is exempt.
        let src = r#"
export const used = new Set();
used.add("a");
console.log(used.has("a"));

const dead = new Map();
dead.set("k", "v");
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("dead"));
    }
}
