use crate::config::{BuildOptions, MultiDocMode, RootMode, SeqGapMode};
use crate::diagnostics::{Category, Diagnostic, Severity};
use serde::Deserialize;
use serde::Serialize;
use serde_yaml::{Mapping, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const RESERVED_YAML_KEYS: &[&str] = &["true", "false", "yes", "no", "null", "on", "off"];
const LARGE_FRAGMENT_WARN_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ExplainReport {
    pub derived_keys: Vec<DerivedKey>,
    pub ignored: Vec<IgnoredEntry>,
    pub directory_modes: Vec<DirectoryMode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DerivedKey {
    pub source: String,
    pub derived_key_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IgnoredEntry {
    pub path: String,
    pub rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectoryMode {
    pub directory: String,
    pub mode: String,
    pub contributors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub value: Option<Value>,
    pub diagnostics: Vec<Diagnostic>,
    pub explain: ExplainReport,
}

pub fn build(root: &Path, options: &BuildOptions) -> BuildOutcome {
    let mut ctx = BuildContext::new(root, options.clone());

    if !root.exists() {
        ctx.diag(
            Diagnostic::error(
                "E000",
                "input directory does not exist",
                Category::InvalidInput,
            )
            .with_location(root.display().to_string())
            .with_cause("The provided path is missing.")
            .with_action("Pass an existing directory to fyaml commands."),
        );
        return ctx.finish(None);
    }

    if !root.is_dir() {
        ctx.diag(
            Diagnostic::error(
                "E000",
                "input path is not a directory",
                Category::InvalidInput,
            )
            .with_location(root.display().to_string())
            .with_cause("FYAML operations require a directory root.")
            .with_action("Provide a directory path as the command argument."),
        );
        return ctx.finish(None);
    }

    let value = match options.root_mode {
        RootMode::MapRoot => Some(ctx.assemble_directory(root, "", true, None)),
        RootMode::SeqRoot => {
            let built = ctx.assemble_directory(root, "", false, None);
            match built {
                Value::Sequence(_) => Some(built),
                Value::Mapping(map) if map.is_empty() => Some(Value::Sequence(Vec::new())),
                _ => {
                    ctx.diag(
                        Diagnostic::error(
                            "E040",
                            "seq-root requires all root contributors to be numeric",
                            Category::InvalidInput,
                        )
                        .with_location(root.display().to_string())
                        .with_cause(
                            "At least one root-level contributor key was non-numeric, so the root is not a sequence.",
                        )
                        .with_action("Rename all root contributors to numeric keys like 0.yml, 1.yml, ..."),
                    );
                    None
                }
            }
        }
        RootMode::FileRoot => ctx.assemble_file_root(root),
    };

    if !ctx.explain.ignored.is_empty() {
        let examples = ctx
            .explain
            .ignored
            .iter()
            .take(3)
            .map(|i| i.path.clone())
            .collect::<Vec<_>>()
            .join(", ");
        ctx.diag(
            Diagnostic::warn(
                "W050",
                format!(
                    "ignored {} file(s)/directory(ies) while scanning",
                    ctx.explain.ignored.len()
                ),
            )
            .with_cause("Entries did not match FYAML inclusion rules.")
            .with_action("Run `fyaml explain` to see all ignored entries.")
            .with_context(format!("Examples: {examples}")),
        );
    }

    ctx.finish(value)
}

struct BuildContext {
    root: PathBuf,
    options: BuildOptions,
    diagnostics: Vec<Diagnostic>,
    explain: ExplainReport,
}

impl BuildContext {
    fn new(root: &Path, options: BuildOptions) -> Self {
        Self {
            root: root.to_path_buf(),
            options,
            diagnostics: Vec::new(),
            explain: ExplainReport::default(),
        }
    }

    fn finish(mut self, value: Option<Value>) -> BuildOutcome {
        if self.options.strict {
            for diagnostic in &mut self.diagnostics {
                if diagnostic.severity == Severity::Warn {
                    diagnostic.severity = Severity::Error;
                    diagnostic.code = format!("STRICT-{}", diagnostic.code);
                    diagnostic.category = Category::InvalidInput;
                    diagnostic.message = format!("strict mode violation: {}", diagnostic.message);
                }
            }
        }

        BuildOutcome {
            value,
            diagnostics: self.diagnostics,
            explain: self.explain,
        }
    }

    fn diag(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    fn add_ignored(&mut self, path: &Path, rule: &str) {
        self.explain.ignored.push(IgnoredEntry {
            path: self.display_path(path),
            rule: rule.to_string(),
        });
    }

    fn add_derived_key(&mut self, source_path: &Path, derived_key_path: &str) {
        self.explain.derived_keys.push(DerivedKey {
            source: self.display_path(source_path),
            derived_key_path: derived_key_path.to_string(),
        });
    }

    fn add_directory_mode(&mut self, directory: &Path, mode: &str, contributors: &[Contributor]) {
        let contributor_names = contributors
            .iter()
            .map(|c| format!("{} ({})", c.key, self.display_path(&c.path)))
            .collect::<Vec<_>>();
        self.explain.directory_modes.push(DirectoryMode {
            directory: self.display_path(directory),
            mode: mode.to_string(),
            contributors: contributor_names,
        });
    }

    fn display_path(&self, path: &Path) -> String {
        if let Ok(relative) = path.strip_prefix(&self.root) {
            if relative.as_os_str().is_empty() {
                ".".to_string()
            } else {
                relative.to_string_lossy().replace('\\', "/")
            }
        } else {
            path.to_string_lossy().replace('\\', "/")
        }
    }

    fn assemble_file_root(&mut self, root: &Path) -> Option<Value> {
        let root_file = match &self.options.root_file {
            Some(file) => file,
            None => {
                self.diag(
                    Diagnostic::error(
                        "E041",
                        "file-root mode requires --root-file",
                        Category::InvalidInput,
                    )
                    .with_location(root.display().to_string())
                    .with_cause("No root file was provided.")
                    .with_action(
                        "Pass --root-file <RELATIVE_PATH> when using --root-mode file-root.",
                    ),
                );
                return None;
            }
        };

        let root_file_abs = if root_file.is_absolute() {
            root_file.clone()
        } else {
            root.join(root_file)
        };

        if !root_file_abs.exists() {
            self.diag(
                Diagnostic::error("E042", "root file does not exist", Category::InvalidInput)
                    .with_location(self.display_path(&root_file_abs))
                    .with_cause("The --root-file path does not resolve to an existing file.")
                    .with_action("Use a valid relative path under the FYAML root."),
            );
            return None;
        }

        let mut root_value = self.parse_yaml_file(&root_file_abs, "$root")?;

        let dir_value = self.assemble_directory(root, "", true, Some(&root_file_abs));
        let dir_map = match dir_value {
            Value::Mapping(mapping) => mapping,
            _ => {
                self.diag(
                    Diagnostic::error(
                        "E043",
                        "internal mapping assembly failed in file-root mode",
                        Category::Internal,
                    )
                    .with_location(self.display_path(root))
                    .with_cause("Directory assembly should produce a mapping when forced.")
                    .with_action("Report this issue; this is an implementation bug."),
                );
                Mapping::new()
            }
        };

        if dir_map.is_empty() {
            return Some(root_value);
        }

        let merge_target = self.options.merge_under.clone();

        if let Some(target_key) = merge_target {
            match &mut root_value {
                Value::Mapping(root_map) => {
                    let key = Value::String(target_key.clone());
                    if let Some(existing) = root_map.get_mut(&key) {
                        match existing {
                            Value::Mapping(existing_map) => {
                                self.merge_mappings(
                                    existing_map,
                                    dir_map,
                                    &format!("{target_key}."),
                                    &self.display_path(&root_file_abs),
                                );
                            }
                            _ => {
                                self.diag(
                                    Diagnostic::error(
                                        "E044",
                                        "merge target exists but is not a mapping",
                                        Category::InvalidInput,
                                    )
                                    .with_location(self.display_path(&root_file_abs))
                                    .with_derived_key_path(target_key.clone())
                                    .with_cause(
                                        "--merge-under requires an existing mapping when the target key already exists.",
                                    )
                                    .with_action("Change the target key to a mapping or choose a different merge key."),
                                );
                            }
                        }
                    } else {
                        root_map.insert(key, Value::Mapping(dir_map));
                    }
                }
                _ => {
                    self.diag(
                        Diagnostic::error(
                            "E045",
                            "file-root merge requires root YAML to be a mapping",
                            Category::InvalidInput,
                        )
                        .with_location(self.display_path(&root_file_abs))
                        .with_cause("The root file parsed to a non-mapping value.")
                        .with_action("Use a mapping root YAML value when merging directory keys."),
                    );
                }
            }
            return Some(root_value);
        }

        match &mut root_value {
            Value::Mapping(root_map) => {
                self.merge_mappings(root_map, dir_map, "", &self.display_path(&root_file_abs));
            }
            _ => {
                self.diag(
                    Diagnostic::error(
                        "E046",
                        "file-root root YAML is not a mapping",
                        Category::InvalidInput,
                    )
                    .with_location(self.display_path(&root_file_abs))
                    .with_cause("Directory keys cannot be merged into a non-mapping root value.")
                    .with_action(
                        "Use --merge-under with a mapping target or make the root file a mapping.",
                    ),
                );
            }
        }

        Some(root_value)
    }

    fn merge_mappings(
        &mut self,
        target: &mut Mapping,
        source: Mapping,
        key_prefix: &str,
        location: &str,
    ) {
        for (key, value) in source {
            if let Some(existing) = target.get(&key) {
                let key_name = key_as_string(&key);
                let key_path = format!("{key_prefix}{key_name}");
                self.diag(
                    Diagnostic::error("E001", "key collision during merge", Category::InvalidInput)
                        .with_location(location.to_string())
                        .with_derived_key_path(key_path.clone())
                        .with_cause("Both sides of a merge define the same key.")
                        .with_action("Rename one key or move content into a different subtree.")
                        .with_context(format!(
                            "Existing value kind: {}, incoming value kind: {}",
                            value_kind(existing),
                            value_kind(&value)
                        )),
                );
            } else {
                target.insert(key, value);
            }
        }
    }

    fn assemble_directory(
        &mut self,
        directory: &Path,
        key_path: &str,
        force_map: bool,
        excluded_file: Option<&Path>,
    ) -> Value {
        let read_dir = match fs::read_dir(directory) {
            Ok(rd) => rd,
            Err(err) => {
                self.diag(
                    Diagnostic::error("E030", "unable to read directory", Category::InvalidInput)
                        .with_location(self.display_path(directory))
                        .with_cause(err.to_string())
                        .with_action("Check directory permissions and path validity."),
                );
                return Value::Mapping(Mapping::new());
            }
        };

        let excluded = excluded_file.and_then(|path| fs::canonicalize(path).ok());
        let mut contributors: Vec<Contributor> = Vec::new();

        for entry in read_dir {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    self.diag(
                        Diagnostic::error(
                            "E031",
                            "unable to iterate directory entry",
                            Category::InvalidInput,
                        )
                        .with_location(self.display_path(directory))
                        .with_cause(err.to_string())
                        .with_action("Check filesystem permissions and retry."),
                    );
                    continue;
                }
            };

            let path = entry.path();
            if excluded
                .as_ref()
                .is_some_and(|x| fs::canonicalize(&path).ok().as_ref() == Some(x))
            {
                self.add_ignored(&path, "root file excluded from normal scanning");
                continue;
            }

            let name = entry.file_name();
            let name = name.to_string_lossy();

            if !self.options.include_hidden && is_hidden_name(&name) {
                self.add_ignored(&path, "hidden entry ignored (use --include-hidden)");
                continue;
            }

            if is_editor_junk(&name) {
                self.add_ignored(&path, "editor/system junk ignored");
                continue;
            }

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(err) => {
                    self.diag(
                        Diagnostic::error(
                            "E032",
                            "unable to read entry file type",
                            Category::InvalidInput,
                        )
                        .with_location(self.display_path(&path))
                        .with_cause(err.to_string())
                        .with_action("Check filesystem permissions and retry."),
                    );
                    continue;
                }
            };

            if file_type.is_symlink() {
                self.add_ignored(&path, "symlink ignored");
                continue;
            }

            if file_type.is_dir() {
                let key = name.to_string();
                if !self.options.allow_reserved_keys && is_reserved_yaml_key(&key) {
                    self.diag(
                        Diagnostic::error(
                            "E020",
                            "reserved YAML key used as directory name",
                            Category::InvalidInput,
                        )
                        .with_location(self.display_path(&path))
                        .with_derived_key_path(join_key_path(key_path, &key))
                        .with_cause(
                            "Reserved YAML words are ambiguous without explicit string quoting.",
                        )
                        .with_action(
                            "Rename this directory or use --allow-reserved-keys to permit it.",
                        ),
                    );
                }

                contributors.push(Contributor {
                    key,
                    path,
                    kind: ContributorKind::Directory,
                });
                continue;
            }

            if file_type.is_file() {
                if !is_yaml_file(path.as_path()) {
                    self.add_ignored(&path, "non-YAML file ignored");
                    continue;
                }

                let key = strip_yaml_extension(&name);
                if key.is_empty() {
                    self.diag(
                        Diagnostic::error(
                            "E021",
                            "empty key derived from YAML filename",
                            Category::InvalidInput,
                        )
                        .with_location(self.display_path(&path))
                        .with_cause("Filename reduces to an empty key after stripping .yml/.yaml.")
                        .with_action("Rename the file to a non-empty key, e.g., config.yml."),
                    );
                    continue;
                }

                if key.contains('.') && !self.options.allow_dotted_keys {
                    self.diag(
                        Diagnostic::warn("W010", "dotted key derived from filename")
                            .with_location(self.display_path(&path))
                            .with_derived_key_path(join_key_path(key_path, &key))
                            .with_cause(
                                "Keys with dots are often accidental and can be confused with nested paths.",
                            )
                            .with_action("Rename the file or pass --allow-dotted-keys if intentional."),
                    );
                }

                if !self.options.allow_reserved_keys && is_reserved_yaml_key(&key) {
                    self.diag(
                        Diagnostic::error(
                            "E022",
                            "reserved YAML key used as filename",
                            Category::InvalidInput,
                        )
                        .with_location(self.display_path(&path))
                        .with_derived_key_path(join_key_path(key_path, &key))
                        .with_cause(
                            "Reserved YAML words are ambiguous without explicit string quoting.",
                        )
                        .with_action("Rename the file or use --allow-reserved-keys to permit it."),
                    );
                }

                contributors.push(Contributor {
                    key,
                    path,
                    kind: ContributorKind::File,
                });
                continue;
            }

            self.add_ignored(&path, "unsupported filesystem entry type");
        }

        contributors.sort_by(|a, b| {
            a.key
                .as_bytes()
                .cmp(b.key.as_bytes())
                .then(a.path.cmp(&b.path))
        });

        self.detect_key_collisions(directory, key_path, &contributors);

        let effective_mode =
            self.resolve_directory_mode(directory, key_path, force_map, &contributors);

        match effective_mode {
            DirectoryAssemblyMode::Sequence => {
                self.assemble_sequence(directory, key_path, contributors, excluded_file)
            }
            DirectoryAssemblyMode::Mapping => {
                self.assemble_mapping(directory, key_path, contributors, excluded_file)
            }
        }
    }

    fn resolve_directory_mode(
        &mut self,
        directory: &Path,
        key_path: &str,
        force_map: bool,
        contributors: &[Contributor],
    ) -> DirectoryAssemblyMode {
        if force_map {
            self.add_directory_mode(directory, "mapping", contributors);
            return DirectoryAssemblyMode::Mapping;
        }

        if contributors.is_empty() {
            self.add_directory_mode(directory, "mapping", contributors);
            return DirectoryAssemblyMode::Mapping;
        }

        let all_numeric = contributors.iter().all(|c| is_numeric_key(&c.key));
        let any_numeric = contributors.iter().any(|c| is_numeric_key(&c.key));

        if all_numeric {
            self.add_directory_mode(directory, "sequence", contributors);
            DirectoryAssemblyMode::Sequence
        } else if any_numeric {
            let conflicting = contributors
                .iter()
                .map(|c| format!("{} ({})", c.key, self.display_path(&c.path)))
                .collect::<Vec<_>>()
                .join(", ");
            self.diag(
                Diagnostic::error(
                    "E002",
                    "mixed numeric and non-numeric children in directory",
                    Category::InvalidInput,
                )
                .with_location(self.display_path(directory))
                .with_derived_key_path(key_path.to_string())
                .with_cause(
                    "Directory sequence detection is ambiguous when contributors mix numeric and non-numeric keys.",
                )
                .with_action(
                    "Rename children so all contributors are numeric (sequence) or all are non-numeric (mapping).",
                )
                .with_context(format!("Contributors: {conflicting}")),
            );
            self.add_directory_mode(directory, "mapping (fallback after error)", contributors);
            DirectoryAssemblyMode::Mapping
        } else {
            self.add_directory_mode(directory, "mapping", contributors);
            DirectoryAssemblyMode::Mapping
        }
    }

    fn assemble_sequence(
        &mut self,
        directory: &Path,
        key_path: &str,
        contributors: Vec<Contributor>,
        excluded_file: Option<&Path>,
    ) -> Value {
        let mut numeric: Vec<(u64, Contributor)> = contributors
            .into_iter()
            .filter_map(|c| c.key.parse::<u64>().ok().map(|n| (n, c)))
            .collect();

        numeric.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.path.cmp(&b.1.path)));

        let mut expected = 0_u64;
        let mut gaps = Vec::new();

        for (index, _) in &numeric {
            if *index != expected {
                gaps.push((expected, *index));
                expected = *index;
            }
            expected += 1;
        }

        if !gaps.is_empty() {
            let gap_text = gaps
                .iter()
                .map(|(start, end)| format!("{start}..{}", end.saturating_sub(1)))
                .collect::<Vec<_>>()
                .join(", ");

            match self.options.seq_gaps {
                SeqGapMode::Error => {
                    self.diag(
                        Diagnostic::error(
                            "E003",
                            "sequence has index gaps",
                            Category::InvalidInput,
                        )
                        .with_location(self.display_path(directory))
                        .with_derived_key_path(key_path.to_string())
                        .with_cause("Sequence contributors are not contiguous.")
                        .with_action("Rename indices to form a contiguous sequence starting at 0.")
                        .with_context(format!("Missing ranges: {gap_text}")),
                    );
                }
                SeqGapMode::Warn => {
                    self.diag(
                        Diagnostic::warn("W011", "sequence has index gaps")
                            .with_location(self.display_path(directory))
                            .with_derived_key_path(key_path.to_string())
                            .with_cause("Sequence contributors are not contiguous.")
                            .with_action(
                                "Rename indices to form a contiguous sequence starting at 0.",
                            )
                            .with_context(format!("Missing ranges: {gap_text}")),
                    );
                }
                SeqGapMode::Allow => {}
            }
        }

        let mut output = Vec::new();
        for (index, contributor) in numeric {
            let child_key_path = if key_path.is_empty() {
                format!("[{index}]")
            } else {
                format!("{key_path}[{index}]")
            };
            self.add_derived_key(&contributor.path, &child_key_path);
            let value = self.load_contributor_value(&contributor, &child_key_path, excluded_file);
            output.push(value);
        }

        Value::Sequence(output)
    }

    fn assemble_mapping(
        &mut self,
        _directory: &Path,
        key_path: &str,
        contributors: Vec<Contributor>,
        excluded_file: Option<&Path>,
    ) -> Value {
        let mut map = Mapping::new();

        for contributor in contributors {
            let child_key_path = join_key_path(key_path, &contributor.key);
            self.add_derived_key(&contributor.path, &child_key_path);
            let value = self.load_contributor_value(&contributor, &child_key_path, excluded_file);
            map.insert(Value::String(contributor.key), value);
        }

        Value::Mapping(map)
    }

    fn load_contributor_value(
        &mut self,
        contributor: &Contributor,
        key_path: &str,
        excluded_file: Option<&Path>,
    ) -> Value {
        match contributor.kind {
            ContributorKind::File => self
                .parse_yaml_file(&contributor.path, key_path)
                .unwrap_or(Value::Null),
            ContributorKind::Directory => {
                self.assemble_directory(&contributor.path, key_path, false, excluded_file)
            }
        }
    }

    fn parse_yaml_file(&mut self, path: &Path, key_path: &str) -> Option<Value> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(err) => {
                self.diag(
                    Diagnostic::error(
                        "E033",
                        "unable to read file metadata",
                        Category::InvalidInput,
                    )
                    .with_location(self.display_path(path))
                    .with_cause(err.to_string())
                    .with_action("Check file permissions and retry."),
                );
                return None;
            }
        };

        if let Some(max_bytes) = self.options.max_yaml_bytes {
            if metadata.len() > max_bytes {
                self.diag(
                    Diagnostic::error(
                        "E034",
                        "YAML fragment exceeds max size",
                        Category::InvalidInput,
                    )
                    .with_location(self.display_path(path))
                    .with_derived_key_path(key_path.to_string())
                    .with_cause(format!(
                        "File size is {} bytes, which exceeds --max-yaml-bytes={max_bytes}.",
                        metadata.len()
                    ))
                    .with_action("Split the fragment or raise --max-yaml-bytes."),
                );
                return None;
            }
        }

        if metadata.len() > LARGE_FRAGMENT_WARN_BYTES {
            self.diag(
                Diagnostic::warn("W012", "large YAML fragment detected")
                    .with_location(self.display_path(path))
                    .with_derived_key_path(key_path.to_string())
                    .with_cause(format!(
                        "Fragment is {} bytes; large fragments can reduce reviewability.",
                        metadata.len()
                    ))
                    .with_action("Consider splitting this YAML into smaller FYAML fragments."),
            );
        }

        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(err) => {
                self.diag(
                    Diagnostic::error("E035", "unable to read YAML file", Category::InvalidInput)
                        .with_location(self.display_path(path))
                        .with_cause(err.to_string())
                        .with_action("Check file permissions and encoding (UTF-8 expected)."),
                );
                return None;
            }
        };

        if !self.options.preserve && (contents.contains('&') || contents.contains('*')) {
            self.diag(
                Diagnostic::warn("W013", "possible YAML anchors/aliases may not be preserved")
                    .with_location(self.display_path(path))
                    .with_derived_key_path(key_path.to_string())
                    .with_cause("Canonical mode may lose source style and anchor details.")
                    .with_action(
                        "Use --preserve if supported behavior is acceptable for your workflow.",
                    ),
            );
        }

        let mut documents = Vec::new();
        for document in serde_yaml::Deserializer::from_str(&contents) {
            match Value::deserialize(document) {
                Ok(value) => documents.push(value),
                Err(err) => {
                    let mut diag =
                        Diagnostic::error("E100", "invalid YAML fragment", Category::Parse)
                            .with_location(self.display_path(path))
                            .with_derived_key_path(key_path.to_string())
                            .with_cause(err.to_string())
                            .with_action("Fix YAML syntax (indentation, colons, and tabs/spaces).")
                            .with_context("Run `fyaml validate` for full diagnostics.".to_string());

                    if let Some(location) = err.location() {
                        diag = diag.with_context(format!(
                            "YAML parser location: line {}, column {}",
                            location.line(),
                            location.column()
                        ));
                    }

                    self.diag(diag);
                    return None;
                }
            }
        }

        if documents.len() <= 1 {
            return Some(documents.into_iter().next().unwrap_or(Value::Null));
        }

        match self.options.multi_doc {
            MultiDocMode::Error => {
                self.diag(
                    Diagnostic::error(
                        "E101",
                        "multi-document YAML is not supported in current mode",
                        Category::Parse,
                    )
                    .with_location(self.display_path(path))
                    .with_derived_key_path(key_path.to_string())
                    .with_cause("YAML input contained multiple documents separated by `---`.")
                    .with_action(
                        "Use --multi-doc=first or --multi-doc=all, or split documents into files.",
                    ),
                );
                None
            }
            MultiDocMode::First => {
                self.diag(
                    Diagnostic::warn(
                        "W014",
                        "multi-document YAML: using first document and ignoring the rest",
                    )
                    .with_location(self.display_path(path))
                    .with_derived_key_path(key_path.to_string())
                    .with_cause("Configured with --multi-doc=first.")
                    .with_action("Use --multi-doc=all to retain all documents as a sequence."),
                );
                documents.into_iter().next()
            }
            MultiDocMode::All => Some(Value::Sequence(documents)),
        }
    }

    fn detect_key_collisions(
        &mut self,
        directory: &Path,
        key_path: &str,
        contributors: &[Contributor],
    ) {
        let mut exact: HashMap<String, Vec<&Contributor>> = HashMap::new();
        let mut case_folded: HashMap<String, Vec<&Contributor>> = HashMap::new();

        for contributor in contributors {
            exact
                .entry(contributor.key.clone())
                .or_default()
                .push(contributor);
            case_folded
                .entry(contributor.key.to_lowercase())
                .or_default()
                .push(contributor);
        }

        for (key, entries) in exact {
            if entries.len() > 1 {
                let paths = entries
                    .iter()
                    .map(|entry| self.display_path(&entry.path))
                    .collect::<Vec<_>>();
                self.diag(
                    Diagnostic::error("E001", "key collision detected", Category::InvalidInput)
                        .with_location(self.display_path(directory))
                        .with_derived_key_path(join_key_path(key_path, &key))
                        .with_paths(paths.clone())
                        .with_cause("Multiple inputs resolve to the same FYAML key.")
                        .with_action("Rename one source or move it into a different directory.")
                        .with_context(format!("Sources: {}", paths.join(", "))),
                );
            }
        }

        for (_folded, entries) in case_folded {
            if entries.len() > 1 {
                let unique = entries
                    .iter()
                    .map(|entry| entry.key.as_str())
                    .collect::<HashSet<_>>();
                if unique.len() > 1 {
                    let example_key = entries.first().map(|e| e.key.clone()).unwrap_or_default();
                    let paths = entries
                        .iter()
                        .map(|entry| self.display_path(&entry.path))
                        .collect::<Vec<_>>();
                    self.diag(
                        Diagnostic::error(
                            "E004",
                            "case-only key collision detected",
                            Category::InvalidInput,
                        )
                        .with_location(self.display_path(directory))
                        .with_derived_key_path(join_key_path(key_path, &example_key))
                        .with_paths(paths.clone())
                        .with_cause(
                            "Case-insensitive filesystems can make these keys indistinguishable.",
                        )
                        .with_action("Rename keys so they are distinct even after lowercasing.")
                        .with_context(format!("Sources: {}", paths.join(", "))),
                    );
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Contributor {
    key: String,
    path: PathBuf,
    kind: ContributorKind,
}

#[derive(Debug, Clone, Copy)]
enum ContributorKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryAssemblyMode {
    Mapping,
    Sequence,
}

fn join_key_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
}

fn is_yaml_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str).map(|s| s.to_ascii_lowercase()),
        Some(ext) if ext == "yml" || ext == "yaml"
    )
}

fn strip_yaml_extension(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".yaml") {
        name[..name.len() - 5].to_string()
    } else if lower.ends_with(".yml") {
        name[..name.len() - 4].to_string()
    } else {
        name.to_string()
    }
}

fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.')
}

fn is_editor_junk(name: &str) -> bool {
    name == ".DS_Store" || name.ends_with('~')
}

fn is_numeric_key(key: &str) -> bool {
    !key.is_empty() && key.as_bytes().iter().all(|b| b.is_ascii_digit())
}

fn is_reserved_yaml_key(key: &str) -> bool {
    RESERVED_YAML_KEYS
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(key))
}

fn key_as_string(key: &Value) -> String {
    match key {
        Value::String(s) => s.clone(),
        _ => serde_yaml::to_string(key)
            .unwrap_or_else(|_| format!("{key:?}"))
            .trim()
            .to_string(),
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Sequence(_) => "sequence",
        Value::Mapping(_) => "mapping",
        Value::Tagged(_) => "tagged",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, content).expect("write file");
    }

    #[test]
    fn sequence_detection_and_ordering() {
        let dir = tempdir().expect("temp dir");
        write(&dir.path().join("2.yml"), "c\n");
        write(&dir.path().join("0.yml"), "a\n");
        write(&dir.path().join("1.yml"), "b\n");

        let options = BuildOptions::default();
        let outcome = build(dir.path(), &options);
        assert!(outcome.diagnostics.iter().all(|d| !d.is_error()));

        let root = outcome.value.expect("value exists");
        let map = root.as_mapping().expect("map root");
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn mixed_keys_are_errors() {
        let dir = tempdir().expect("temp dir");
        fs::create_dir_all(dir.path().join("items")).expect("create dir");
        write(&dir.path().join("items/0.yml"), "a\n");
        write(&dir.path().join("items/name.yml"), "b\n");

        let options = BuildOptions::default();
        let outcome = build(dir.path(), &options);
        assert!(outcome.diagnostics.iter().any(|d| d.code == "E002"));
    }

    #[test]
    fn reserved_filename_is_error_by_default() {
        let dir = tempdir().expect("temp dir");
        write(&dir.path().join("true.yml"), "x\n");

        let options = BuildOptions::default();
        let outcome = build(dir.path(), &options);
        assert!(outcome.diagnostics.iter().any(|d| d.code == "E022"));
    }

    #[test]
    fn reserved_filename_allowed_with_flag() {
        let dir = tempdir().expect("temp dir");
        write(&dir.path().join("true.yml"), "x\n");

        let options = BuildOptions {
            allow_reserved_keys: true,
            ..BuildOptions::default()
        };
        let outcome = build(dir.path(), &options);
        assert!(!outcome.diagnostics.iter().any(|d| d.code == "E022"));
    }

    #[test]
    fn key_collision_between_file_and_directory() {
        let dir = tempdir().expect("temp dir");
        write(&dir.path().join("auth.yml"), "x\n");
        write(&dir.path().join("auth/provider.yml"), "ok: true\n");

        let options = BuildOptions::default();
        let outcome = build(dir.path(), &options);
        assert!(outcome.diagnostics.iter().any(|d| d.code == "E001"));
    }
}
