use clap::ValueEnum;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RootMode {
    MapRoot,
    SeqRoot,
    FileRoot,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SeqGapMode {
    Error,
    Warn,
    Allow,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MultiDocMode {
    Error,
    First,
    All,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    Yaml,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DiffFormat {
    Path,
    Json,
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub include_hidden: bool,
    pub allow_dotted_keys: bool,
    pub allow_reserved_keys: bool,
    pub seq_gaps: SeqGapMode,
    pub multi_doc: MultiDocMode,
    pub strict: bool,
    pub max_yaml_bytes: Option<u64>,
    pub root_mode: RootMode,
    pub root_file: Option<PathBuf>,
    pub merge_under: Option<String>,
    pub preserve: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            allow_dotted_keys: false,
            allow_reserved_keys: false,
            seq_gaps: SeqGapMode::Warn,
            multi_doc: MultiDocMode::Error,
            strict: false,
            max_yaml_bytes: None,
            root_mode: RootMode::MapRoot,
            root_file: None,
            merge_under: None,
            preserve: false,
        }
    }
}
