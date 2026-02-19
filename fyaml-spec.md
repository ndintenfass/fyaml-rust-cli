# FYAML CLI Specification v2

**Standalone Introduction, Specification, and Product Requirements (Rust static binary)**

**Status:** Revised draft — incorporates design decisions from collaborative review.

---

## 1. Introduction

### 1.1 What FYAML is

FYAML ("filesystem-backed YAML") is a convention for representing a single YAML document as a directory tree of smaller YAML fragments. A FYAML tool packs that tree into a single YAML document.

Key property: **FYAML is a one-way transformation.**

- Many different directory structures can pack to the same YAML.
- The tool is not required to recover the original layout from the packed YAML.
- This enables refactoring the directory structure over time without changing the packed YAML output.

### 1.2 What FYAML is for

Large YAML configurations often become difficult to maintain because they are monolithic, hard to navigate in code review, brittle to edit due to indentation and long-range coupling, and hard to reuse.

FYAML helps by enabling:

- **Modularity:** small, focused YAML files per component.
- **Reviewability:** diffs and PR review at the fragment level.
- **Team scalability:** parallel edits with fewer conflicts.
- **Refactoring:** reorganize files/directories while keeping packed YAML identical.

### 1.3 What FYAML does not do

FYAML tooling must not become a templating language:

- No variable substitution, no conditionals, no loops.
- No environment interpolation.
- No network access.
- No schema awareness (generic YAML only).

---

## 2. Product Requirements

### 2.1 Product goals

- A single CLI binary (`fyaml`) that performs FYAML operations on a directory.
- Predictable, deterministic packing.
- Strong diagnostics and developer experience for common failure modes.
- CI-friendly exit codes and machine-readable diagnostics.

### 2.2 Non-goals

- No promise of reversibility (no "true unpack").
- No domain-specific shortcuts.
- No inline templating features.
- No include directives (deferred to phase 2; see Appendix B).

### 2.3 Primary user stories

1. As a developer, I can split a massive YAML into a tree of fragments and reliably pack it for deployment.
2. As a team, we can refactor the directory structure without changing the packed YAML output.
3. As a CI system, we can validate that a repo's FYAML directory packs correctly and fails with actionable output.
4. As an engineer debugging issues, I can see exactly which files were used and why something was ignored or collided.

### 2.4 Developer experience bar

The CLI should:

- Fail fast with actionable error messages.
- Provide `validate` and `explain` commands so users don't have to infer behavior.
- Be deterministic by default.
- Provide a `diff` capability to support refactoring with confidence.

---

## 3. CLI Surface Area

### 3.1 Command summary

```
fyaml <command> [options]
```

Required commands: `pack`, `validate`, `explain`

Recommended: `diff`, `scaffold`

### 3.2 pack

Convert a directory tree into a single YAML document.

```
fyaml pack <DIR> [-o <FILE>] [--format yaml|json] [flags...]
```

- Reads FYAML-structured inputs under `<DIR>`.
- Produces a single YAML document to stdout by default.
- Emits a version header comment by default (e.g., `# packed by fyaml v0.1.0`). Suppress with `--no-header`.
- Deterministic output in canonical mode.
- `<DIR>` is always a directory path; no stdin mode.

### 3.3 validate

Check that a directory is packable under the rules.

```
fyaml validate <DIR> [--json] [--strict] [flags...]
```

- Performs all validations required for a successful pack.
- Does not emit the packed YAML.
- Exits non-zero on errors.

### 3.4 explain

Provide a human-friendly trace of how the directory maps to YAML.

```
fyaml explain <DIR> [--json] [flags...]
```

Prints:

- Derived key tree (what keys come from what paths).
- Ignored files and why.
- Sequence vs mapping decisions.
- Any warnings.

### 3.5 diff (recommended)

Compare two FYAML directories by their packed semantics.

```
fyaml diff <DIR_A> <DIR_B> [--format path|json] [flags...]
```

- Packs both directories to ASTs (not bytes), compares semantic equality.
- Prints the first difference path with a short explanation (and optionally all differences).

### 3.6 scaffold (optional, explicitly non-invertible)

Generate a FYAML-friendly directory layout from a YAML input to help users get started.

```
fyaml scaffold <INPUT.yml> <DIR> [layout flags...]
```

**Critical disclaimer:** Scaffold is not an inverse of pack. It generates *a* layout that packs to the input YAML, not *the original* layout.

---

## 4. Core Specification: Directory → YAML Mapping

### 4.1 Input classification

- **YAML files:** `.yml`, `.yaml` (case-insensitive extension matching).
- **Non-YAML files:** ignored.
- **Hidden entries** (starting with `.`): ignored by default; optionally include with `--include-hidden`.

### 4.2 Filename-to-key rules

The key derived from a filename is the filename with its final `.yml` or `.yaml` extension stripped.

**Examples:**
- `database.yml` → key `database`
- `foo.bar.yml` → key `foo.bar`

#### Multi-dot filenames

Filenames that produce keys containing dots (e.g., `foo.bar.yml` → `foo.bar`) emit a **warning** by default, because they are often accidental.

- `--allow-dotted-keys` suppresses this warning.

#### YAML reserved word filenames

Filenames that match YAML reserved words (`true`, `false`, `yes`, `no`, `null`, `on`, `off`) are **errors** by default, because they create ambiguity about whether the key is a string or a YAML special value.

- `--allow-reserved-keys` permits these filenames; the corresponding keys are emitted as quoted strings.

#### Numeric filenames in non-sequence context

A file like `1.yml` in a directory that is not treated as a sequence directory produces the string key `"1"` (quoted). See §4.4 for sequence rules.

### 4.3 Root modes

Provide `--root-mode` to define how the output document's root is constructed:

- **`map-root`** (default): root is a mapping derived from directory contents.
- **`seq-root`**: root is a sequence derived from numeric keys (see §4.4).
- **`file-root`**: root is the parsed YAML of a specified file.
  - Requires `--root-file <RELATIVE_PATH>`.
  - The root file is **excluded from normal directory key scanning** — it does not produce a separate key.
  - `--merge-under <KEY>` merges the packed directory mapping into that key of the root file. If the merge target exists and is not a mapping, error.

### 4.4 Sequence rule

A directory is treated as a **sequence directory** if and only if **all** child entries that contribute keys are numeric (integers: `0`, `1`, `2`, …).

Numeric keys can come from files (`0.yml`, `1.yml`, …) or directories (`0/`, `1/`, …).

**Ordering:** sort by integer value ascending.

**Gaps:** configurable via `--seq-gaps=error|warn|allow` (default `warn`).

**Mixed numeric and non-numeric children:** This is an **error**. The error message should guide the user to rename files to resolve the ambiguity — either make all children numeric (for a sequence) or all non-numeric (for a mapping).

### 4.5 Mapping rule (default)

In `map-root` mode, each directory corresponds to a YAML mapping. Its children contribute keys:

- **File** `name.yml` → mapping entry `name: <parsed YAML content>`
- **Subdirectory** `dir/` → mapping entry `dir: <mapping or sequence per §4.4>`

### 4.6 YAML parsing rules

For each YAML file:

- Parse as a single YAML document.
- Multi-document YAML (`---`) handling:
  - Default: **error** ("multi-document YAML not supported").
  - `--multi-doc=error|first|all`
    - `first`: take the first document and warn.
    - `all`: treat as a sequence of documents.

### 4.7 Conflicts and collisions (hard errors)

At any mapping level, it is an error if multiple inputs define the same key.

Collisions include:

- `foo.yml` and `foo/` in the same directory (both map to key `foo`).
- `foo.yml` and `foo.yaml` in the same directory.
- Case-only collisions on case-insensitive filesystems (must be detected; default to error).

Additionally: if an operation requires a mapping but a value is already a non-mapping and a merge is attempted, error.

### 4.8 Ignoring rules

By default, `pack` and `validate` ignore:

- Non-YAML files.
- Hidden files/directories (unless `--include-hidden`).
- Common editor junk (recommended): `*~`, `.DS_Store`, etc.

`explain` must list ignored entries and the rule that caused each to be ignored.

---

## 5. Determinism, Canonicalization, and Output Formats

### 5.1 Deterministic assembly

In canonical mode (default):

- Mapping keys are sorted lexicographically (UTF-8 byte order).
- Filesystem traversal order must not affect output.
- Numeric ordering for sequence directories is by integer value.
- Implementation may parallelize reads and parsing; output determinism is the only constraint.

### 5.2 Canonical YAML emission

Defaults:

- 2-space indent.
- `\n` newlines.
- Quoting behavior deferred to the YAML emitter library's safe defaults.
- Block scalars for multiline strings.
- Do not emit anchors unless preserve mode is explicitly enabled.
- Header comment emitted by default (`# packed by fyaml vX.Y.Z`); suppress with `--no-header`.

### 5.3 Preserve mode (optional)

`--preserve` may attempt to keep key order within source YAML fragments and retain YAML styles where feasible. Filesystem-induced mappings should still be sorted unless the user explicitly opts out. Preserve mode must clearly document which aspects are not preserved.

### 5.4 JSON output

`--format=json` emits canonical JSON (sorted keys, stable arrays).

---

## 6. Validation Specification

Validation is defined as: "this directory can be packed deterministically under the rules."

### 6.1 Validation phases

**Phase A: Scan**

- Unreadable directory/file → error.
- Collision detection: file-vs-dir same key, duplicate extensions, case-insensitive collisions.
- Invalid names: empty key (`.yml`), reserved word filenames (§4.2).

**Phase B: Parse**

- YAML parse errors with file path, line/col when available, snippet context when safe.
- Multi-document YAML handling per configuration.

**Phase C: Assemble**

- Sequence/mapping ambiguity errors.
- Merge target type mismatches (`file-root` with `--merge-under`).

### 6.2 Warnings (not errors unless --strict)

- Ignored files present (with count and examples).
- Sequence gaps.
- Large YAML fragments (suggest splitting).
- Anchors/aliases in input when canonical mode will lose them.
- Multi-dot filenames (§4.2).

---

## 7. Error Handling Requirements

### 7.1 Error message format (mandatory)

Each error must include:

1. **Summary** (1 line).
2. **Location** (path, plus YAML location if applicable).
3. **Cause** (what rule was violated).
4. **Action** (how to fix).
5. **Context** (collision partners, etc.).

### 7.2 Common error categories and remediation guidance

**Key collision:**
Show both sources and the derived key path. Suggest renaming one side or moving into a different directory.

**Invalid YAML:**
Provide parse location and a short hint (indentation, missing colon, tabs vs spaces).

**Sequence ambiguity (mixed numeric/non-numeric):**
Show the directory path and list the conflicting children. Guide the user to rename files so all children are either numeric or non-numeric.

**Reserved word filename:**
Name the file and the reserved word. Suggest renaming, or mention `--allow-reserved-keys`.

**Permissions:**
Show OS error and hint about chmod/ownership or running in CI containers.

### 7.3 Machine-readable diagnostics

`validate --json` and `explain --json` must emit a list of diagnostics with:

- `code` (stable identifier)
- `severity` (`error`/`warn`/`info`)
- `message`
- `path(s)`
- `derived_key_path` (if applicable)

This enables IDE tooling and CI annotation.

---

## 8. Developer Experience Notes

### 8.1 Recommended UX defaults

- `fyaml pack .` should "just work" for `map-root` layouts.
- `fyaml validate .` should be the first diagnostic tool users run.
- `fyaml explain .` should help users understand what happened without reading source code.

### 8.2 Make refactoring safe

- `fyaml diff dirA dirB` to compare packed semantics.
- `fyaml pack dir | sha256sum` for quick CI checks.
- `--strict` to enforce hygiene (no ignored junk, no gaps).

### 8.3 Prevent surprising behavior

- Do not silently choose sequence vs mapping when ambiguous; error with guidance.
- Do not silently ignore YAML parse errors in fragments.

### 8.4 Performance and robustness guardrails

- Stable traversal and sorting.
- Configurable size caps: `--max-yaml-bytes`.
- Windows path normalization and case-collision detection.

---

## 9. Exit Codes (CI contract)

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Unexpected/internal error |
| 2 | Invalid FYAML input tree (scan/assemble errors) |
| 3 | YAML parse error |
| 5 | Write error (output path, permissions, overwrite policy) |

---

## 10. Optional Scaffold Specification (explicitly non-invertible)

If implemented, scaffold is a helper to generate a reasonable FYAML layout from YAML.

Requirements:

- Must prominently state it is not an inverse operation.
- Must be deterministic given the same YAML and flags.
- Must provide layout flags (because no unique layout exists):
  - `--layout=flat|nested|hybrid`
  - `--seq=dir|files`
  - `--split-threshold-bytes=N`

Correctness criterion: `pack(scaffold(yaml))` must be AST-semantically equal to input YAML (not necessarily byte-equal).

---

## 11. Test Plan Requirements

### 11.1 Golden tests

- **Deterministic packing:** same tree packed twice yields identical bytes (canonical mode).
- **Collision detection:** file/dir same key, duplicate extensions, case-collision.
- **Sequence directory:** numeric ordering, gaps, mixed keys behavior (error).
- **Reserved word filenames:** error by default, quoted keys with `--allow-reserved-keys`.
- **Multi-dot filenames:** warning by default, suppressed with `--allow-dotted-keys`.

### 11.2 Semantic equivalence tests (one-way property)

- Construct multiple different trees intended to pack to the same YAML.
- Assert packed AST equality.

### 11.3 Cross-platform tests

- Windows path separators and case sensitivity.
- Stable ordering despite filesystem enumeration differences.

### 11.4 Optional scaffold tests

- `pack(scaffold(yaml))` AST-equals `yaml`.

---

## 12. Documentation Requirements

The shipped CLI must include:

- `fyaml --help` with clear descriptions and defaults.
- Examples: simple pack, sequence directory, validate and explain usage, refactor safety with diff.
- A clear statement: **"FYAML packing is one-way; directory layout is not recoverable from the packed YAML."**

---

## 13. Flag Reference (summary)

| Flag | Default | Description |
|------|---------|-------------|
| `--root-mode` | `map-root` | Root construction: `map-root`, `seq-root`, `file-root` |
| `--root-file` | — | Required with `file-root`; specifies the root YAML file |
| `--merge-under` | — | With `file-root`, merge directory mapping under this key |
| `--format` | `yaml` | Output format: `yaml`, `json` |
| `--no-header` | off | Suppress version comment in packed output |
| `--include-hidden` | off | Include hidden files/directories |
| `--seq-gaps` | `warn` | Sequence gap handling: `error`, `warn`, `allow` |
| `--multi-doc` | `error` | Multi-document YAML: `error`, `first`, `all` |
| `--allow-dotted-keys` | off | Suppress warning on multi-dot filenames |
| `--allow-reserved-keys` | off | Allow YAML reserved words as keys (emitted quoted) |
| `--preserve` | off | Attempt to preserve source key order and styles |
| `--strict` | off | Promote warnings to errors |
| `--max-yaml-bytes` | — | Size cap for input YAML files |
| `-o` | stdout | Output file path |
| `--json` | off | Machine-readable diagnostic output (for `validate`, `explain`) |

---

## Appendix A: Worked Examples

### Example 1: Simple mapping (map-root)

Directory structure:
```
myconfig/
├── database.yml
├── server.yml
└── logging.yml
```

`database.yml`:
```yaml
host: localhost
port: 5432
name: myapp
```

`server.yml`:
```yaml
port: 8080
workers: 4
```

`logging.yml`:
```yaml
level: info
format: json
```

`fyaml pack myconfig/` produces:
```yaml
# packed by fyaml v0.1.0
database:
  host: localhost
  name: myapp
  port: 5432
logging:
  format: json
  level: info
server:
  port: 8080
  workers: 4
```

Note: top-level keys are sorted lexicographically. Keys within each fragment are also sorted in canonical mode.

### Example 2: Nested mappings

Directory structure:
```
infra/
├── production/
│   ├── database.yml
│   └── cache.yml
└── staging/
    ├── database.yml
    └── cache.yml
```

`infra/production/database.yml`:
```yaml
host: prod-db.internal
replicas: 3
```

`infra/production/cache.yml`:
```yaml
host: prod-cache.internal
ttl: 3600
```

`infra/staging/database.yml`:
```yaml
host: staging-db.internal
replicas: 1
```

`infra/staging/cache.yml`:
```yaml
host: staging-cache.internal
ttl: 60
```

`fyaml pack infra/` produces:
```yaml
# packed by fyaml v0.1.0
production:
  cache:
    host: prod-cache.internal
    ttl: 3600
  database:
    host: prod-db.internal
    replicas: 3
staging:
  cache:
    host: staging-cache.internal
    ttl: 60
  database:
    host: staging-db.internal
    replicas: 1
```

### Example 3: Sequence directory

Directory structure:
```
pipeline/
├── steps/
│   ├── 0.yml
│   ├── 1.yml
│   └── 2.yml
└── name.yml
```

`pipeline/name.yml`:
```yaml
my-data-pipeline
```

`pipeline/steps/0.yml`:
```yaml
action: extract
source: s3://bucket/raw
```

`pipeline/steps/1.yml`:
```yaml
action: transform
script: normalize.py
```

`pipeline/steps/2.yml`:
```yaml
action: load
target: warehouse
```

`fyaml pack pipeline/` produces:
```yaml
# packed by fyaml v0.1.0
name: my-data-pipeline
steps:
  - action: extract
    source: s3://bucket/raw
  - action: transform
    script: normalize.py
  - action: load
    target: warehouse
```

Note: `steps/` is detected as a sequence directory because all children have numeric keys. Items are ordered by integer value.

### Example 4: Collision error

Directory structure:
```
broken/
├── auth.yml
└── auth/
    └── provider.yml
```

`fyaml pack broken/` produces an error:

```
error[E001]: key collision at `auth`

  Sources:
    broken/auth.yml
    broken/auth/

  Both resolve to key `auth` at the root level.

  Fix: rename one source to use a different key, or move one
  into a subdirectory.
```

Exit code: 2

---

## Appendix B: Phase 2 — Include Directives (deferred)

The following features are deferred to a future phase. They are documented here for planning purposes and to preserve design intent.

### Motivation

Includes would enable clean management of large multiline content (scripts, policies, certificates) without embedding unreadable YAML block scalars.

### Planned design (subject to revision)

- Includes off by default; enable with `--includes=on`.
- Directive syntax: `<<include(path)>>`, `<<include-text(path)>>`, `<<include-yaml(path)>>`.
- Paths resolve **relative to the file containing the directive**.
- Sandboxing: includes must resolve to a target within the FYAML root by default.
- Symlink policy: `--follow-symlinks=never|within-root|always`.
- Cycle detection via canonical path stack.
- Size limits via `--max-include-bytes`.
- Root file (`--root-file`) does not participate in include resolution in phase 2's initial scope.
- Exit code 4 reserved for include errors.

---

## Appendix C: Suggested Defaults (summary)

| Setting | Default |
|---------|---------|
| Root mode | `map-root` |
| Includes | Off (phase 2) |
| Deterministic sorting | On |
| Sequence directories | Enabled with strict ambiguity error |
| Multi-document YAML | Error |
| Hidden files | Ignored |
| Dotted key names | Warning |
| Reserved word filenames | Error |
| Header comment | On |
| Warnings | Shown; `--strict` promotes to errors |
