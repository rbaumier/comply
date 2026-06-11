use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let nodes = semantic.nodes();

        for node in nodes.iter() {
            let method_def = match node.kind() {
                AstKind::MethodDefinition(m) => m,
                _ => continue,
            };

            // Skip static methods
            if method_def.r#static {
                continue;
            }

            // Skip constructors
            if method_def.kind == oxc_ast::ast::MethodDefinitionKind::Constructor {
                continue;
            }

            // Skip abstract methods (no body)
            if method_def.value.body.is_none() {
                continue;
            }

            // Skip decorated methods
            if !method_def.decorators.is_empty() {
                continue;
            }

            // Skip methods whose enclosing class is decorated, extends a base
            // class, or implements an interface. With `extends`/`implements`,
            // the method may be required by the base-class or interface
            // contract (e.g. NestJS DI factories, overrides), so making it
            // `static` or extracting it would break polymorphism.
            if let Some(class) = enclosing_class(node.id(), nodes)
                && (!class.decorators.is_empty()
                    || class.super_class.is_some()
                    || !class.implements.is_empty())
            {
                continue;
            }

            // Check if body contains `this`
            if body_contains_this(method_def.span.start, nodes) {
                continue;
            }

            let name = match &method_def.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(ident) => ident.name.as_str(),
                _ => "<computed>",
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, method_def.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Method `{name}` does not use `this` — make it `static` \
                     or extract to a standalone function."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

/// Check if any descendant of the method body references `this`, stopping at
/// nested function/class boundaries.
fn body_contains_this(
    method_span_start: u32,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    for child in nodes.iter() {
        if !matches!(child.kind(), AstKind::ThisExpression(_)) {
            continue;
        }
        // Walk up from this `this` expression to see if it belongs to our method.
        // The hierarchy is: MethodDefinition -> Function -> FunctionBody -> ...
        // The method's own Function is the one that binds `this` for the method,
        // so we allow it. We stop at OTHER Function/Class nodes.
        let mut current = child.id();
        let mut found_method = false;
        loop {
            let parent_id = nodes.parent_id(current);
            if parent_id == current {
                break;
            }
            let parent = nodes.get_node(parent_id);
            match parent.kind() {
                AstKind::MethodDefinition(m) if m.span.start == method_span_start => {
                    found_method = true;
                    break;
                }
                // Arrow functions don't rebind `this` — continue upward
                AstKind::ArrowFunctionExpression(_) => {}
                // The method's own Function node is the direct child of MethodDefinition.
                // Check if the grandparent is our MethodDefinition.
                AstKind::Function(_) => {
                    let gp_id = nodes.parent_id(parent_id);
                    if gp_id != parent_id {
                        let gp = nodes.get_node(gp_id);
                        if let AstKind::MethodDefinition(m) = gp.kind()
                            && m.span.start == method_span_start {
                                // This is the method's own function — allow
                                current = parent_id;
                                continue;
                            }
                    }
                    // Different function — rebinds `this`
                    break;
                }
                AstKind::Class(_) => break,
                _ => {}
            }
            current = parent_id;
        }
        if found_method {
            return true;
        }
    }
    false
}

/// Walk up from a method node to its enclosing `Class`.
fn enclosing_class<'a>(
    method_node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes<'a>,
) -> Option<&'a oxc_ast::ast::Class<'a>> {
    let mut current = method_node_id;
    loop {
        let parent_id = nodes.parent_id(current);
        if parent_id == current {
            return None;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::Class(class) = parent.kind() {
            return Some(class);
        }
        current = parent_id;
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
    fn flags_method_without_this() {
        let diags = run_on("class Foo { bar() { return 1; } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn allows_method_with_this() {
        assert!(run_on("class Foo { bar() { return this.x; } }").is_empty());
    }

    #[test]
    fn allows_static_method() {
        assert!(run_on("class Foo { static bar() { return 1; } }").is_empty());
    }

    #[test]
    fn allows_constructor() {
        assert!(run_on("class Foo { constructor() { const x = 1; } }").is_empty());
    }

    #[test]
    fn allows_decorated_method_without_this() {
        let src = "class Foo { @Get() bar() { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_methods_in_decorated_class_without_this() {
        let src = "@Controller()\nclass Foo { bar() { return 1; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_method_in_class_implementing_interface() {
        // Issue #972: NestJS factory pattern — `createGqlOptions` is required
        // by the `GqlOptionsFactory` interface and cannot be made static.
        let src = "class ConfigService implements GqlOptionsFactory {\n\
                   createGqlOptions() { return { typePaths: [] }; }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_method_in_class_extending_base_class() {
        // Issue #972: `serializeError` overrides a method of the parent class.
        let src = "class ErrorHandlingProxy extends ClientGrpcProxy {\n\
                   serializeError(err) { return new RpcException(err); }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_override_method_in_extends_class() {
        let src = "class Foo extends Bar { override baz() { return 1; } }";
        assert!(run_on(src).is_empty());
    }
}
