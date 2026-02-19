use crate::diagnostics::{Category, Diagnostic};
use serde::Deserialize;
use serde::Serialize;
use serde_yaml::Value;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, clap::ValueEnum, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ScaffoldLayout {
    Flat,
    Nested,
    Hybrid,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SequenceLayout {
    Dir,
    Files,
}

#[derive(Debug, Clone)]
pub struct ScaffoldOptions {
    pub layout: ScaffoldLayout,
    pub seq: SequenceLayout,
    pub split_threshold_bytes: Option<usize>,
}

impl Default for ScaffoldOptions {
    fn default() -> Self {
        Self {
            layout: ScaffoldLayout::Hybrid,
            seq: SequenceLayout::Files,
            split_threshold_bytes: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScaffoldOutcome {
    pub diagnostics: Vec<Diagnostic>,
}

pub fn scaffold(input_file: &Path, output_dir: &Path, options: &ScaffoldOptions) -> ScaffoldOutcome {
    let mut diagnostics = Vec::new();

    let contents = match fs::read_to_string(input_file) {
        Ok(contents) => contents,
        Err(err) => {
            diagnostics.push(
                Diagnostic::error("E200", "unable to read scaffold input file", Category::InvalidInput)
                    .with_location(input_file.display().to_string())
                    .with_cause(err.to_string())
                    .with_action("Pass a readable YAML file to `fyaml scaffold`."),
            );
            return ScaffoldOutcome { diagnostics };
        }
    };

    let mut docs = Vec::new();
    for document in serde_yaml::Deserializer::from_str(&contents) {
        match Value::deserialize(document) {
            Ok(value) => docs.push(value),
            Err(err) => {
                diagnostics.push(
                    Diagnostic::error("E201", "invalid YAML in scaffold input", Category::Parse)
                        .with_location(input_file.display().to_string())
                        .with_cause(err.to_string())
                        .with_action("Fix YAML syntax before scaffolding."),
                );
                return ScaffoldOutcome { diagnostics };
            }
        }
    }

    if docs.len() > 1 {
        diagnostics.push(
            Diagnostic::error(
                "E202",
                "scaffold input must be a single YAML document",
                Category::Parse,
            )
            .with_location(input_file.display().to_string())
            .with_cause("Multiple documents were found in scaffold input.")
            .with_action("Provide a single YAML document for deterministic scaffold output."),
        );
        return ScaffoldOutcome { diagnostics };
    }

    let value = docs.into_iter().next().unwrap_or(Value::Null);

    if let Err(err) = fs::create_dir_all(output_dir) {
        diagnostics.push(
            Diagnostic::error("E203", "unable to create scaffold output directory", Category::Write)
                .with_location(output_dir.display().to_string())
                .with_cause(err.to_string())
                .with_action("Check write permissions for the output path."),
        );
        return ScaffoldOutcome { diagnostics };
    }

    if let Err(diagnostic) = write_value(None, &value, output_dir, options) {
        diagnostics.push(diagnostic);
    }

    diagnostics.push(
        Diagnostic::info(
            "I200",
            "scaffold generated a deterministic FYAML layout (non-invertible helper)",
        )
        .with_location(output_dir.display().to_string())
        .with_cause("Scaffold is intentionally one-way and not a reverse of pack.")
        .with_action("Validate with `fyaml pack <DIR>` and compare semantic output in CI."),
    );

    ScaffoldOutcome { diagnostics }
}

fn write_value(
    key: Option<&str>,
    value: &Value,
    directory: &Path,
    options: &ScaffoldOptions,
) -> Result<(), Diagnostic> {
    match value {
        Value::Mapping(map) => write_mapping(key, map, directory, options),
        Value::Sequence(sequence) => write_sequence(key, sequence, directory, options),
        _ => write_scalar_file(key.unwrap_or("root"), value, directory, options),
    }
}

fn write_mapping(
    key: Option<&str>,
    map: &serde_yaml::Mapping,
    directory: &Path,
    options: &ScaffoldOptions,
) -> Result<(), Diagnostic> {
    let target_directory = if let Some(key) = key {
        let key = normalize_path_key(key)?;
        let next = directory.join(key);
        fs::create_dir_all(&next).map_err(|err| {
            Diagnostic::error("E204", "unable to create mapping directory", Category::Write)
                .with_location(next.display().to_string())
                .with_cause(err.to_string())
                .with_action("Check write permissions and path validity.")
        })?;
        next
    } else {
        directory.to_path_buf()
    };

    let mut entries: Vec<(String, &Value)> = map
        .iter()
        .map(|(key, value)| {
            let key = key.as_str().ok_or_else(|| {
                Diagnostic::error(
                    "E205",
                    "non-string YAML mapping keys are unsupported for scaffold",
                    Category::InvalidInput,
                )
                .with_cause("Filesystem entries require string-like path names.")
                .with_action("Convert mapping keys to strings before running scaffold.")
            })?;
            Ok((key.to_string(), value))
        })
        .collect::<Result<Vec<_>, _>>()?;

    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    for (child_key, child_value) in entries {
        match child_value {
            Value::Mapping(_) => {
                let as_file = matches!(options.layout, ScaffoldLayout::Flat);
                if as_file {
                    write_scalar_file(&child_key, child_value, &target_directory, options)?;
                } else {
                    write_mapping(Some(&child_key), child_value.as_mapping().expect("mapping"), &target_directory, options)?;
                }
            }
            Value::Sequence(_) => {
                let as_file = matches!(options.layout, ScaffoldLayout::Flat);
                if as_file {
                    write_scalar_file(&child_key, child_value, &target_directory, options)?;
                } else {
                    write_sequence(Some(&child_key), child_value.as_sequence().expect("sequence"), &target_directory, options)?;
                }
            }
            _ => write_scalar_file(&child_key, child_value, &target_directory, options)?,
        }
    }

    Ok(())
}

fn write_sequence(
    key: Option<&str>,
    sequence: &[Value],
    directory: &Path,
    options: &ScaffoldOptions,
) -> Result<(), Diagnostic> {
    let base_directory = if let Some(key) = key {
        let key = normalize_path_key(key)?;
        let next = directory.join(key);
        fs::create_dir_all(&next).map_err(|err| {
            Diagnostic::error("E206", "unable to create sequence directory", Category::Write)
                .with_location(next.display().to_string())
                .with_cause(err.to_string())
                .with_action("Check write permissions and path validity.")
        })?;
        next
    } else {
        directory.to_path_buf()
    };

    for (index, item) in sequence.iter().enumerate() {
        let key = index.to_string();
        match options.seq {
            SequenceLayout::Files => write_scalar_file(&key, item, &base_directory, options)?,
            SequenceLayout::Dir => {
                let item_dir = base_directory.join(&key);
                fs::create_dir_all(&item_dir).map_err(|err| {
                    Diagnostic::error("E207", "unable to create sequence item directory", Category::Write)
                        .with_location(item_dir.display().to_string())
                        .with_cause(err.to_string())
                        .with_action("Check write permissions and path validity.")
                })?;

                match item {
                    Value::Mapping(map) => write_mapping(None, map, &item_dir, options)?,
                    Value::Sequence(seq) => write_sequence(None, seq, &item_dir, options)?,
                    _ => write_scalar_file("value", item, &item_dir, options)?,
                }
            }
        }
    }

    Ok(())
}

fn write_scalar_file(
    key: &str,
    value: &Value,
    directory: &Path,
    options: &ScaffoldOptions,
) -> Result<(), Diagnostic> {
    let key = normalize_path_key(key)?;
    let output_path = directory.join(format!("{key}.yml"));

    let yaml = serde_yaml::to_string(value).map_err(|err| {
        Diagnostic::error("E208", "unable to serialize YAML fragment", Category::Internal)
            .with_location(output_path.display().to_string())
            .with_cause(err.to_string())
            .with_action("Report this issue; YAML serialization should succeed for parsed input.")
    })?;

    if let Some(threshold) = options.split_threshold_bytes {
        if yaml.len() > threshold && matches!(value, Value::String(_)) {
            let nested_path = directory.join(&key);
            fs::create_dir_all(&nested_path).map_err(|err| {
                Diagnostic::error("E209", "unable to create split directory", Category::Write)
                    .with_location(nested_path.display().to_string())
                    .with_cause(err.to_string())
                    .with_action("Check write permissions and path validity.")
            })?;
            let fallback = nested_path.join("value.yml");
            fs::write(&fallback, yaml).map_err(|err| {
                Diagnostic::error("E210", "unable to write split YAML fragment", Category::Write)
                    .with_location(fallback.display().to_string())
                    .with_cause(err.to_string())
                    .with_action("Check write permissions and available disk space.")
            })?;
            return Ok(());
        }
    }

    fs::write(&output_path, yaml).map_err(|err| {
        Diagnostic::error("E211", "unable to write YAML fragment", Category::Write)
            .with_location(output_path.display().to_string())
            .with_cause(err.to_string())
            .with_action("Check write permissions and available disk space.")
    })?;

    Ok(())
}

fn normalize_path_key(key: &str) -> Result<String, Diagnostic> {
    if key.contains('/') || key.contains('\\') {
        return Err(
            Diagnostic::error(
                "E212",
                "mapping key contains path separators and cannot be scaffolded",
                Category::InvalidInput,
            )
            .with_cause("The scaffold layout maps keys to filesystem paths.")
            .with_action("Rename keys to avoid `/` or `\\`, or scaffold manually."),
        );
    }

    if key.is_empty() {
        return Err(
            Diagnostic::error("E213", "empty mapping key cannot be scaffolded", Category::InvalidInput)
                .with_cause("Filesystem entries require non-empty names.")
                .with_action("Ensure all mapping keys are non-empty strings."),
        );
    }

    Ok(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scaffold_creates_files_for_simple_map() {
        let dir = tempdir().expect("temp dir");
        let input = dir.path().join("input.yml");
        fs::write(&input, "a: 1\nb: true\n").expect("write input");

        let out = dir.path().join("out");
        let outcome = scaffold(&input, &out, &ScaffoldOptions::default());

        assert!(outcome.diagnostics.iter().all(|d| !d.is_error()));
        assert!(out.join("a.yml").exists());
        assert!(out.join("b.yml").exists());
    }
}
