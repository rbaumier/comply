# Intégration tsgolint dans comply

**tsgolint a déjà 61 règles type-aware implémentées.** On n'a pas besoin de les écrire, juste d'intégrer tsgolint comme backend (comme oxlint/clippy).

---

## Architecture

```
comply (Rust)
  └── spawn tsgolint headless (binaire Go, protocole binaire IPC)
        └── typescript-go internals (via go:linkname)
              └── Type checker natif (~10x plus rapide que tsc)
```

**Repo :** [oxc-project/tsgolint](https://github.com/oxc-project/tsgolint)

---

## Règles disponibles (61 règles)

tsgolint implémente déjà toutes les règles type-aware de typescript-eslint :

### Async / Promises
- `no-floating-promises` — Promise non-awaited/catchée
- `no-misused-promises` — Promise dans contexte void
- `await-thenable` — await sur non-thenable
- `require-await` — async fn sans await
- `promise-function-async` — fn retournant Promise doit être async

### Boolean / Conditions
- `strict-boolean-expressions` — conditions explicitement boolean
- `no-unnecessary-condition` — condition toujours true/false
- `no-unnecessary-boolean-literal-compare` — `x === true` inutile

### Type Safety
- `no-unsafe-argument` — any passé à fn typée
- `no-unsafe-assignment` — assigner any
- `no-unsafe-call` — appeler any
- `no-unsafe-member-access` — accéder à membre de any
- `no-unsafe-return` — retourner any
- `no-unsafe-enum-comparison` — comparer enum avec autre type
- `no-unsafe-unary-minus` — `-` sur non-number

### Nullish
- `no-unnecessary-type-assertion` — `x as T` inutile
- `no-non-null-assertion` — `x!` 
- `no-non-null-asserted-optional-chain` — `x?.y!`
- `no-non-null-asserted-nullish-coalescing` — `x! ?? y`
- `prefer-nullish-coalescing` — `??` au lieu de `||`
- `prefer-optional-chain` — `?.` au lieu de `&&`

### Classes / Methods
- `no-unnecessary-qualifier` — qualificateur inutile
- `no-unnecessary-template-expression` — template literal simple
- `no-unnecessary-type-arguments` — type args inférables
- `no-unnecessary-type-constraint` — `extends unknown` inutile
- `no-useless-empty-export` — export {} inutile
- `prefer-return-this-type` — retourner `this` type
- `unbound-method` — méthode non-liée passée en callback
- `no-meaningless-void-operator` — `void x` inutile
- `use-unknown-in-catch-callback-variable` — `unknown` dans catch

### Arrays / Loops
- `no-for-in-array` — `for...in` sur array
- `prefer-find` — `.filter()[0]` → `.find()`
- `prefer-includes` — `.indexOf() !== -1` → `.includes()`
- `prefer-reduce-type-parameter` — type param explicite
- `prefer-string-starts-ends-with` — `.startsWith()/.endsWith()`
- `require-array-sort-compare` — `.sort()` avec comparateur

### Misc
- `consistent-type-exports` — `export type`
- `no-confusing-void-expression` — void dans expression
- `no-duplicate-type-constituents` — `A | A`
- `no-mixed-enums` — enum mixte string/number
- `no-redundant-type-constituents` — `string | 'a'`
- `no-throw-literal` — throw non-Error
- `no-unnecessary-parameter-property-assignment` — assigner param à this
- `restrict-plus-operands` — `+` entre types incompatibles
- `restrict-template-expressions` — interpolation de any
- `return-await` — return await cohérent
- `switch-exhaustiveness-check` — switch exhaustif
- `prefer-readonly` — champ jamais réassigné
- `prefer-regexp-exec` — `.exec()` au lieu de `.match()`
- ... et plus

---

## Protocole IPC headless

tsgolint utilise un protocole binaire sur stdin/stdout (pas JSON-RPC).

### Format entrée (stdin)

```json
{
  "version": 2,
  "configs": [{
    "file_paths": ["src/a.ts", "src/b.ts"],
    "rules": [
      {"name": "no-floating-promises", "options": {}},
      {"name": "strict-boolean-expressions", "options": {}}
    ]
  }],
  "source_overrides": {},
  "report_syntactic": false,
  "report_semantic": false
}
```

### Format sortie (stdout)

Header 5 octets : `[uint32 LE length][uint8 messageType]` puis payload JSON.

```rust
// Message types
const MSG_DIAGNOSTIC: u8 = 0;
const MSG_DONE: u8 = 1;
const MSG_ERROR: u8 = 2;
```

### Payload diagnostic

```json
{
  "kind": 0,
  "range": {"pos": 42, "end": 55},
  "message": {
    "id": "floating-promise",
    "description": "Promises must be awaited, caught, or returned",
    "help": "Add `await` or `.catch()`"
  },
  "file_path": "src/api.ts",
  "rule": "no-floating-promises",
  "fixes": [...],
  "suggestions": [...]
}
```

---

## Implémentation dans comply

### 1. Module Rust pour tsgolint

```rust
// src/rules/delegated/tsgolint.rs

use std::io::{Read, Write};
use std::process::{Command, Stdio};

pub struct TsgolintRunner {
    binary_path: PathBuf,
}

impl TsgolintRunner {
    pub fn run(&self, files: &[PathBuf], rules: &[&str]) -> Result<Vec<Diagnostic>> {
        let mut child = Command::new(&self.binary_path)
            .arg("headless")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        
        // Send input
        let input = TsgolintInput {
            version: 2,
            configs: vec![TsgolintConfig {
                file_paths: files.iter().map(|p| p.to_string_lossy().into()).collect(),
                rules: rules.iter().map(|r| TsgolintRule { name: r.to_string(), options: json!({}) }).collect(),
            }],
            source_overrides: HashMap::new(),
            report_syntactic: false,
            report_semantic: false,
        };
        
        let stdin = child.stdin.as_mut().unwrap();
        serde_json::to_writer(stdin, &input)?;
        stdin.flush()?;
        drop(child.stdin.take());
        
        // Read output (binary framed)
        let stdout = child.stdout.as_mut().unwrap();
        let mut diagnostics = Vec::new();
        
        loop {
            let mut header = [0u8; 5];
            if stdout.read_exact(&mut header).is_err() {
                break;
            }
            
            let len = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
            let msg_type = header[4];
            
            let mut payload = vec![0u8; len];
            stdout.read_exact(&mut payload)?;
            
            match msg_type {
                0 => { // Diagnostic
                    let diag: TsgolintDiagnostic = serde_json::from_slice(&payload)?;
                    diagnostics.push(diag.into());
                }
                1 => break, // Done
                2 => { // Error
                    let err: TsgolintError = serde_json::from_slice(&payload)?;
                    return Err(anyhow::anyhow!("tsgolint error: {}", err.message));
                }
                _ => {}
            }
        }
        
        child.wait()?;
        Ok(diagnostics)
    }
}
```

### 2. Conversion en Diagnostic comply

```rust
impl From<TsgolintDiagnostic> for Diagnostic {
    fn from(d: TsgolintDiagnostic) -> Self {
        // Note: tsgolint gives UTF-16 offsets, need to convert to line:col
        Diagnostic {
            path: PathBuf::from(d.file_path.unwrap_or_default()),
            line: 0,    // TODO: compute from offset
            column: 0,  // TODO: compute from offset
            rule_id: format!("tsgolint/{}", d.rule.unwrap_or_default()),
            message: d.message.description,
            severity: Severity::Error,
        }
    }
}
```

### 3. Flag CLI

```rust
// src/main.rs
#[derive(Parser)]
struct Args {
    #[arg(long)]
    with_types: bool,
}

// Dans le runner
if args.with_types {
    let tsgolint = TsgolintRunner::new()?;
    let type_diags = tsgolint.run(&ts_files, &enabled_rules)?;
    diagnostics.extend(type_diags);
}
```

---

## Plan d'implémentation

### Phase 1 : Setup (1 jour)

1. Ajouter tsgolint comme dépendance
   - Option A : Git submodule + build Go dans le build comply
   - Option B : Télécharger binaire pré-compilé au runtime (lazy)
   - Option C : Distribuer avec comply (comme oxlint)

2. Tester en local
   ```bash
   git clone https://github.com/oxc-project/tsgolint
   cd tsgolint
   just init
   just build
   ./tsgolint headless < test-input.json
   ```

### Phase 2 : Intégration Rust (2-3 jours)

1. Créer `src/rules/delegated/tsgolint.rs`
2. Implémenter le protocole binaire IPC
3. Convertir les offsets UTF-16 → ligne:colonne
4. Ajouter flag `--with-types`
5. Registry des règles tsgolint disponibles

### Phase 3 : Tests & polish (1 jour)

1. Tests d'intégration
2. Gestion des erreurs (tsgolint not found, tsconfig invalide)
3. Documentation

### Phase 4 : Distribution (1 jour)

Options :
- **Option A** : Binaire tsgolint inclus dans release comply (~50MB supplémentaire)
- **Option B** : Téléchargement lazy au premier `--with-types` (comme deno)
- **Option C** : L'utilisateur installe tsgolint séparément (`npm i -g oxlint-tsgolint`)

---

## Configuration

### comply.toml

```toml
[tsgolint]
enabled = true
rules = [
  "no-floating-promises",
  "no-misused-promises", 
  "await-thenable",
  "strict-boolean-expressions",
  "no-unnecessary-condition",
  # ... ou "all" pour activer toutes les 61 règles
]
```

### Règles à activer par défaut

Les plus impactantes :
1. `no-floating-promises` — bugs async silencieux
2. `no-misused-promises` — Promise dans mauvais contexte
3. `await-thenable` — await inutile
4. `no-unsafe-argument` — any leak
5. `no-unsafe-return` — return any

---

## Dépendances

- Go 1.26+ (pour build tsgolint)
- typescript-go (submodule dans tsgolint)
- Binaire tsgolint : ~50MB par plateforme

---

## Effort total estimé

| Phase | Durée |
|-------|-------|
| Setup | 1 jour |
| Intégration Rust | 2-3 jours |
| Tests | 1 jour |
| Distribution | 1 jour |
| **Total** | **5-6 jours** |

---

## Références

- [oxc-project/tsgolint](https://github.com/oxc-project/tsgolint)
- [microsoft/typescript-go](https://github.com/microsoft/typescript-go)
- [tsgolint ARCHITECTURE.md](https://github.com/oxc-project/tsgolint/blob/main/ARCHITECTURE.md)
- [typescript-eslint rules](https://typescript-eslint.io/rules/)
