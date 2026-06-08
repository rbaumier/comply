use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["interface"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // .d.ts files use interface for declaration merging / module augmentation.
        if ctx.path.to_str().is_some_and(|p| p.ends_with(".d.ts")) {
            return Vec::new();
        }

        // First pass: collect all names used in `implements` clauses.
        let mut implemented = FxHashSet::default();
        for node in semantic.nodes().iter() {
            if let AstKind::TSClassImplements(impl_clause) = node.kind() {
                // The expression is a TSTypeName — extract the identifier.
                let name = type_name_str(&impl_clause.expression);
                if let Some(n) = name {
                    implemented.insert(n.to_string());
                }
            }
        }

        // Second pass: flag interface declarations without extends and not implemented.
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::TSInterfaceDeclaration(iface) = node.kind() else {
                continue;
            };
            // Skip if it has an extends clause.
            if !iface.extends.is_empty() {
                continue;
            }
            let name = iface.id.name.as_str();
            if implemented.contains(name) {
                continue;
            }
            // Interfaces inside `declare module` are for augmentation / merging.
            if is_inside_declare_module(semantic, node) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, iface.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Interface '{name}' has no extends clause and is not implemented — use \
                     `type {name} = {{ ... }}` instead. Types support \
                     unions, intersections, mapped types, and conditional \
                     types. Keep `interface` for extension, declaration \
                     merging, and `implements` only."
                ),
                severity: super::META.severity,
                span: None,
            });
        }
        diagnostics
    }
}

fn is_inside_declare_module<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    node: &oxc_semantic::AstNode<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
        if matches!(ancestor.kind(), AstKind::TSModuleDeclaration(_)) {
            return true;
        }
    }
    false
}

fn type_name_str<'a>(name: &'a oxc_ast::ast::TSTypeName<'a>) -> Option<&'a str> {
    match name {
        oxc_ast::ast::TSTypeName::IdentifierReference(id) => Some(id.name.as_str()),
        oxc_ast::ast::TSTypeName::QualifiedName(_) | oxc_ast::ast::TSTypeName::ThisExpression(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_plain_interface() {
        assert_eq!(run_on("interface User { name: string; }").len(), 1);
    }

    #[test]
    fn allows_interface_with_extends() {
        assert!(run_on("interface Admin extends User { role: string; }").is_empty());
    }

    #[test]
    fn allows_type_alias() {
        assert!(run_on("type User = { name: string };").is_empty());
    }

    #[test]
    fn allows_interface_with_implements() {
        let code = r#"
            interface Serializable { serialize(): string; }
            class User implements Serializable { serialize() { return ""; } }
        "#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_interface_with_generic_implements() {
        let code = r#"
            interface Repository<T> { find(id: string): T; }
            class UserRepo implements Repository<User> { find(id: string) { return null; } }
        "#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_interface_not_implemented() {
        let code = r#"
            interface Unused { foo: string; }
            class User implements OtherInterface {}
        "#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_interface_in_declare_module() {
        let code = r#"
            declare module "@tanstack/react-router" {
                interface Register { router: typeof router; }
            }
        "#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_dts_file() {
        let diags = crate::rules::test_helpers::run_oxc_ts_with_path(
            "interface ImportMetaEnv { readonly VITE_API: string; }",
            &Check,
            "env.d.ts",
        );
        assert!(diags.is_empty());
    }
}
