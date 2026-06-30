use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ClassBody, ClassElement, Declaration, Function, MethodDefinition, Program, PropertyKey,
    Statement, TSType,
};
use std::sync::Arc;

pub struct Check;

/// The static key name of a method (`Identifier` or string-literal key), or
/// `None` for computed, private, or other keys.
fn method_key_name<'a>(method: &'a MethodDefinition<'a>) -> Option<&'a str> {
    match &method.key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// The body-less function declaration carried by a statement (directly or via an
/// `export`) — a TypeScript overload signature. `None` for any statement that is
/// not a body-less function declaration.
fn statement_signature<'a>(stmt: &'a Statement<'a>) -> Option<&'a Function<'a>> {
    let f = match stmt {
        Statement::FunctionDeclaration(f) => f,
        Statement::ExportNamedDeclaration(exp) => match exp.declaration.as_ref()? {
            Declaration::FunctionDeclaration(f) => f,
            _ => return None,
        },
        _ => return None,
    };
    f.body.is_none().then_some(f.as_ref())
}

/// True when `program` declares a body-less function named `name` before byte
/// offset `impl_start` — an overload signature for the implementation there.
fn program_has_preceding_signature(program: &Program, name: &str, impl_start: u32) -> bool {
    program.body.iter().any(|stmt| {
        statement_signature(stmt).is_some_and(|f| {
            f.span.start < impl_start && f.id.as_ref().is_some_and(|id| id.name == name)
        })
    })
}

/// True when `class_body` declares a body-less method named `name` before byte
/// offset `impl_start` — a class-method overload signature.
fn class_body_has_preceding_signature(
    class_body: &ClassBody,
    name: &str,
    impl_start: u32,
) -> bool {
    class_body.body.iter().any(|element| match element {
        ClassElement::MethodDefinition(method) => {
            method.value.body.is_none()
                && method.value.span.start < impl_start
                && method_key_name(method) == Some(name)
        }
        _ => false,
    })
}

/// True when `func` (the node `node`) is the implementation body of a TypeScript
/// overload set: it has a body and is preceded, at the same scope (module body
/// or class body), by at least one body-less declaration of the same name (an
/// overload signature). TypeScript checks the implementation against the typed
/// overload signatures rather than against its own return annotation, so an
/// idiomatic `: any` return there is not a type-safety defect.
fn is_overload_implementation<'a>(
    func: &Function<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // Only a function WITH a body can be an implementation; signatures are
    // body-less.
    if func.body.is_none() {
        return false;
    }
    let nodes = semantic.nodes();
    let parent = nodes.parent_node(node.id());

    // Class-method implementation: the name lives on the enclosing
    // `MethodDefinition` key and the sibling signatures sit in the class body.
    if let AstKind::MethodDefinition(method) = parent.kind() {
        let Some(name) = method_key_name(method) else {
            return false;
        };
        let AstKind::ClassBody(class_body) = nodes.parent_node(parent.id()).kind() else {
            return false;
        };
        return class_body_has_preceding_signature(class_body, name, func.span.start);
    }

    // Module-level function declaration (optionally wrapped in an `export`).
    let Some(name) = func.id.as_ref().map(|id| id.name.as_str()) else {
        return false;
    };
    let container = match parent.kind() {
        AstKind::Program(_) => parent,
        AstKind::ExportNamedDeclaration(_) => nodes.parent_node(parent.id()),
        _ => return false,
    };
    let AstKind::Program(program) = container.kind() else {
        return false;
    };
    program_has_preceding_signature(program, name, func.span.start)
}

/// True if the type resolves to `any` or `Promise<any>`.
fn resolves_to_any(ty: &TSType) -> bool {
    match ty {
        TSType::TSAnyKeyword(_) => true,
        TSType::TSTypeReference(type_ref) => {
            let name = match &type_ref.type_name {
                oxc_ast::ast::TSTypeName::IdentifierReference(id) => id.name.as_str(),
                _ => return false,
            };
            if name != "Promise" {
                return false;
            }
            let Some(params) = &type_ref.type_arguments else {
                return false;
            };
            params.params.iter().any(|p| resolves_to_any(p))
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Machine-generated files (e.g. AutoRest Azure SDK serializers) carry a
        // `do not edit` / `code generated` header and aren't hand-maintained;
        // their `: any` return types are noise, not a type-safety defect.
        if ctx.file.is_generated {
            return;
        }
        let return_type = match node.kind() {
            AstKind::Function(func) => func.return_type.as_ref(),
            AstKind::ArrowFunctionExpression(arrow) => arrow.return_type.as_ref(),
            _ => return,
        };
        let Some(type_ann) = return_type else { return };
        if !resolves_to_any(&type_ann.type_annotation) {
            return;
        }
        // The implementation body of an overloaded function idiomatically returns
        // `: any`: TypeScript validates it against the typed overload signatures,
        // not against this annotation, so it is not a defect.
        if let AstKind::Function(func) = node.kind()
            && is_overload_implementation(func, node, semantic)
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, type_ann.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Function has explicit `: any` return type — use a specific type or `unknown`."
                .into(),
            severity: super::META.severity,
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

    #[test]
    fn flags_any_return_function() {
        assert_eq!(run_on("function foo(): any {}").len(), 1);
    }

    #[test]
    fn flags_any_return_arrow() {
        assert_eq!(run_on("const foo = (): any => {};").len(), 1);
    }

    #[test]
    fn flags_promise_any_return() {
        assert_eq!(run_on("async function foo(): Promise<any> {}").len(), 1);
    }

    #[test]
    fn allows_specific_return_type() {
        assert!(run_on("function foo(): string {}").is_empty());
    }

    #[test]
    fn allows_unknown_return() {
        assert!(run_on("function foo(): unknown {}").is_empty());
    }

    #[test]
    fn allows_any_return_on_overload_implementation() {
        // Issue #6804 (privatenumber/cleye `cli()`): a group of body-less
        // overload signatures followed by the implementation, whose `: any`
        // return satisfies the typed signatures — not a defect.
        let src = "\
function cli(options: Options, callback?: undefined): ParsedResult;
function cli(options: Options, callback: Cb): MaybePromise;
function cli(options: Options, callback?: Cb, argv = []): any {
    return run(options, callback, argv);
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_any_return_on_exported_overload_implementation() {
        // cleye declares `cli` with `export function` on every signature and the
        // implementation; the export wrapper must not defeat the exemption.
        let src = "\
export function cli(options: Options, callback?: undefined): ParsedResult;
export function cli(options: Options, callback: Cb): MaybePromise;
export function cli(options: Options, callback?: Cb, argv = []): any {
    return run(options, callback, argv);
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_any_return_on_class_method_overload_implementation() {
        // Class-method overloads: typed signatures then an `: any` implementation.
        let src = "\
class Foo {
    bar(x: string): string;
    bar(x: number): number;
    bar(x: unknown): any { return x; }
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_any_return_without_overload_signature() {
        // A function with a body but no preceding same-name signature is a plain
        // `: any` return, not an overload implementation.
        assert_eq!(run_on("function foo(): any { return x; }").len(), 1);
    }

    #[test]
    fn still_flags_class_method_any_return_without_overloads() {
        // A class method with no preceding same-name signature still fires.
        assert_eq!(run_on("class Foo { bar(): any { return x; } }").len(), 1);
    }

    #[test]
    fn unrelated_signature_does_not_exempt() {
        // A body-less declaration of a DIFFERENT name is not an overload
        // signature for `foo`, so `foo`'s `: any` return still fires.
        let src = "\
function bar(x: string): string;
function foo(): any { return x; }";
        assert_eq!(run_on(src).len(), 1);
    }

    /// Run with a `FileCtx` built from the source so the generated-header
    /// marker sets `is_generated` (the default helper uses an empty FileCtx).
    fn run_with_file_ctx(source: &str, path: &str) -> Vec<Diagnostic> {
        use crate::files::Language;
        use crate::rules::file_ctx::FileCtx;
        let path = std::path::Path::new(path);
        let lang = Language::from_path(path).unwrap_or(Language::TypeScript);
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(path, source, lang, project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &file)
    }

    #[test]
    fn ignores_any_return_in_generated_file() {
        // AutoRest-generated Azure SDK serializers carry a `do not edit` header,
        // so `ctx.file.is_generated` is true and the `: any` return is noise.
        let source = "// Code generated by AutoRest.\n// DO NOT EDIT.\nexport function fooSerializer(x: Foo): any { return {}; }";
        assert!(run_with_file_ctx(source, "models/models.ts").is_empty());
    }

    #[test]
    fn still_flags_any_return_in_non_generated_file() {
        // No generated header — the guard must not relax the rule here.
        let source = "export function foo(): any { return x; }";
        assert_eq!(run_with_file_ctx(source, "src/foo.ts").len(), 1);
    }
}
