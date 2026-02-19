use crate::cli::{Cli, Command, DiffArgs, ExplainArgs, PackArgs, ValidateArgs};
use crate::config::{DiffFormat, OutputFormat};
use crate::diagnostics::{Category, Diagnostic, ExitCode, Severity};
use crate::engine::{build, BuildOutcome};
use crate::scaffold;
use crate::serializer::{canonicalize_yaml, emit_json, emit_yaml};
use clap::Parser;
use serde::Serialize;
use serde_yaml::{Mapping, Value};
use std::cmp::Ordering;
use std::fs;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run_from_env() -> i32 {
    let cli = Cli::parse();
    run(cli) as i32
}

pub fn run(cli: Cli) -> ExitCode {
    match cli.command {
        Command::Pack(args) => run_pack(args),
        Command::Validate(args) => run_validate(args),
        Command::Explain(args) => run_explain(args),
        Command::Diff(args) => run_diff(args),
        Command::Scaffold(args) => run_scaffold(args),
    }
}

fn run_pack(args: PackArgs) -> ExitCode {
    let options = args.flags.to_build_options();
    let outcome = build(&args.dir, &options);

    if has_errors(&outcome.diagnostics) {
        print_diagnostics_human(&outcome.diagnostics);
        return ExitCode::from_diagnostics(&outcome.diagnostics);
    }

    print_warnings_human(&outcome.diagnostics);

    let Some(value) = outcome.value else {
        return ExitCode::Internal;
    };

    let value = if options.preserve {
        value
    } else {
        canonicalize_yaml(&value)
    };

    let rendered = match args.format {
        OutputFormat::Yaml => match emit_yaml(&value, !args.no_header, APP_VERSION) {
            Ok(output) => output,
            Err(err) => {
                let diag = Diagnostic::error(
                    "E300",
                    "unable to serialize YAML output",
                    Category::Internal,
                )
                .with_cause(err.to_string())
                .with_action("Report this issue; serialization should succeed for parsed input.");
                eprintln!("{}", diag.render_human());
                return ExitCode::Internal;
            }
        },
        OutputFormat::Json => match emit_json(&value) {
            Ok(output) => output,
            Err(err) => {
                let diag = Diagnostic::error("E301", "unable to serialize JSON output", Category::Write)
                    .with_cause(err.to_string())
                    .with_action(
                        "Ensure YAML mapping keys are JSON-compatible strings when using --format json.",
                    );
                eprintln!("{}", diag.render_human());
                return ExitCode::WriteError;
            }
        },
    };

    if let Some(output_path) = args.output {
        if let Err(err) = fs::write(&output_path, rendered) {
            let diag = Diagnostic::error("E302", "unable to write output file", Category::Write)
                .with_location(output_path.display().to_string())
                .with_cause(err.to_string())
                .with_action("Check path permissions and available disk space.");
            eprintln!("{}", diag.render_human());
            return ExitCode::WriteError;
        }
    } else {
        print!("{rendered}");
    }

    ExitCode::Success
}

fn run_validate(args: ValidateArgs) -> ExitCode {
    let options = args.flags.to_build_options();
    let outcome = build(&args.dir, &options);

    if args.json {
        print_diagnostics_json(&outcome.diagnostics);
    } else {
        print_diagnostics_human(&outcome.diagnostics);
    }

    if has_errors(&outcome.diagnostics) {
        ExitCode::from_diagnostics(&outcome.diagnostics)
    } else {
        ExitCode::Success
    }
}

fn run_explain(args: ExplainArgs) -> ExitCode {
    let options = args.flags.to_build_options();
    let outcome = build(&args.dir, &options);

    if args.json {
        #[derive(Serialize)]
        struct ExplainJson<'a> {
            diagnostics: &'a [Diagnostic],
            explain: &'a crate::engine::ExplainReport,
        }

        let payload = ExplainJson {
            diagnostics: &outcome.diagnostics,
            explain: &outcome.explain,
        };

        match serde_json::to_string_pretty(&payload) {
            Ok(json) => println!("{json}"),
            Err(err) => {
                let diag =
                    Diagnostic::error("E303", "unable to render explain JSON", Category::Internal)
                        .with_cause(err.to_string())
                        .with_action("Report this issue; JSON serialization should succeed.");
                eprintln!("{}", diag.render_human());
                return ExitCode::Internal;
            }
        }
    } else {
        print_explain_human(&outcome);
    }

    if has_errors(&outcome.diagnostics) {
        ExitCode::from_diagnostics(&outcome.diagnostics)
    } else {
        ExitCode::Success
    }
}

fn run_diff(args: DiffArgs) -> ExitCode {
    let options = args.flags.to_build_options();

    let left = build(&args.dir_a, &options);
    let right = build(&args.dir_b, &options);

    let mut diagnostics = left.diagnostics.clone();
    diagnostics.extend(right.diagnostics.clone());

    if has_errors(&diagnostics) {
        match args.format {
            DiffFormat::Path => print_diagnostics_human(&diagnostics),
            DiffFormat::Json => print_diagnostics_json(&diagnostics),
        }
        return ExitCode::from_diagnostics(&diagnostics);
    }

    let left_value = canonicalize_yaml(&left.value.unwrap_or(Value::Null));
    let right_value = canonicalize_yaml(&right.value.unwrap_or(Value::Null));

    let diff = first_difference(&left_value, &right_value, "$".to_string());

    match diff {
        None => {
            match args.format {
                DiffFormat::Path => println!("equal"),
                DiffFormat::Json => println!("{{\"equal\":true}}"),
            }
            ExitCode::Success
        }
        Some((path, reason)) => {
            match args.format {
                DiffFormat::Path => {
                    println!("different at {path}: {reason}");
                }
                DiffFormat::Json => {
                    let payload = serde_json::json!({
                        "equal": false,
                        "first_difference_path": path,
                        "reason": reason
                    });
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&payload)
                            .unwrap_or_else(|_| payload.to_string())
                    );
                }
            }
            ExitCode::InvalidInput
        }
    }
}

fn run_scaffold(args: crate::cli::ScaffoldArgs) -> ExitCode {
    let outcome = scaffold::scaffold(&args.input, &args.dir, &args.to_options());

    for diagnostic in &outcome.diagnostics {
        match diagnostic.severity {
            Severity::Error | Severity::Warn => eprintln!("{}", diagnostic.render_human()),
            Severity::Info => println!("{}", diagnostic.render_human()),
        }
    }

    if has_errors(&outcome.diagnostics) {
        ExitCode::from_diagnostics(&outcome.diagnostics)
    } else {
        ExitCode::Success
    }
}

fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(Diagnostic::is_error)
}

fn print_diagnostics_human(diags: &[Diagnostic]) {
    if diags.is_empty() {
        println!("no diagnostics");
        return;
    }

    for diagnostic in diags {
        match diagnostic.severity {
            Severity::Error | Severity::Warn => eprintln!("{}", diagnostic.render_human()),
            Severity::Info => println!("{}", diagnostic.render_human()),
        }
    }
}

fn print_warnings_human(diags: &[Diagnostic]) {
    for diagnostic in diags {
        if diagnostic.severity == Severity::Warn {
            eprintln!("{}", diagnostic.render_human());
        }
    }
}

fn print_diagnostics_json(diags: &[Diagnostic]) {
    match serde_json::to_string_pretty(diags) {
        Ok(json) => println!("{json}"),
        Err(err) => {
            let diag = Diagnostic::error(
                "E304",
                "unable to render diagnostics JSON",
                Category::Internal,
            )
            .with_cause(err.to_string())
            .with_action("Report this issue; JSON serialization should succeed.");
            eprintln!("{}", diag.render_human());
        }
    }
}

fn print_explain_human(outcome: &BuildOutcome) {
    println!("Derived Key Tree:");
    if outcome.explain.derived_keys.is_empty() {
        println!("  (none)");
    } else {
        for entry in &outcome.explain.derived_keys {
            println!("  {} <- {}", entry.derived_key_path, entry.source);
        }
    }

    println!("\nDirectory Decisions:");
    if outcome.explain.directory_modes.is_empty() {
        println!("  (none)");
    } else {
        for decision in &outcome.explain.directory_modes {
            println!("  {} => {}", decision.directory, decision.mode);
            if !decision.contributors.is_empty() {
                println!("    contributors: {}", decision.contributors.join(", "));
            }
        }
    }

    println!("\nIgnored Entries:");
    if outcome.explain.ignored.is_empty() {
        println!("  (none)");
    } else {
        for ignored in &outcome.explain.ignored {
            println!("  {} ({})", ignored.path, ignored.rule);
        }
    }

    println!("\nDiagnostics:");
    if outcome.diagnostics.is_empty() {
        println!("  no diagnostics");
    } else {
        for diagnostic in &outcome.diagnostics {
            print!("{}", diagnostic.render_human());
        }
    }
}

fn first_difference(left: &Value, right: &Value, path: String) -> Option<(String, String)> {
    match (left, right) {
        (Value::Null, Value::Null)
        | (Value::Bool(_), Value::Bool(_))
        | (Value::Number(_), Value::Number(_))
        | (Value::String(_), Value::String(_)) => {
            if left == right {
                None
            } else {
                Some((path, "scalar value differs".to_string()))
            }
        }
        (Value::Sequence(a), Value::Sequence(b)) => {
            if a.len() != b.len() {
                return Some((
                    path,
                    format!("sequence length differs ({} vs {})", a.len(), b.len()),
                ));
            }

            for (index, (left_item, right_item)) in a.iter().zip(b.iter()).enumerate() {
                let child_path = format!("{path}[{index}]");
                if let Some(diff) = first_difference(left_item, right_item, child_path) {
                    return Some(diff);
                }
            }

            None
        }
        (Value::Mapping(a), Value::Mapping(b)) => first_map_difference(a, b, path),
        (Value::Tagged(a), Value::Tagged(b)) => first_difference(&a.value, &b.value, path),
        _ => Some((path, "value type differs".to_string())),
    }
}

fn first_map_difference(left: &Mapping, right: &Mapping, path: String) -> Option<(String, String)> {
    let mut left_keys: Vec<&Value> = left.keys().collect();
    let mut right_keys: Vec<&Value> = right.keys().collect();

    left_keys.sort_by(|a, b| compare_yaml_key(a, b));
    right_keys.sort_by(|a, b| compare_yaml_key(a, b));

    for key in &left_keys {
        if !right.contains_key(*key) {
            let key_text = yaml_key_text(key);
            return Some((
                path.clone(),
                format!("key missing on right side: {key_text}"),
            ));
        }
    }

    for key in &right_keys {
        if !left.contains_key(*key) {
            let key_text = yaml_key_text(key);
            return Some((
                path.clone(),
                format!("key missing on left side: {key_text}"),
            ));
        }
    }

    for key in left_keys {
        let left_value = left.get(key).expect("left key exists");
        let right_value = right.get(key).expect("right key exists");
        let next_path = if path == "$" {
            format!("$.{}", yaml_key_text(key))
        } else {
            format!("{}.{}", path, yaml_key_text(key))
        };

        if let Some(diff) = first_difference(left_value, right_value, next_path) {
            return Some(diff);
        }
    }

    None
}

fn compare_yaml_key(a: &Value, b: &Value) -> Ordering {
    yaml_sort_key(a).cmp(&yaml_sort_key(b))
}

fn yaml_sort_key(value: &Value) -> Vec<u8> {
    match value {
        Value::String(s) => s.as_bytes().to_vec(),
        _ => serde_yaml::to_string(value)
            .unwrap_or_else(|_| format!("{value:?}"))
            .into_bytes(),
    }
}

fn yaml_key_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        _ => serde_yaml::to_string(value)
            .unwrap_or_else(|_| format!("{value:?}"))
            .trim()
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_difference_finds_nested_path() {
        let left: Value = serde_yaml::from_str("a:\n  b: 1\n").expect("left parse");
        let right: Value = serde_yaml::from_str("a:\n  b: 2\n").expect("right parse");

        let diff = first_difference(&left, &right, "$".to_string()).expect("difference exists");
        assert_eq!(diff.0, "$.a.b");
    }
}
