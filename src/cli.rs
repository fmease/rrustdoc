//! The command-line interface.

use crate::data::{CrateNameBuf, CrateType, Edition};
use clap::{ColorChoice, Parser};
use joinery::JoinableIterator;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about)]
pub(crate) struct Arguments {
    /// Path to the source file.
    pub(crate) path: PathBuf,
    /// Flags passed to `rustc` and `rustdoc` verbatim.
    #[arg(last(true), value_name("VERBATIM"))]
    pub(crate) verbatim_flags: Vec<String>,
    /// Open the generated docs in a browser.
    #[arg(short, long)]
    pub(crate) open: bool,
    /// Set the name of the (base) crate.
    #[arg(short = 'n', long, value_name("NAME"), value_parser = CrateNameBuf::parse_cli_style)]
    pub(crate) crate_name: Option<CrateNameBuf>,
    /// Set the type of the (base) crate.
    #[arg(short = 'y', long, value_name("TYPE"), value_parser = CrateType::parse_cli_style)]
    pub(crate) crate_type: Option<CrateType>,
    /// Set the edition of the source files.
    #[arg(short, long, value_parser = Edition::parse_cli_style)]
    pub(crate) edition: Option<Edition>,
    #[command(flatten)]
    pub(crate) build_flags: BuildFlags,
    /// Enable the cross-crate re-export mode.
    #[arg(short = 'X', long)]
    pub(crate) cross_crate: bool,
    /// Enable ui_test-style compiletest directives: `//@`.
    #[arg(short = 'T', long, conflicts_with("cross_crate"))]
    pub(crate) compiletest: bool,
    /// Enable XPath / JsonPath queries.
    #[arg(short = 'Q', long, conflicts_with("cross_crate"), requires("compiletest"))]
    pub(crate) query: bool,
    #[command(flatten)]
    pub(crate) program_flags: ProgramFlags,
    /// Control when to use color.
    #[arg(long, value_name("WHEN"), default_value("auto"))]
    pub(crate) color: ColorChoice,
}

/// Flags that get passed to `rustc` and `rustdoc` in a lowered form.
#[derive(Parser)]
pub(crate) struct BuildFlags {
    /// Set the toolchain.
    #[arg(short, long, value_name("NAME"))]
    pub(crate) toolchain: Option<String>,
    /// Enable a `cfg`.
    #[arg(long = "cfg", value_name("SPEC"))]
    pub(crate) cfgs: Vec<String>,
    /// Enable a compiletest revision.
    #[arg(long = "rev", value_name("NAME"), requires("compiletest"))]
    pub(crate) revisions: Vec<String>,
    /// Enable a Cargo-like feature.
    #[arg(short = 'f', long = "cargo-feature", value_name("NAME"))]
    pub(crate) cargo_features: Vec<String>,
    /// Enable an experimental rustc library or language feature.
    #[arg(short = 'F', long = "rustc-feature", value_name("NAME"))]
    pub(crate) rustc_features: Vec<String>,
    /// Output JSON instead of HTML.
    #[arg(short, long, conflicts_with("open"))]
    pub(crate) json: bool,
    /// Set the version of the (root) crate.
    #[arg(short = 'v', long, value_name("VERSION"))]
    pub(crate) crate_version: Option<String>,
    /// Document private items.
    #[arg(short = 'P', long)]
    pub(crate) private: bool,
    /// Document hidden items.
    #[arg(short = 'H', long)]
    pub(crate) hidden: bool,
    /// Document the memory layout of types.
    #[arg(long)]
    pub(crate) layout: bool,
    /// Generate links to definitions.
    #[arg(short = 'D', long)]
    pub(crate) link_to_definition: bool,
    /// Normalize types and constants.
    #[arg(long)]
    pub(crate) normalize: bool,
    /// Set the theme.
    #[arg(long, default_value("ayu"))]
    pub(crate) theme: String,
    /// Cap lints at a level.
    #[arg(long, value_name("LEVEL"))]
    pub(crate) cap_lints: Option<String>,
    /// Enable rustc's `-Zverbose-internals`.
    #[arg(short = '#', long = "internals")]
    pub(crate) rustc_verbose_internals: bool,
    /// Override `RUSTC_LOG` to be `debug`.
    #[arg(long)]
    pub(crate) log: bool,
    /// Override `RUST_BACKTRACE` to be `0`.
    #[arg(short = 'B', long)]
    pub(crate) no_backtrace: bool,
}

/// Flags that are specific to `rruxwry` itself.
#[derive(Parser)]
pub(crate) struct ProgramFlags {
    /// Use verbose output.
    #[arg(short = 'V', long)]
    pub(crate) verbose: bool,

    /// Run through without making any changes.
    #[arg(short = '0', long)]
    pub(crate) dry_run: bool,
}

impl Edition {
    fn parse_cli_style(source: &str) -> Result<Self, String> {
        match source {
            "D" => Ok(Self::default()),
            "S" => Ok(Self::LATEST_STABLE),
            "U" => Ok(Self::BLEEDING_EDGE),
            source => source.parse(),
        }
        .map_err(|()| possible_values(Self::elements().map(Self::to_str).chain(["D", "S", "U"])))
    }
}

impl CrateNameBuf {
    fn parse_cli_style(source: &str) -> Result<Self, &'static str> {
        Self::adjust_and_parse(source).map_err(|()| "not a non-empty alphanumeric string")
    }
}

impl CrateType {
    fn parse_cli_style(source: &str) -> Result<Self, String> {
        source.parse().map_err(|()| possible_values(["bin", "lib", "rlib", "proc-macro"]))
    }
}

fn possible_values(values: impl IntoIterator<Item: std::fmt::Display, IntoIter: Clone>) -> String {
    format!(
        "possible values: {}",
        values.into_iter().map(|value| format!("`{value}`")).join_with(", ")
    )
}
