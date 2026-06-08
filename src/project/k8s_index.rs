//! Cross-file Kubernetes resource index built once per run.
//!
//! Rules that need cross-manifest visibility (HPA → Deployment, Ingress →
//! Service, ServiceAccount existence, env valueFrom resolves to a real
//! Secret/ConfigMap) currently re-parse the world per-rule. The index
//! parses every YAML file once and exposes name/label lookup tables.
//!
//! How it works:
//! - `K8sIndex::build(files)` walks every `Language::Yaml` file with
//!   tree-sitter-yaml. Multi-document YAML is supported — each
//!   `document` child of the root `stream` is inspected independently.
//! - For each k8s manifest (mapping with both `apiVersion` and `kind`):
//!   - record `(kind, namespace) → name` so rules can ask "does this
//!     `Service/web` exist in namespace `default`?";
//!   - for `Deployment` / `StatefulSet` / `DaemonSet` / `ReplicaSet` /
//!     `Job`: record `spec.template.metadata.labels` so rules can
//!     resolve a `Service`/`NetworkPolicy` selector against the actual
//!     pod label set;
//!   - for `Service`: record `spec.selector` so `ServiceMonitor`
//!     rules can confirm the target service has a matching selector.
//! - Missing namespace defaults to `"default"` (k8s implicit namespace).
//! - Labels and selectors are stored as plain string maps. Rules that
//!   need to test "does any workload's labels match all keys/values in
//!   this selector?" call `has_pods_matching`.

use rustc_hash::{FxHashMap, FxHashSet};

use rayon::prelude::*;
use tree_sitter::{Node, Parser, Tree};

use crate::files::{Language, SourceFile};
use crate::rules::yaml_k8s_helpers as y;

/// Default Kubernetes namespace used when `metadata.namespace` is omitted.
const DEFAULT_NAMESPACE: &str = "default";

/// Snapshot of Kubernetes resources parsed across the input set. Frozen
/// after `build` — all accessors are read-only.
#[derive(Debug, Default)]
pub struct K8sIndex {
    /// `(kind, namespace) → set of resource names`. Used by
    /// `has_resource` to answer "does this Service exist?".
    resources: FxHashMap<(String, String), FxHashSet<String>>,
    /// `(namespace, workload_name) → labels` extracted from
    /// `spec.template.metadata.labels` of Deployments/StatefulSets/
    /// DaemonSets/ReplicaSets/Jobs.
    pod_labels: FxHashMap<(String, String), FxHashMap<String, String>>,
    /// `(namespace, service_name) → selector labels` from
    /// `spec.selector` of Service manifests.
    service_selectors: FxHashMap<(String, String), FxHashMap<String, String>>,
}

impl K8sIndex {
    /// Parse every `Language::Yaml` file in `files` and build the index.
    /// Non-YAML files are ignored. Files that fail to parse contribute
    /// nothing — they neither error nor populate entries.
    #[must_use]
    pub fn build(files: &[&SourceFile]) -> Self {
        let per_file: Vec<FileExtract> = files
            .par_iter()
            .filter(|f| matches!(f.language, Language::Yaml))
            .map_init(Parser::new, |parser, file| extract_for(parser, file))
            .flatten()
            .collect();

        let mut idx = K8sIndex::default();
        for extract in per_file {
            for resource in extract.resources {
                idx.resources
                    .entry((resource.kind, resource.namespace.clone()))
                    .or_default()
                    .insert(resource.name.clone());
                if let Some(labels) = resource.pod_template_labels {
                    idx.pod_labels
                        .insert((resource.namespace.clone(), resource.name.clone()), labels);
                }
                if let Some(selector) = resource.service_selector {
                    idx.service_selectors
                        .insert((resource.namespace, resource.name), selector);
                }
            }
        }
        idx
    }

    /// True if a resource of `kind` named `name` exists in `namespace`.
    /// Namespace lookups use the literal string — callers normalise
    /// missing namespaces to `"default"` themselves (or use
    /// `default_namespace()`).
    #[must_use]
    pub fn has_resource(&self, kind: &str, namespace: &str, name: &str) -> bool {
        self.resources
            .get(&(kind.to_string(), namespace.to_string()))
            .is_some_and(|set| set.contains(name))
    }

    /// Pod-template labels for a workload, or `None` if the workload
    /// isn't indexed (or has no `spec.template.metadata.labels`).
    #[must_use]
    pub fn pod_template_labels(
        &self,
        namespace: &str,
        workload_name: &str,
    ) -> Option<&FxHashMap<String, String>> {
        self.pod_labels
            .get(&(namespace.to_string(), workload_name.to_string()))
    }

    /// True if at least one indexed workload in `namespace` has a
    /// pod-template label set that contains every (key, value) pair in
    /// `selector`. An empty selector matches every workload — that
    /// matches Kubernetes' real semantics for `Service.spec.selector`
    /// (an empty selector is treated as "select nothing" by the API
    /// server, but most of our rules want to call out missing selectors
    /// before they reach this index).
    #[must_use]
    pub fn has_pods_matching(&self, namespace: &str, selector: &FxHashMap<String, String>) -> bool {
        if selector.is_empty() {
            // Conservatively match: an empty selector can't be proved
            // dangling by this index. Callers that want to flag empty
            // selectors should do so before calling here.
            return self.pod_labels.keys().any(|(ns, _)| ns == namespace);
        }
        self.pod_labels
            .iter()
            .filter(|((ns, _), _)| ns == namespace)
            .any(|(_, labels)| selector.iter().all(|(k, v)| labels.get(k) == Some(v)))
    }

    /// Selector labels for a `Service` manifest, or `None` if no such
    /// service is indexed.
    #[must_use]
    pub fn service_selector(
        &self,
        namespace: &str,
        name: &str,
    ) -> Option<&FxHashMap<String, String>> {
        self.service_selectors
            .get(&(namespace.to_string(), name.to_string()))
    }

    /// Default namespace string used when `metadata.namespace` is
    /// missing. Exposed so rule code can apply the same fallback when
    /// reading the consuming manifest.
    #[must_use]
    pub fn default_namespace() -> &'static str {
        DEFAULT_NAMESPACE
    }

    /// True when no Kubernetes resources were indexed. Useful for rules
    /// that want to short-circuit in single-file/test contexts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }
}

/// One indexed manifest's contribution to the global index.
#[derive(Debug)]
struct ResourceEntry {
    kind: String,
    namespace: String,
    name: String,
    pod_template_labels: Option<FxHashMap<String, String>>,
    service_selector: Option<FxHashMap<String, String>>,
}

#[derive(Debug, Default)]
struct FileExtract {
    resources: Vec<ResourceEntry>,
}

fn extract_for(parser: &mut Parser, file: &SourceFile) -> Option<FileExtract> {
    let source = std::fs::read_to_string(&file.path).ok()?;
    let lang: tree_sitter::Language = tree_sitter_yaml::LANGUAGE.into();
    parser.set_language(&lang).ok()?;
    let tree = parser.parse(source.as_bytes(), None)?;
    Some(extract_from_tree(&tree, source.as_bytes()))
}

/// Walk the YAML stream → document children, extracting one
/// `ResourceEntry` per k8s manifest. Multi-document files (separated by
/// `---`) contribute one entry per document.
fn extract_from_tree(tree: &Tree, source: &[u8]) -> FileExtract {
    let mut out = FileExtract::default();
    let root = tree.root_node();
    // The root is `stream`. Iterate `document` children.
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "document" {
            continue;
        }
        let Some(mapping) = y::top_mapping_of_document(child) else {
            continue;
        };
        if !y::is_k8s_manifest_mapping(mapping, source) {
            continue;
        }
        if let Some(entry) = extract_resource(mapping, source) {
            out.resources.push(entry);
        }
    }
    out
}

fn extract_resource(mapping: Node, source: &[u8]) -> Option<ResourceEntry> {
    let kind = y::manifest_kind(mapping, source)?;
    let metadata = y::descend_mapping(mapping, source, &["metadata"])?;
    let name_pair = y::find_pair(metadata, source, "name")?;
    let name = y::pair_scalar_value(name_pair, source)?;
    let namespace = y::find_pair(metadata, source, "namespace")
        .and_then(|p| y::pair_scalar_value(p, source))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_NAMESPACE.to_string());

    let pod_template_labels = match kind.as_str() {
        "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "Job" => {
            y::descend_mapping(mapping, source, &["spec", "template", "metadata", "labels"])
                .map(|m| extract_string_map(m, source))
        }
        _ => None,
    };

    let service_selector = if kind == "Service" {
        y::descend_mapping(mapping, source, &["spec", "selector"])
            .map(|m| extract_string_map(m, source))
    } else {
        None
    };

    Some(ResourceEntry {
        kind,
        namespace,
        name,
        pod_template_labels,
        service_selector,
    })
}

/// Extract every scalar `key: value` pair under a `block_mapping` into
/// a plain string map. Pairs whose value isn't a scalar (nested
/// mapping, sequence) are skipped — they aren't valid k8s label
/// values anyway.
fn extract_string_map(mapping: Node, source: &[u8]) -> FxHashMap<String, String> {
    let mut out = FxHashMap::default();
    let mut cursor = mapping.walk();
    for child in mapping.named_children(&mut cursor) {
        if child.kind() != "block_mapping_pair" {
            continue;
        }
        let Some(key) = y::pair_key_text(child, source) else {
            continue;
        };
        let Some(value) = y::pair_scalar_value(child, source) else {
            continue;
        };
        out.insert(key, value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn build_index(files: &[(&str, &str)]) -> (TempDir, K8sIndex, Vec<PathBuf>) {
        let dir = TempDir::new().unwrap();
        let mut sources = Vec::new();
        let mut paths = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            sources.push(SourceFile {
                path: p.clone(),
                language: Language::Yaml,
            });
            paths.push(p);
        }
        let refs: Vec<&SourceFile> = sources.iter().collect();
        let index = K8sIndex::build(&refs);
        (dir, index, paths)
    }

    #[test]
    fn indexes_deployment_with_default_namespace() {
        let (_dir, index, _paths) = build_index(&[(
            "deploy.yaml",
            "apiVersion: apps/v1\n\
             kind: Deployment\n\
             metadata:\n  name: web\n\
             spec:\n  template:\n    metadata:\n      labels:\n        app: web\n    spec:\n      containers:\n      - name: c\n        image: nginx\n",
        )]);
        assert!(index.has_resource("Deployment", "default", "web"));
        let labels = index.pod_template_labels("default", "web").unwrap();
        assert_eq!(labels.get("app"), Some(&"web".to_string()));
    }

    #[test]
    fn indexes_service_selector() {
        let (_dir, index, _paths) = build_index(&[(
            "svc.yaml",
            "apiVersion: v1\n\
             kind: Service\n\
             metadata:\n  name: api\n  namespace: prod\n\
             spec:\n  selector:\n    app: api\n    tier: backend\n",
        )]);
        let sel = index.service_selector("prod", "api").unwrap();
        assert_eq!(sel.get("app"), Some(&"api".to_string()));
        assert_eq!(sel.get("tier"), Some(&"backend".to_string()));
        assert!(index.has_resource("Service", "prod", "api"));
    }

    #[test]
    fn has_pods_matching_succeeds_when_all_keys_match() {
        let (_dir, index, _paths) = build_index(&[(
            "deploy.yaml",
            "apiVersion: apps/v1\n\
             kind: Deployment\n\
             metadata:\n  name: web\n\
             spec:\n  template:\n    metadata:\n      labels:\n        app: web\n        tier: frontend\n    spec:\n      containers:\n      - name: c\n        image: nginx\n",
        )]);
        let mut sel = FxHashMap::default();
        sel.insert("app".to_string(), "web".to_string());
        assert!(index.has_pods_matching("default", &sel));

        sel.insert("tier".to_string(), "frontend".to_string());
        assert!(index.has_pods_matching("default", &sel));

        sel.insert("missing".to_string(), "nope".to_string());
        assert!(!index.has_pods_matching("default", &sel));
    }

    #[test]
    fn has_pods_matching_respects_namespace() {
        let (_dir, index, _paths) = build_index(&[(
            "deploy.yaml",
            "apiVersion: apps/v1\n\
             kind: Deployment\n\
             metadata:\n  name: web\n  namespace: prod\n\
             spec:\n  template:\n    metadata:\n      labels:\n        app: web\n    spec:\n      containers:\n      - name: c\n        image: nginx\n",
        )]);
        let mut sel = FxHashMap::default();
        sel.insert("app".to_string(), "web".to_string());
        assert!(index.has_pods_matching("prod", &sel));
        assert!(!index.has_pods_matching("default", &sel));
    }

    #[test]
    fn multi_document_yaml_indexes_all_resources() {
        let (_dir, index, _paths) = build_index(&[(
            "all.yaml",
            "apiVersion: v1\n\
             kind: Service\n\
             metadata:\n  name: api\n\
             spec:\n  selector:\n    app: api\n\
             ---\n\
             apiVersion: apps/v1\n\
             kind: Deployment\n\
             metadata:\n  name: api\n\
             spec:\n  template:\n    metadata:\n      labels:\n        app: api\n    spec:\n      containers:\n      - name: c\n        image: nginx\n",
        )]);
        assert!(index.has_resource("Service", "default", "api"));
        assert!(index.has_resource("Deployment", "default", "api"));
        let labels = index.pod_template_labels("default", "api").unwrap();
        assert_eq!(labels.get("app"), Some(&"api".to_string()));
    }

    #[test]
    fn missing_namespace_defaults_to_default() {
        let (_dir, index, _paths) = build_index(&[(
            "sa.yaml",
            "apiVersion: v1\n\
             kind: ServiceAccount\n\
             metadata:\n  name: deployer\n",
        )]);
        assert!(index.has_resource("ServiceAccount", "default", "deployer"));
        assert!(!index.has_resource("ServiceAccount", "prod", "deployer"));
    }

    #[test]
    fn non_k8s_yaml_is_ignored() {
        let (_dir, index, _paths) = build_index(&[(
            "config.yaml",
            "name: ci\non: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n",
        )]);
        assert!(index.is_empty());
    }

    #[test]
    fn empty_input_yields_empty_index() {
        let index = K8sIndex::build(&[]);
        assert!(index.is_empty());
        assert!(!index.has_resource("Service", "default", "x"));
        assert!(index.pod_template_labels("default", "x").is_none());
    }

    #[test]
    fn statefulset_pod_labels_indexed() {
        let (_dir, index, _paths) = build_index(&[(
            "ss.yaml",
            "apiVersion: apps/v1\n\
             kind: StatefulSet\n\
             metadata:\n  name: db\n\
             spec:\n  template:\n    metadata:\n      labels:\n        app: db\n    spec:\n      containers:\n      - name: c\n        image: postgres\n",
        )]);
        let labels = index.pod_template_labels("default", "db").unwrap();
        assert_eq!(labels.get("app"), Some(&"db".to_string()));
    }
}
