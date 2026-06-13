use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::collections::HashSet;
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
        let mut implemented = HashSet::new();
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
            // Interfaces describing only callable/constructable shapes (`(...): T`
            // or `new (...): T`) are the idiomatic TypeScript form for those — the
            // `type` rewrite is valid but conventionally avoided (cf. the stdlib's
            // `ArrayConstructor`, `ObjectConstructor`).
            if is_callable_or_constructable_only(iface) {
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

/// `true` when the interface has at least one member and every member is a call
/// signature (`(...): T`) or construct signature (`new (...): T`).
fn is_callable_or_constructable_only(iface: &oxc_ast::ast::TSInterfaceDeclaration) -> bool {
    let members = &iface.body.body;
    !members.is_empty()
        && members.iter().all(|member| {
            matches!(
                member,
                oxc_ast::ast::TSSignature::TSCallSignatureDeclaration(_)
                    | oxc_ast::ast::TSSignature::TSConstructSignatureDeclaration(_)
            )
        })
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
    fn allows_construct_signature_only_interface() {
        // https://github.com/rbaumier/comply/issues/1926
        let code = r#"
            export interface PluginConstructor<
              T extends DragDropManager<any, any> = DragDropManager<any, any>,
              U extends Plugin<T> = Plugin<T>,
              V extends PluginOptions = InferPluginOptions<U>,
            > {
              new (manager: T, options?: V): U;
            }
        "#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_call_signature_only_interface() {
        assert!(run_on("interface Comparator { (a: number, b: number): number; }").is_empty());
    }

    #[test]
    fn flags_interface_mixing_property_and_construct_signature() {
        let code = "interface Factory { kind: string; new (): Factory; }";
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn flags_empty_interface() {
        assert_eq!(run_on("interface Empty {}").len(), 1);
    }

    #[test]
    fn allows_dts_file() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "interface ImportMetaEnv { readonly VITE_API: string; }", "env.d.ts");
        assert!(diags.is_empty());
    }
}
