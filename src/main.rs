use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;
use upgradelab::report::{self, Report};
use upgradelab::scenario::Scenario;

/// Executable Soroban migration rehearsal.
///
/// Exit codes: 0 every invariant passed; 1 at least one invariant failed (or a replay
/// differed); 2 invalid input (bad scenario, bad report, unsafe path, usage);
/// 3 inconclusive (at least one invariant could not be evaluated and none failed);
/// 4 environment error (cannot read a WASM, CLI missing, network failure).
#[derive(Parser)]
#[command(name = "upgradelab", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum SchemaKind {
    Report,
    Scenario,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a scenario against the compiled old and new WASM in the in-process Soroban host.
    Run {
        scenario: PathBuf,
        /// Directory that the scenario's relative WASM paths are resolved against.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
        /// Write the JSON report to this file (in addition to the chosen stdout format).
        #[arg(long)]
        out: Option<PathBuf>,
        /// A `cargo test` log of native SDK tests to attach as a separate category.
        #[arg(long)]
        native_log: Option<PathBuf>,
    },
    /// Re-run the scenario embedded in a report and compare the result byte for byte.
    Replay {
        report: PathBuf,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        native_log: Option<PathBuf>,
    },
    /// Check that a scenario file parses and is internally consistent.
    Validate { scenario: PathBuf },
    /// Check that a report file parses as report version 1.
    CheckReport { report: PathBuf },
    /// Print a JSON Schema.
    Schema {
        #[arg(value_enum)]
        kind: SchemaKind,
    },
    /// Opt-in: run the scenario on Stellar testnet through the Stellar CLI.
    Testnet(upgradelab::testnet::TestnetArgs),
}

fn load_scenario(p: &PathBuf) -> Result<Scenario, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
    Scenario::parse(&text).map_err(|e| e.0)
}

fn load_report(p: &PathBuf) -> Result<Report, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
    let r: Report = serde_json::from_str(&text).map_err(|e| format!("report JSON: {e}"))?;
    if r.report_version != report::REPORT_VERSION || r.kind != report::REPORT_KIND {
        return Err(format!("unsupported report (version {}, kind {})", r.report_version, r.kind));
    }
    Ok(r)
}

fn emit(r: &Report, format: Format, out: Option<&PathBuf>) -> Result<(), String> {
    if let Some(p) = out {
        std::fs::write(p, r.to_json()).map_err(|e| format!("cannot write {}: {e}", p.display()))?;
    }
    match format {
        Format::Text => print!("{}", upgradelab::text::render(r)),
        Format::Json => print!("{}", r.to_json()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.cmd {
        Cmd::Validate { scenario } => match load_scenario(&scenario) {
            Ok(s) => {
                println!(
                    "ok: scenario `{}` ({} ops, {} probes, {} invariants)",
                    s.name,
                    s.ops.len(),
                    s.probes.len(),
                    s.invariants.len()
                );
                0
            }
            Err(e) => {
                eprintln!("invalid scenario: {e}");
                2
            }
        },
        Cmd::CheckReport { report } => match load_report(&report) {
            Ok(r) => {
                println!(
                    "ok: report v{} for `{}`, verdict {}",
                    r.report_version,
                    r.scenario.name,
                    r.verdict.status.as_str()
                );
                0
            }
            Err(e) => {
                eprintln!("invalid report: {e}");
                2
            }
        },
        Cmd::Schema { kind } => {
            match kind {
                SchemaKind::Report => print!("{}", report::schema_json()),
                SchemaKind::Scenario => print!("{}", report::scenario_schema_json()),
            }
            0
        }
        Cmd::Run { scenario, root, format, out, native_log } => {
            let s = match load_scenario(&scenario) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("invalid scenario: {e}");
                    return ExitCode::from(2);
                }
            };
            match upgradelab::run_host(&s, &root, native_log.as_deref()) {
                Ok(r) => match emit(&r, format, out.as_ref()) {
                    Ok(()) => r.exit_code(),
                    Err(e) => {
                        eprintln!("{e}");
                        4
                    }
                },
                Err(e) => {
                    eprintln!("error: {e}");
                    4
                }
            }
        }
        Cmd::Replay { report, root, native_log } => {
            let r = match load_report(&report) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("invalid report: {e}");
                    return ExitCode::from(2);
                }
            };
            match upgradelab::replay(&r, &root, native_log.as_deref()) {
                Ok((fresh, diffs)) => {
                    let identical = diffs.is_empty() && fresh.to_json() == r.to_json();
                    if identical {
                        println!(
                            "REPRODUCED: re-running `{}` produced an identical report (verdict {}).",
                            r.scenario.name,
                            r.verdict.status.as_str()
                        );
                        0
                    } else {
                        println!("DIFFERENT: the re-run does not match the report ({} difference(s)):", diffs.len());
                        for d in diffs.iter().take(40) {
                            println!("  {d}");
                        }
                        1
                    }
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    4
                }
            }
        }
        Cmd::Testnet(args) => upgradelab::testnet::run_cli(args),
    };
    ExitCode::from(code as u8)
}
