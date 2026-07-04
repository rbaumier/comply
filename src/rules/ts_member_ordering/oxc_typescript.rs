//! OXC backend for ts-member-ordering — enforce canonical member order in
//! classes and interfaces: signatures, fields, constructors, methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentTarget, Class, ClassElement, Expression, MethodDefinitionKind, Statement,
    TSSignature,
};
use rustc_hash::FxHashSet;
use std::sync::Arc;

fn class_element_rank(elem: &ClassElement) -> Option<u8> {
    match elem {
        ClassElement::TSIndexSignature(_) => Some(0),
        ClassElement::PropertyDefinition(prop) => {
            if prop.r#type == oxc_ast::ast::PropertyDefinitionType::TSAbstractPropertyDefinition {
                Some(1)
            } else {
                Some(1)
            }
        }
        ClassElement::MethodDefinition(method) => match method.kind {
            MethodDefinitionKind::Constructor => Some(2),
            // A `get x()` / `set x()` accessor is property-like, ranked the same
            // as the `accessor x` field syntax below so an accessor grouped
            // among fields does not poison the field section's running rank.
            MethodDefinitionKind::Get | MethodDefinitionKind::Set => Some(1),
            MethodDefinitionKind::Method => Some(3),
        },
        ClassElement::AccessorProperty(_) => Some(1),
        ClassElement::StaticBlock(_) => None,
    }
}

/// Property names assigned via `this.<name> = ...` directly in the constructor
/// body. Such fields are legitimately declared after methods: the constructor
/// definitely assigns them, so their declaration position is purely stylistic
/// (e.g. AutoRest-generated SDK clients group sub-client fields at the end).
fn constructor_assigned_fields<'a>(class: &Class<'a>) -> FxHashSet<&'a str> {
    let mut names = FxHashSet::default();
    for elem in &class.body.body {
        let ClassElement::MethodDefinition(method) = elem else { continue };
        if method.kind != MethodDefinitionKind::Constructor || method.r#static {
            continue;
        }
        let Some(body) = method.value.body.as_ref() else { continue };
        for stmt in &body.statements {
            let Statement::ExpressionStatement(expr_stmt) = stmt else { continue };
            let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
                continue;
            };
            let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
                continue;
            };
            if !matches!(&member.object, Expression::ThisExpression(_)) {
                continue;
            }
            names.insert(member.property.name.as_str());
        }
    }
    names
}

/// A property whose initializer is an arrow or function expression is a
/// method-like member (e.g. `handleClick = () => { ... }`): the function form
/// is chosen to capture `this`, so grouping such fields after the regular
/// methods is a deliberate auto-binding convention, not an ordering smell.
fn is_method_like_field(prop: &oxc_ast::ast::PropertyDefinition) -> bool {
    matches!(
        prop.value,
        Some(Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_))
    )
}

fn ts_signature_rank(sig: &TSSignature) -> Option<u8> {
    match sig {
        // A catch-all index signature (`[key: string]: T`) has no canonical
        // position: leading and trailing placement are both idiomatic, so it is
        // excluded from ordering and never updates or violates the running rank.
        TSSignature::TSIndexSignature(_) => None,
        TSSignature::TSCallSignatureDeclaration(_)
        | TSSignature::TSConstructSignatureDeclaration(_) => Some(0),
        TSSignature::TSPropertySignature(_) => Some(1),
        TSSignature::TSMethodSignature(_) => Some(3),
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class, AstType::TSInterfaceDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Class(class) => {
                let assigned = constructor_assigned_fields(class);
                let mut max_rank: u8 = 0;
                for elem in &class.body.body {
                    let Some(rank) = class_element_rank(elem) else { continue };
                    if rank < max_rank {
                        if let ClassElement::PropertyDefinition(prop) = elem
                            && (is_method_like_field(prop)
                                || prop
                                    .key
                                    .name()
                                    .is_some_and(|name| assigned.contains(name.as_ref())))
                        {
                            continue;
                        }
                        let span = match elem {
                            ClassElement::MethodDefinition(m) => m.span,
                            ClassElement::PropertyDefinition(p) => p.span,
                            ClassElement::AccessorProperty(a) => a.span,
                            ClassElement::TSIndexSignature(s) => s.span,
                            ClassElement::StaticBlock(s) => s.span,
                        };
                        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Member is out of order — expected: signatures, \
                                      fields, constructors, methods."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    } else {
                        max_rank = rank;
                    }
                }
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                let mut max_rank: u8 = 0;
                for sig in &iface.body.body {
                    let Some(rank) = ts_signature_rank(sig) else { continue };
                    if rank < max_rank {
                        let span = match sig {
                            TSSignature::TSIndexSignature(s) => s.span,
                            TSSignature::TSCallSignatureDeclaration(s) => s.span,
                            TSSignature::TSConstructSignatureDeclaration(s) => s.span,
                            TSSignature::TSPropertySignature(s) => s.span,
                            TSSignature::TSMethodSignature(s) => s.span,
                        };
                        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Member is out of order — expected: signatures, \
                                      fields, constructors, methods."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    } else {
                        max_rank = rank;
                    }
                }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_field_after_method() {
        let diags = run("class Foo {\n  bar(): void {}\n  x: string;\n}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_constructor_assigned_fields_after_methods() {
        // AutoRest-generated ARM SDK pattern: sub-client fields are declared
        // after methods but assigned in the constructor body.
        let src = "export class ManagementGroupsAPI {\n\
            \x20 $host: string;\n\
            \x20 apiVersion: string;\n\
            \x20 constructor() {\n\
            \x20   this.managementGroups = new ManagementGroupsImpl(this);\n\
            \x20   this.entities = new EntitiesImpl(this);\n\
            \x20 }\n\
            \x20 checkNameAvailability(): Promise<void> { return Promise.resolve(); }\n\
            \x20 startTenantBackfill(): Promise<void> { return Promise.resolve(); }\n\
            \x20 managementGroups: ManagementGroups;\n\
            \x20 entities: Entities;\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_unassigned_field_after_method() {
        // A field declared after methods that is NOT assigned in the
        // constructor is a genuine ordering smell and must still fire.
        let src = "class Foo {\n\
            \x20 constructor() {\n\
            \x20   this.assigned = 1;\n\
            \x20 }\n\
            \x20 bar(): void {}\n\
            \x20 assigned: number;\n\
            \x20 stray: string;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_arrow_function_fields_after_methods() {
        // framer/motion MotionValue pattern: arrow-function fields are
        // `this`-bound callbacks passed to external APIs, so they are
        // intentionally grouped after the regular methods.
        let src = "class MotionValue<V = any> {\n\
            \x20 current!: V;\n\
            \x20 prev: V | undefined;\n\
            \x20 constructor(initX: V) { this.current = initX; }\n\
            \x20 addDependent(): void {}\n\
            \x20 updateAndNotify = (v: V) => { this.current = v; };\n\
            \x20 get() {}\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_function_expression_fields_after_methods() {
        // A `function` expression assigned to a field is method-like too.
        let src = "class Foo {\n\
            \x20 bar(): void {}\n\
            \x20 handler = function () {};\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_data_field_after_method() {
        // An ordinary data field (non-arrow, non-constructor-assigned)
        // placed after methods remains a genuine ordering smell.
        let src = "class Foo {\n\
            \x20 bar(): void {}\n\
            \x20 hasAnimated = false;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_fields_and_constructor_after_getter_accessor() {
        // typeorm/typeorm EntityManager pattern: a getter accessor (a deprecated
        // property alias) is grouped among the fields. It is property-like, so
        // the fields and constructor that follow it stay in order.
        let src = "class EntityManager {\n\
            \x20 readonly dataSource: DataSource;\n\
            \x20 get connection(): DataSource { return this.dataSource; }\n\
            \x20 readonly queryRunner?: QueryRunner;\n\
            \x20 protected repositories = new Map();\n\
            \x20 constructor(dataSource: DataSource) {}\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_field_after_method_alongside_getter() {
        // A real data field placed after a real method remains a genuine
        // ordering smell even when a getter accessor sits among the fields: the
        // accessor (rank 1) must not suppress the downstream violation.
        let src = "class Foo {\n\
            \x20 a: A;\n\
            \x20 get g(): number { return 1; }\n\
            \x20 foo(): void {}\n\
            \x20 b: B;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_field_before_constructor_before_method() {
        // The canonical order (field, constructor, method) is unaffected.
        let src = "class Foo {\n\
            \x20 a: A;\n\
            \x20 constructor() {}\n\
            \x20 foo(): void {}\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_accessor_field_among_fields() {
        // The `accessor x` field syntax is property-like (rank 1) and does not
        // poison the fields that follow it — consistent with `get x()`.
        let src = "class Foo {\n\
            \x20 a: A;\n\
            \x20 accessor b: B;\n\
            \x20 c: C;\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_trailing_index_signature_in_interface() {
        // nitrojs/nitro pattern: a catch-all index signature placed last, after
        // the explicitly typed members, is the canonical "open interface" idiom.
        let src = "interface CFPagesEnv {\n\
            \x20 CF_PAGES: \"1\";\n\
            \x20 CF_PAGES_URL: string;\n\
            \x20 [key: string]: any;\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_leading_index_signature_in_interface() {
        // A leading index signature is equally idiomatic and must not flag the
        // properties that follow it.
        let src = "interface Bag {\n\
            \x20 [key: string]: unknown;\n\
            \x20 id: string;\n\
            \x20 name: string;\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_call_signature_after_property_in_interface() {
        // A genuine ordering smell among non-index members still fires: a call
        // signature (rank 0) belongs before properties (rank 1).
        let src = "interface Foo {\n\
            \x20 value: number;\n\
            \x20 (): void;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_method_before_property_in_interface() {
        // A method (rank 3) declared before a property (rank 1) is a genuine
        // misorder and must still fire.
        let src = "interface Foo {\n\
            \x20 bar(): void;\n\
            \x20 value: number;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
