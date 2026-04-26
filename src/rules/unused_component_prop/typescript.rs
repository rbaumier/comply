use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{
    BindingPattern, PropertyKey, TSSignature, TSType, TSTypeName,
};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
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
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, prop.span_start);
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "unused-component-prop".into(),
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
        })
    }
}

struct PropInfo {
    name: String,
    span_start: usize,
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

impl Clone for PropInfo {
    fn clone(&self) -> Self {
        PropInfo {
            name: self.name.clone(),
            span_start: self.span_start,
        }
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

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
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
    fn flags_unused_prop_in_type_alias() {
        let src = r#"
type Props = {
  name: string;
  age: number;
};
function App({ name }: Props) {
  return name;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`age`"));
    }

    #[test]
    fn flags_unused_prop_inline_type() {
        let src = r#"
function App({ name }: { name: string; age: number }) {
  return name;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`age`"));
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
    fn allows_rest_spread() {
        let src = r#"
interface Props { name: string; age: number; email: string; }
function App({ name, ...rest }: Props) {
  return name;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_with_all_props() {
        let src = r#"
interface Props { x: number; }
const App = ({ x }: Props) => x;
"#;
        assert!(run_on(src).is_empty());
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
    fn flags_unused_with_member_access() {
        let src = r#"
interface Props { name: string; age: number; }
function App(props: Props) {
  return props.name;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`age`"));
    }

    #[test]
    fn allows_all_props_via_member_access() {
        let src = r#"
interface Props { name: string; age: number; }
function App(props: Props) {
  return props.name + props.age;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_props_passed_opaquely() {
        let src = r#"
interface Props { name: string; age: number; }
function App(props: Props) {
  return doSomething(props);
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_unused_with_secondary_destructure() {
        let src = r#"
interface Props { name: string; age: number; email: string; }
function App(props: Props) {
  const { name, age } = props;
  return name + age;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`email`"));
    }

    #[test]
    fn allows_secondary_destructure_with_rest() {
        let src = r#"
interface Props { name: string; age: number; email: string; }
function App(props: Props) {
  const { name, ...rest } = props;
  return name;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_props_parameter() {
        let src = r#"
function helper({ a }: { a: number; b: string }) {
  return a;
}
"#;
        // Non-component functions also get checked (inline type)
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
