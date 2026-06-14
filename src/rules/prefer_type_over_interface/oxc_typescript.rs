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

        // Library packages (package.json declares `main`/`module`/`exports`)
        // expose interfaces as public API so consumers can augment them via
        // declaration merging (`declare module 'pkg' { interface Foo { ... } }`),
        // which only works with `interface`, not `type`. A non-extends interface
        // here is a deliberate public-contract choice, not a smell.
        if ctx
            .project
            .nearest_package_json(ctx.path)
            .is_some_and(|pkg| pkg.is_library)
        {
            return Vec::new();
        }

        // First pass: collect all names used in `implements` clauses (by classes)
        // and in `extends` clauses (by other interfaces). An interface that any
        // class implements or any sibling interface extends must stay an
        // `interface`: it serves as an extension base and supports declaration
        // merging, neither of which a `type` alias provides.
        let mut referenced_as_base = HashSet::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::TSClassImplements(impl_clause) => {
                    // The expression is a TSTypeName — extract the identifier.
                    if let Some(n) = type_name_str(&impl_clause.expression) {
                        referenced_as_base.insert(n.to_string());
                    }
                }
                AstKind::TSInterfaceDeclaration(iface) => {
                    for heritage in &iface.extends {
                        // The expression is an `Expression`; the base name is an
                        // identifier reference (`extends Resource`).
                        if let Some(n) = heritage_base_name(&heritage.expression) {
                            referenced_as_base.insert(n.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        // Second pass: flag interface declarations without extends that no class
        // implements and no other interface extends.
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
            if referenced_as_base.contains(name) {
                continue;
            }
            // Interfaces inside an ambient context (`declare global { ... }` for
            // global augmentation, `declare module "..." { ... }` for module
            // augmentation) exist only to merge into / augment existing types,
            // which TypeScript supports for `interface` but not `type`.
            if crate::oxc_helpers::is_in_ambient_declaration(node.id(), semantic) {
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

/// Base interface name in an `extends` heritage expression. For `extends Foo`
/// the expression is an identifier reference; qualified bases (`extends a.B`)
/// reference an imported namespace, not a sibling interface, so they are ignored.
fn heritage_base_name<'a>(expr: &'a oxc_ast::ast::Expression<'a>) -> Option<&'a str> {
    match expr {
        oxc_ast::ast::Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
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

    /// Run the check against `source` with a real `ProjectCtx` rooted at a
    /// tempdir whose `package.json` is `pkg_json` — exercises the library
    /// relaxation, which depends on `nearest_package_json`.
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
        let file_path = dir.path().join("src/mapBuilders.ts");
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
        Check.run_on_semantic(&semantic, &ctx)
    }

    // Issue #1886: redux-toolkit's public-API interfaces in a library package.
    const ACTION_REDUCER_MAP_BUILDER: &str = r#"
        export interface ActionReducerMapBuilder<State> {
          addCase<ActionCreator extends TypedActionCreator<string>>(
            actionCreator: ActionCreator,
            reducer: CaseReducer<State, ReturnType<ActionCreator>>,
          ): ActionReducerMapBuilder<State>
          addMatcher<A>(
            matcher: TypeGuard<A>,
            reducer: CaseReducer<State, A>,
          ): Omit<ActionReducerMapBuilder<State>, 'addCase'>
          addDefaultCase(reducer: CaseReducer<State, AnyAction>): {}
        }
    "#;

    #[test]
    fn allows_interface_in_library_package() {
        let pkg = r#"{ "name": "@reduxjs/toolkit", "exports": { ".": "./dist/index.js" } }"#;
        assert!(
            run_with_pkg(pkg, ACTION_REDUCER_MAP_BUILDER).is_empty(),
            "interfaces in library packages support declaration merging"
        );
    }

    #[test]
    fn flags_interface_in_non_library_package() {
        let pkg = r#"{ "name": "my-app", "private": true }"#;
        assert_eq!(
            run_with_pkg(pkg, ACTION_REDUCER_MAP_BUILDER).len(),
            1,
            "application code still gets flagged"
        );
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
    fn allows_interface_in_declare_global() {
        // https://github.com/rbaumier/comply/issues/1596
        // `declare global { interface HTMLElementTagNameMap { ... } }` is the
        // canonical Lit/Web-Components pattern: augmenting a global interface
        // via declaration merging, which only `interface` (not `type`) allows.
        let code = r#"
            export class EmergencyContactFields extends LitElement {}
            declare global {
              interface HTMLElementTagNameMap {
                'emergency-contact-fields': EmergencyContactFields
              }
            }
        "#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_top_level_interface_outside_declare_global() {
        // Negative space: a plain top-level interface that is not inside any
        // ambient augmentation block is still flagged.
        let code = r#"
            export class EmergencyContactFields extends LitElement {}
            interface HTMLElementTagNameMap {
              'emergency-contact-fields': EmergencyContactFields
            }
        "#;
        assert_eq!(run_on(code).len(), 1);
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
    fn allows_interface_extended_by_other_interface() {
        // https://github.com/rbaumier/comply/issues/1168
        let code = r#"
            export interface Resource {
              id?: string;
              name?: string;
            }
            export interface TrackedResource extends Resource {
              location: string;
            }
            export interface ProxyResource extends Resource {}
        "#;
        assert!(
            run_on(code).is_empty(),
            "an interface used as an extends base by other interfaces enables \
             extension and declaration merging — keep it as `interface`"
        );
    }

    #[test]
    fn flags_standalone_interface_not_extended_by_anyone() {
        // A plain interface with no extends clause that no other interface
        // extends and no class implements is still flagged.
        let code = r#"
            export interface Resource {
              id?: string;
            }
            export interface Other {
              name: string;
            }
        "#;
        assert_eq!(run_on(code).len(), 2);
    }

    #[test]
    fn allows_dts_file() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "interface ImportMetaEnv { readonly VITE_API: string; }", "env.d.ts");
        assert!(diags.is_empty());
    }
}
