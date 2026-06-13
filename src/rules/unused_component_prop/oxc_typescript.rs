//! unused-component-prop OXC backend.
//!
//! Uses `run_on_semantic` to detect React component props that are declared
//! in the Props type but never read in the component body.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, PropertyKey, TSSignature, TSType, TSTypeName};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};

pub struct Check;

fn is_type_test_file(path: &std::path::Path, source: &str) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.contains("types.test")
        || name.contains(".type-test")
        || name.ends_with(".d.ts")
        || name.ends_with(".d.tsx")
        || crate::oxc_helpers::source_contains(source, "@vitest-environment")
        || crate::oxc_helpers::source_contains(source, "@ts-check")
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if is_type_test_file(ctx.path, ctx.source) {
            return Vec::new();
        }

        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        // Pass 1: collect interface/type property names
        let mut prop_types: HashMap<String, Vec<PropInfo>> = HashMap::new();
        for node in nodes.iter() {
            match node.kind() {
                AstKind::TSInterfaceDeclaration(decl) => {
                    let name = decl.id.name.to_string();
                    let props = collect_ts_signature_props(&decl.body.body);
                    if !props.is_empty() {
                        prop_types.insert(name, props);
                    }
                }
                AstKind::TSTypeAliasDeclaration(decl) => {
                    if let TSType::TSTypeLiteral(lit) = &decl.type_annotation {
                        let name = decl.id.name.to_string();
                        let props = collect_ts_signature_props(&lit.members);
                        if !props.is_empty() {
                            prop_types.insert(name, props);
                        }
                    }
                }
                _ => {}
            }
        }

        let scoping = semantic.scoping();

        // Pass 2: find functions with typed props parameter
        for node in nodes.iter() {
            let param = match node.kind() {
                AstKind::FormalParameter(p) => p,
                _ => continue,
            };

            if !is_in_component_function(nodes, node.id()) {
                continue;
            }

            let declared_props = match resolve_props(param, &prop_types) {
                Some(p) => p,
                None => continue,
            };

            let used_props: HashSet<String> = match &param.pattern {
                BindingPattern::ObjectPattern(obj) => {
                    if obj.rest.is_some() {
                        continue;
                    }
                    obj.properties
                        .iter()
                        .filter_map(|p| prop_key_name(&p.key))
                        .collect()
                }
                BindingPattern::BindingIdentifier(ident) => {
                    let Some(sym) = ident.symbol_id.get() else {
                        continue;
                    };
                    match collect_accessed_props(scoping, nodes, sym) {
                        Some(props) => props,
                        None => continue,
                    }
                }
                _ => continue,
            };

            for prop in &declared_props {
                if !used_props.contains(&prop.name) {
                    let (line, column) = byte_offset_to_line_col(ctx.source, prop.span_start);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Prop `{}` is declared but never read in the component.",
                            prop.name
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

struct PropInfo {
    name: String,
    span_start: usize,
}

impl Clone for PropInfo {
    fn clone(&self) -> Self {
        PropInfo {
            name: self.name.clone(),
            span_start: self.span_start,
        }
    }
}

/// A parameter is a component's props parameter only when it is the *direct*
/// parameter of the component function (depth 1). The first function-like
/// ancestor encountered must therefore be the component itself — a parameter
/// nested inside a callback within the component body (e.g. `getOptionLabel={(o:
/// Model) => ...}`) belongs to the callback, not the component.
fn is_in_component_function(
    nodes: &oxc_semantic::AstNodes,
    node_id: oxc_semantic::NodeId,
) -> bool {
    let mut ancestors = nodes.ancestor_kinds(node_id);
    match ancestors.next() {
        // The `FormalParameters` wrapper that sits between the parameter and its
        // enclosing function is the parameter's immediate parent.
        Some(AstKind::FormalParameters(_)) => {}
        _ => return false,
    }
    match ancestors.next() {
        Some(AstKind::Function(f)) => f
            .id
            .as_ref()
            .is_some_and(|id| id.name.as_str().starts_with(char::is_uppercase)),
        Some(AstKind::ArrowFunctionExpression(_)) => match ancestors.next() {
            Some(AstKind::VariableDeclarator(decl)) => match &decl.id {
                BindingPattern::BindingIdentifier(ident) => {
                    ident.name.as_str().starts_with(char::is_uppercase)
                }
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}

fn collect_ts_signature_props(sigs: &[TSSignature]) -> Vec<PropInfo> {
    sigs.iter()
        .filter_map(|sig| {
            if let TSSignature::TSPropertySignature(prop) = sig {
                let name = prop_key_name(&prop.key)?;
                Some(PropInfo {
                    name,
                    span_start: prop.span.start as usize,
                })
            } else {
                None
            }
        })
        .collect()
}

fn resolve_props<'a>(
    param: &oxc_ast::ast::FormalParameter<'a>,
    prop_types: &HashMap<String, Vec<PropInfo>>,
) -> Option<Vec<PropInfo>> {
    let ta = param.type_annotation.as_ref()?;
    match &ta.type_annotation {
        TSType::TSTypeReference(tref) => {
            let TSTypeName::IdentifierReference(ident) = &tref.type_name else {
                return None;
            };
            let type_name = ident.name.as_str();
            prop_types.get(type_name).cloned()
        }
        TSType::TSTypeLiteral(lit) => {
            let props = collect_ts_signature_props(&lit.members);
            if props.is_empty() { None } else { Some(props) }
        }
        _ => None,
    }
}

fn collect_accessed_props(
    scoping: &oxc_semantic::Scoping,
    nodes: &oxc_semantic::AstNodes,
    sym: oxc_semantic::SymbolId,
) -> Option<HashSet<String>> {
    let mut used = HashSet::new();
    for reference in scoping.get_resolved_references(sym) {
        let ref_id = reference.node_id();
        let parent_id = nodes.parent_id(ref_id);
        match nodes.kind(parent_id) {
            AstKind::StaticMemberExpression(member) => {
                used.insert(member.property.name.to_string());
            }
            AstKind::VariableDeclarator(decl) => {
                let BindingPattern::ObjectPattern(obj) = &decl.id else {
                    return None;
                };
                if obj.rest.is_some() {
                    return None;
                }
                for prop in &obj.properties {
                    if let Some(name) = prop_key_name(&prop.key) {
                        used.insert(name);
                    }
                }
            }
            _ => return None,
        }
    }
    Some(used)
}

fn prop_key_name(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(ident) => Some(ident.name.to_string()),
        _ => None,
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_unused_prop_in_interface() {
        let src = r#"
interface Props {
  name: string;
  age: number;
}
function App({ name }: Props) {
  return name;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`age`"));
    }

    #[test]
    fn flags_arrow_with_unused_prop() {
        let src = r#"
interface Props { x: number; y: number; }
const App = ({ x }: Props) => x;
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`y`"));
    }

    #[test]
    fn allows_all_props_used() {
        let src = r#"
interface Props { name: string; age: number; }
function App({ name, age }: Props) {
  return name + age;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn skips_non_component_function() {
        let src = r#"
function helper({ a }: { a: number; b: string }) {
  return a;
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Regression for #2015: a data-model type referenced as a callback
    /// parameter type inside a component is not the component's props type.
    /// `CountryType.suggested` is never read by `getOptionLabel` by design and
    /// must not be flagged.
    #[test]
    fn allows_model_type_as_callback_param_inside_component() {
        let src = r#"
function CountrySelect() {
  return (
    <Autocomplete
      options={countries}
      getOptionLabel={(option: CountryType) =>
        `${option.label} (${option.code}) +${option.phone}`
      }
    />
  );
}

interface CountryType {
  code: string;
  label: string;
  phone: string;
  suggested?: boolean;
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// A genuinely unused prop on the component's *direct* props parameter must
    /// still fire even when a nested callback also takes a typed parameter.
    #[test]
    fn flags_unused_direct_prop_despite_nested_callback() {
        let src = r#"
interface Props { title: string; subtitle: string; }
interface Item { id: string; }
function List({ title }: Props) {
  return items.map((item: Item) => item.id + title);
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`subtitle`"));
    }
}
