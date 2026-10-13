//! Offline KCL checking and the versioned diagnostics output.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use kcl_lib::{
    ExecutorSettings, SourceRange, TypedPath,
    check::{CheckDiagnostic, CheckOutcome, check_file, check_source},
    kcl_error::{Severity, Tag},
};
use serde::Serialize;

/// Check KCL offline using mock execution, without a Zoo API token.
///
/// This reports language errors and warnings, but does not verify authoritative
/// CAD geometry. Project settings are loaded from the nearest project.toml.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclCheck {
    /// KCL file, project directory, or '-' to read source from stdin.
    #[clap(default_value = "main.kcl")]
    input: PathBuf,
    /// Emit a versioned JSON diagnostics report on stdout.
    #[clap(long)]
    json: bool,
    /// Return a failing exit status for warnings as well as errors.
    #[clap(long)]
    deny_warnings: bool,
}

#[derive(Serialize)]
struct CheckReport {
    schema_version: u32,
    ok: bool,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize)]
struct Diagnostic {
    severity: &'static str,
    code: Option<String>,
    message: String,
    locations: Vec<Location>,
    suggestion: Option<Suggestion>,
}

#[derive(Serialize)]
struct Location {
    file: Option<String>,
    start: Position,
    end: Position,
}

#[derive(Serialize)]
struct Position {
    byte: usize,
    line: Option<usize>,
    column: Option<usize>,
}

#[derive(Serialize)]
struct Suggestion {
    title: String,
    insert: String,
    location: Location,
}

fn position(source: Option<&str>, byte: usize) -> Position {
    let prefix = source.and_then(|source| source.get(..byte));
    Position {
        byte,
        line: prefix.map(|prefix| prefix.bytes().filter(|byte| *byte == b'\n').count() + 1),
        column: prefix.map(|prefix| prefix.rsplit('\n').next().unwrap_or_default().chars().count() + 1),
    }
}

fn location(outcome: &CheckOutcome, range: SourceRange, stdin: bool) -> Location {
    let source = outcome.source_files.get(&range.module_id());
    Location {
        file: source.map(|source| {
            if stdin && range.module_id().is_top_level() {
                "-".to_owned()
            } else {
                source.path.to_string()
            }
        }),
        start: position(source.map(|source| source.source.as_str()), range.start()),
        end: position(source.map(|source| source.source.as_str()), range.end()),
    }
}

fn report(outcome: &CheckOutcome, deny_warnings: bool, stdin: bool) -> CheckReport {
    CheckReport {
        schema_version: 1,
        ok: !outcome.has_errors() && !(deny_warnings && outcome.has_warnings()),
        diagnostics: outcome
            .diagnostics
            .iter()
            .map(|diagnostic| {
                let (code, suggestion) = match diagnostic {
                    CheckDiagnostic::Issue(issue) => {
                        let code = match issue.tag {
                            Tag::Deprecated => Some("deprecated"),
                            Tag::Unnecessary => Some("unnecessary"),
                            Tag::UnknownNumericUnits => Some("unknown-numeric-units"),
                            Tag::None => None,
                        }
                        .map(str::to_owned);
                        let suggestion = issue.suggestion.as_ref().map(|suggestion| Suggestion {
                            title: suggestion.title.clone(),
                            insert: suggestion.insert.clone(),
                            location: location(outcome, suggestion.source_range, stdin),
                        });
                        (code, suggestion)
                    }
                    CheckDiagnostic::Error(error) => (Some(error.error_type().to_owned()), None),
                };
                Diagnostic {
                    severity: match diagnostic.severity() {
                        Severity::Warning => "warning",
                        Severity::Error | Severity::Fatal => "error",
                    },
                    code,
                    message: diagnostic.message(),
                    locations: diagnostic
                        .source_ranges()
                        .into_iter()
                        .map(|range| location(outcome, range, stdin))
                        .collect(),
                    suggestion,
                }
            })
            .collect(),
    }
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclCheck {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let result = async {
            let (source, path) = ctx.get_code_and_file_path(&self.input).await?;
            let stdin = path == std::path::Path::new("-");
            let outcome = if stdin {
                // Stdin has no actual entry-point file. Imports use cwd; project
                // settings cannot be inferred from an input path.
                check_source(
                    &source,
                    ExecutorSettings {
                        project_directory: Some(TypedPath(std::env::current_dir()?)),
                        ..Default::default()
                    },
                )
                .await
            } else {
                check_file(&path).await?
            };
            Ok::<_, anyhow::Error>(report(&outcome, self.deny_warnings, stdin))
        }
        .await;
        let report = match result {
            Ok(report) => report,
            Err(error) => CheckReport {
                schema_version: 1,
                ok: false,
                diagnostics: vec![Diagnostic {
                    severity: "error",
                    code: Some("input".into()),
                    message: format!("{error:#}"),
                    locations: Vec::new(),
                    suggestion: None,
                }],
            },
        };
        if self.json {
            serde_json::to_writer(&mut ctx.io.out, &report)?;
            writeln!(ctx.io.out)?;
        } else {
            for diagnostic in &report.diagnostics {
                if let Some(location) = diagnostic.locations.first() {
                    let file = location.file.as_deref().unwrap_or("<unknown>");
                    write!(ctx.io.err_out, "{file}")?;
                    if let (Some(line), Some(column)) = (location.start.line, location.start.column) {
                        write!(ctx.io.err_out, ":{line}:{column}")?;
                    }
                    write!(ctx.io.err_out, ": ")?;
                }
                writeln!(ctx.io.err_out, "{}: {}", diagnostic.severity, diagnostic.message)?;
                if let Some(suggestion) = &diagnostic.suggestion {
                    writeln!(ctx.io.err_out, "  help: {}", suggestion.title)?;
                }
            }
            if report.ok {
                writeln!(ctx.io.out, "Offline KCL check passed.")?;
            }
        }
        if !report.ok {
            anyhow::bail!("Offline KCL check failed.");
        }
        Ok(())
    }
}
