use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let rules_dir = Path::new(&manifest_dir).join("src/rules");
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("generated_rules.rs");

    println!("cargo:rerun-if-changed=src/rules/");

    // Directories that have register() but share an ID with a delegated (tsgolint) rule.
    // Exclude them from codegen to avoid duplicate rule IDs in all_rule_defs().
    const EXCLUDED: &[&str] = &[
        "prefer_includes",
        "prefer_regexp_exec",
        "prefer_string_starts_ends_with",
    ];

    let mut rule_names: Vec<String> = Vec::new();

    let entries = fs::read_dir(&rules_dir).expect("failed to read src/rules/");
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let mod_rs = path.join("mod.rs");
        if !mod_rs.exists() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if EXCLUDED.contains(&name.as_str()) {
            continue;
        }
        let content = fs::read_to_string(&mod_rs).unwrap_or_default();
        if content.contains("pub fn register() -> RuleDef") {
            rule_names.push(name);
        }
    }

    rule_names.sort();

    let mut out = String::new();
    for name in &rule_names {
        let mod_path = rules_dir.join(name).join("mod.rs");
        out.push_str(&format!("#[path = {:?}]\n", mod_path));
        out.push_str(&format!("pub mod {};\n", name));
    }
    out.push('\n');
    out.push_str("pub fn all_rule_defs() -> Vec<RuleDef> {\n");
    out.push_str("    let mut rules = vec![\n");
    for name in &rule_names {
        out.push_str(&format!("        {}::register(),\n", name));
    }
    out.push_str("    ];\n");
    out.push_str("    rules.extend(delegated::register_all());\n");
    out.push_str("    rules.extend(delegated::register_tsgolint());\n");
    out.push_str("    rules.extend(delegated::register_type_aware());\n");
    out.push_str("    rules\n");
    out.push_str("}\n");

    fs::write(out_path, &out).expect("failed to write generated_rules.rs");
}
