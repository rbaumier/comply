//! no-property-mutation OXC backend — flag property mutations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, is_local_object_builder_binding, is_react_display_name_assignment,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const SENTRY_HOOKS: &[&str] = &["beforeSend", "beforeBreadcrumb", "beforeSendTransaction"];

/// Static name of an object-property key, if it's an identifier or string literal.
fn static_key_name<'a>(key: &PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Name of the nearest enclosing named function (declaration or named expression).
fn nearest_enclosing_fn_name<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::Function(func) = ancestor.kind()
            && let Some(id) = &func.id
        {
            return Some(id.name.as_str());
        }
    }
    None
}

/// True when the mutation sits inside a Sentry hook callback — either an inline
/// lambda/method assigned to `beforeSend`/`beforeBreadcrumb`/`beforeSendTransaction`,
/// or a named function registered as one of those hooks somewhere in the file.
/// Sentry's hooks are designed around in-place mutation and offer no immutable API.
fn is_inside_sentry_hook<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // Inline callback: an ancestor object property keyed by a Sentry hook.
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::ObjectProperty(prop) = ancestor.kind()
            && static_key_name(&prop.key).is_some_and(|name| SENTRY_HOOKS.contains(&name))
        {
            return true;
        }
    }

    // Named function registered by reference: `beforeSend: scrubEventRequestUrl`.
    let Some(fn_name) = nearest_enclosing_fn_name(node, semantic) else {
        return false;
    };
    for n in semantic.nodes().iter() {
        if let AstKind::ObjectProperty(prop) = n.kind()
            && static_key_name(&prop.key).is_some_and(|name| SENTRY_HOOKS.contains(&name))
            && let Expression::Identifier(id) = &prop.value
            && id.name.as_str() == fn_name
        {
            return true;
        }
    }
    false
}

/// Get the root object identifier name from an expression chain.
fn root_object_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => root_object_name(&m.object),
        Expression::ComputedMemberExpression(m) => root_object_name(&m.object),
        _ => None,
    }
}

/// Get the root `IdentifierReference` from a member-access chain. Used to resolve
/// the binding via semantic and inspect its declaration.
fn root_identifier_of_expr<'a>(expr: &'a Expression<'a>) -> Option<&'a IdentifierReference<'a>> {
    match expr {
        Expression::Identifier(id) => Some(id),
        Expression::StaticMemberExpression(m) => root_identifier_of_expr(&m.object),
        Expression::ComputedMemberExpression(m) => root_identifier_of_expr(&m.object),
        _ => None,
    }
}

/// True when `ident` resolves to a binding initialised via `document.createElement(...)`
/// or `document.createElementNS(...)`. A freshly created DOM element is unattached and
/// must be configured by property assignment before insertion — not a state mutation.
fn is_created_dom_element(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            let Some(init) = &decl.init else { return false };
            return is_create_element_call(init);
        }
    }
    false
}

const DOM_WRITE_INTERMEDIARIES: &[&str] = &["style", "dataset"];

/// True when the assignment target chain passes through a DOM write property
/// such as `el.style.width = v` or `el.dataset.key = v`. Mutating `.style`/
/// `.dataset` sub-properties is the canonical imperative DOM API with no
/// immutable alternative.
fn has_dom_write_intermediary(expr: &Expression) -> bool {
    match expr {
        Expression::StaticMemberExpression(m) => {
            if DOM_WRITE_INTERMEDIARIES.contains(&m.property.name.as_str()) {
                return true;
            }
            has_dom_write_intermediary(&m.object)
        }
        _ => false,
    }
}

fn is_create_element_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    let Expression::Identifier(obj) = &member.object else { return false };
    if obj.name.as_str() != "document" { return false }
    let method = member.property.name.as_str();
    method == "createElement" || method == "createElementNS"
}

/// True when `node` sits inside a `constructor()` body. Assigning `this.x = value`
/// while the object is being constructed is initialisation, not mutation of an
/// already-stable object — TypeScript even allows setting `readonly` fields here.
fn is_inside_constructor<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut ancestors = semantic.nodes().ancestors(node.id()).peekable();
    let mut first = true;
    while let Some(ancestor) = ancestors.next() {
        if first {
            first = false;
            continue;
        }
        match ancestor.kind() {
            AstKind::MethodDefinition(method) => {
                return method.kind == MethodDefinitionKind::Constructor;
            }
            AstKind::Function(_) => {
                // The constructor body is wrapped in a Function node in OXC's AST.
                if let Some(next) = ancestors.peek()
                    && let AstKind::MethodDefinition(method) = next.kind()
                {
                    return method.kind == MethodDefinitionKind::Constructor;
                }
                return false;
            }
            AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::AssignmentExpression,
            AstType::UpdateExpression,
            AstType::UnaryExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Test files mutate local fixtures, accumulators, and mock-captured
        // state freely — bounded to the test scope with no non-mutating
        // alternative. Consistent with no-mutation / no-mutating-assign.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        match node.kind() {
            AstKind::AssignmentExpression(assign) => {
                // Component.displayName = "Component" (React naming convention)
                if is_react_display_name_assignment(assign) {
                    return;
                }
                match &assign.left {
                    AssignmentTarget::StaticMemberExpression(m) => {
                        let obj_text = &ctx.source
                            [m.object.span().start as usize..m.object.span().end as usize];
                        let prop_text = m.property.name.as_str();

                        if obj_text == "module" || obj_text == "exports" { return; }
                        if prop_text == "current" { return; }
                        if obj_text == "document" && prop_text == "cookie" { return; }
                        if matches!(&m.object, Expression::ThisExpression(_))
                            && is_inside_constructor(node, semantic) { return; }
                        if is_inside_sentry_hook(node, semantic) { return; }
                        if root_object_name(&m.object) == Some("set") { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && (is_created_dom_element(id, semantic)
                                || is_local_object_builder_binding(id, semantic)) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }

                        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation — use spread or immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    AssignmentTarget::ComputedMemberExpression(m) => {
                        let obj_text = &ctx.source
                            [m.object.span().start as usize..m.object.span().end as usize];

                        if obj_text == "module" || obj_text == "exports" { return; }
                        if matches!(&m.object, Expression::ThisExpression(_))
                            && is_inside_constructor(node, semantic) { return; }
                        if is_inside_sentry_hook(node, semantic) { return; }
                        if root_object_name(&m.object) == Some("set") { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && (is_created_dom_element(id, semantic)
                                || is_local_object_builder_binding(id, semantic)) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }

                        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation — use spread or immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    _ => {}
                }
            }
            AstKind::UpdateExpression(update) => {
                // update.argument is a SimpleAssignmentTarget.
                // Check if it's a member expression.
                match &update.argument {
                    SimpleAssignmentTarget::StaticMemberExpression(m) => {
                        if is_inside_sentry_hook(node, semantic) { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && is_created_dom_element(id, semantic) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }
                        let (line, column) = byte_offset_to_line_col(ctx.source, update.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation (increment/decrement) — use immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    SimpleAssignmentTarget::ComputedMemberExpression(m) => {
                        if is_inside_sentry_hook(node, semantic) { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && is_created_dom_element(id, semantic) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }
                        let (line, column) = byte_offset_to_line_col(ctx.source, update.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "no-property-mutation".into(),
                            message: "Property mutation (increment/decrement) — use immutable patterns.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    _ => {}
                }
            }
            AstKind::UnaryExpression(unary) => {
                if unary.operator != UnaryOperator::Delete {
                    return;
                }
                match &unary.argument {
                    Expression::StaticMemberExpression(m) => {
                        if is_inside_sentry_hook(node, semantic) { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && is_created_dom_element(id, semantic) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }
                    }
                    Expression::ComputedMemberExpression(m) => {
                        if is_inside_sentry_hook(node, semantic) { return; }
                        if let Some(id) = root_identifier_of_expr(&m.object)
                            && is_created_dom_element(id, semantic) { return; }
                        if has_dom_write_intermediary(&m.object) { return; }
                    }
                    _ => return,
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-property-mutation".into(),
                    message: "Property deletion — use destructuring or immutable patterns.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn skips_in_test_file_issue_582() {
        // Tests mutate local fixtures and mock-captured state freely; bounded
        // to the test scope with no non-mutating alternative.
        let src = r#"
            beforeEach(() => {
                config.retries = 3;
                state["count"] = 0;
            });
        "#;
        assert!(run_in_test_file(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_local_object_spread_builder() {
        // Regression for rbaumier/comply#1930 — dnd-kit boundingRectangle:
        // `value` is a fresh local copy via object spread, built up via
        // conditional property assignments before being returned.
        let src = r#"
            export function boundingRectangle(transform, shape, boundingRect) {
                const value = { ...transform };
                if (cond) {
                    value.y = boundingRect.top - shape.boundingRectangle.top;
                } else if (cond2) {
                    value.y = boundingRect.bottom;
                }
                if (cond3) {
                    value.x = boundingRect.left;
                }
                return value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_local_object_literal_builder() {
        let src = r#"
            function build() {
                const value = { a: 1 };
                value.b = 2;
                return value;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_property_assignment_on_function_parameter() {
        // A function parameter is external state, not a local object builder.
        let src = r#"
            function mutate(value) {
                value.x = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_property_assignment_on_const_from_external_call() {
        // A `const` initialized from a function call (not an object literal /
        // spread) references external state — mutating it is still flagged.
        let src = r#"
            function mutate() {
                const value = getConfig();
                value.x = 1;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_property_assignment_on_created_dom_element() {
        let src = r#"
            function download(objectUrl: string, filename: string) {
                const anchor = document.createElement("a");
                anchor.href = objectUrl;
                anchor.download = filename;
                anchor.rel = "noopener";
                document.body.append(anchor);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_property_assignment_on_created_svg_element() {
        let src = r#"
            function build() {
                const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
                svg.id = "chart";
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_this_assignment_in_constructor() {
        // Regression for issue #477: `this.x = value` in a constructor body is
        // field initialisation (including `readonly` fields), not mutation.
        let src = r#"
            class ProblemError extends Error {
                readonly problem: Problem;
                constructor(problem: Problem) {
                    super();
                    this.name = 'ProblemError';
                    this.problem = problem;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_this_assignment_in_method() {
        let src = r#"
            class Foo {
                update() { this.value = 1; }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_mutation_on_unrelated_const() {
        let src = r#"
            function set(objectUrl: string) {
                const anchor = getAnchorFromDom();
                anchor.href = objectUrl;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Sentry beforeSend/beforeBreadcrumb in-place scrub hooks — issue #478

    #[test]
    fn allows_mutation_inside_inline_before_send_arrow() {
        let src = r#"
            Sentry.init({
                beforeSend: (event) => {
                    event.request.url = scrubSensitiveQueryFromUrl(url);
                    return event;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_inside_inline_before_breadcrumb_method() {
        let src = r#"
            Sentry.init({
                beforeBreadcrumb(breadcrumb) {
                    breadcrumb.data = sanitize(breadcrumb.data);
                    return breadcrumb;
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_in_named_function_registered_as_before_send() {
        let src = r#"
            function scrubEventRequestUrl(event) {
                event.request.url = scrubSensitiveQueryFromUrl(event.request.url);
                return event;
            }
            Sentry.init({ beforeSend: scrubEventRequestUrl });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_subscript_mutation_in_named_function_registered_as_before_breadcrumb() {
        let src = r#"
            function scrubStringField(bag, key) {
                bag[key] = scrubSensitiveQueryFromUrl(bag[key]);
            }
            Sentry.init({ beforeBreadcrumb: scrubStringField });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_mutation_outside_sentry_hook() {
        let src = r#"
            function scrubStringField(bag, key) {
                bag[key] = scrubSensitiveQueryFromUrl(bag[key]);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // DOM .style / .dataset chains — issue #750

    #[test]
    fn skips_dom_style_chain_issue_750() {
        // Mutating `.style` sub-properties is the canonical imperative DOM API;
        // no spread/immutable equivalent exists.
        let src = r#"
            function applyStyle(el: HTMLElement, width: number): void {
                el.style.width = `${width}px`;
                elements.floating.style.maxHeight = `${availableHeight}px`;
                el.dataset.key = "value";
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_direct_style_assignment() {
        // Assigning directly to `.style` (replacing the whole object) is a
        // genuine mutation — only sub-property writes via `.style.X` are exempt.
        let src = r#"
            function reset(el: HTMLElement): void {
                el.style = someObj;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // React displayName naming convention — issue #1779

    #[test]
    fn allows_display_name_assignment_on_forward_ref_component() {
        // Regression for rbaumier/comply#1779 — setting `displayName` on a
        // forwardRef-wrapped component is the standard React naming convention.
        let src = r#"
            const RadioGroup = React.forwardRef((props, ref) => {
                return <RadioGroupPrimitives.Root ref={ref} {...props} />;
            });
            RadioGroup.displayName = "RadioGroup";
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }

    #[test]
    fn still_flags_non_string_display_name_assignment() {
        // Only string-literal `displayName` writes are exempt; assigning a
        // computed value is still a property mutation.
        let src = r#"
            RadioGroup.displayName = getName();
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_other_string_property_assignment() {
        let src = r#"
            RadioGroup.label = "RadioGroup";
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
