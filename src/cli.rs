use crate::config::{BuildOptions, DiffFormat, MultiDocMode, OutputFormat, RootMode, SeqGapMode};
use crate::scaffold::{ScaffoldLayout, ScaffoldOptions, SequenceLayout};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "fyaml",
    version,
    about = "FYAML (filesystem-backed YAML) packer",
    long_about = "FYAML packs a directory tree of YAML fragments into one deterministic YAML document.\n\nFYAML packing is one-way; directory layout is not recoverable from the packed YAML."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Pack a FYAML directory into a single document
    Pack(PackArgs),
    /// Validate that a FYAML directory packs under the configured rules
    Validate(ValidateArgs),
    /// Explain derived keys, ignored files, and sequence/mapping decisions
    Explain(ExplainArgs),
    /// Compare two FYAML directories by packed semantics
    Diff(DiffArgs),
    /// Generate a FYAML-friendly starter layout from YAML (non-invertible helper)
    Scaffold(ScaffoldArgs),
}

#[derive(Debug, Args)]
pub struct PackArgs {
    /// Input directory
    pub dir: PathBuf,

    /// Output file path (defaults to stdout)
    #[arg(short = 'o')]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(long, default_value_t = OutputFormat::Yaml)]
    pub format: OutputFormat,

    /// Suppress the default version header comment
    #[arg(long)]
    pub no_header: bool,

    #[command(flatten)]
    pub flags: BuildFlags,
}

#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Input directory
    pub dir: PathBuf,

    /// Emit machine-readable diagnostics as JSON
    #[arg(long)]
    pub json: bool,

    #[command(flatten)]
    pub flags: BuildFlags,
}

#[derive(Debug, Args)]
pub struct ExplainArgs {
    /// Input directory
    pub dir: PathBuf,

    /// Emit machine-readable diagnostics and explain report as JSON
    #[arg(long)]
    pub json: bool,

    #[command(flatten)]
    pub flags: BuildFlags,
}

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// First FYAML directory
    pub dir_a: PathBuf,

    /// Second FYAML directory
    pub dir_b: PathBuf,

    /// Diff output format
    #[arg(long, default_value_t = DiffFormat::Path)]
    pub format: DiffFormat,

    #[command(flatten)]
    pub flags: BuildFlags,
}

#[derive(Debug, Args)]
pub struct ScaffoldArgs {
    /// Input YAML file
    pub input: PathBuf,

    /// Output directory for generated FYAML layout
    pub dir: PathBuf,

    /// Layout strategy (deterministic helper, not invertible)
    #[arg(long, default_value_t = ScaffoldLayout::Hybrid)]
    pub layout: ScaffoldLayout,

    /// Sequence representation in generated layout
    #[arg(long, default_value_t = SequenceLayout::Files)]
    pub seq: SequenceLayout,

    /// Optional split threshold for large scalar fragments
    #[arg(long)]
    pub split_threshold_bytes: Option<usize>,
}

impl ScaffoldArgs {
    pub fn to_options(&self) -> ScaffoldOptions {
        ScaffoldOptions {
            layout: self.layout,
            seq: self.seq,
            split_threshold_bytes: self.split_threshold_bytes,
        }
    }
}

#[derive(Debug, Clone, Args)]
pub struct BuildFlags {
    /// Root construction mode
    #[arg(long, default_value_t = RootMode::MapRoot)]
    pub root_mode: RootMode,

    /// Root file path (required with --root-mode file-root)
    #[arg(long)]
    pub root_file: Option<PathBuf>,

    /// Merge packed directory mapping under this key in file-root mode
    #[arg(long)]
    pub merge_under: Option<String>,

    /// Include hidden files/directories
    #[arg(long)]
    pub include_hidden: bool,

    /// Sequence gap handling
    #[arg(long, default_value_t = SeqGapMode::Warn)]
    pub seq_gaps: SeqGapMode,

    /// Multi-document YAML handling
    #[arg(long, default_value_t = MultiDocMode::Error)]
    pub multi_doc: MultiDocMode,

    /// Suppress warnings for dotted keys derived from filenames
    #[arg(long)]
    pub allow_dotted_keys: bool,

    /// Allow YAML reserved words as keys
    #[arg(long)]
    pub allow_reserved_keys: bool,

    /// Attempt to preserve source order/styles where possible
    #[arg(long)]
    pub preserve: bool,

    /// Promote warnings to errors
    #[arg(long)]
    pub strict: bool,

    /// Maximum YAML bytes allowed per input file
    #[arg(long)]
    pub max_yaml_bytes: Option<u64>,
}

impl BuildFlags {
    pub fn to_build_options(&self) -> BuildOptions {
        BuildOptions {
            include_hidden: self.include_hidden,
            allow_dotted_keys: self.allow_dotted_keys,
            allow_reserved_keys: self.allow_reserved_keys,
            seq_gaps: self.seq_gaps,
            multi_doc: self.multi_doc,
            strict: self.strict,
            max_yaml_bytes: self.max_yaml_bytes,
            root_mode: self.root_mode,
            root_file: self.root_file.clone(),
            merge_under: self.merge_under.clone(),
            preserve: self.preserve,
        }
    }
}
