//! Low-level build commands.

// Note that we try to avoid generating unnecessary flags where possible even if that means
// doing more work on our side. The main motivation for this is being able to just copy/paste
// the commands printed by `--verbose` for use in GitHub discussions without requiring any
// manual minimization.
// FIXME: Also mention to reduce conflicts with compile flags passed via `ui_test`

use crate::{
    builder::{CrateName, CrateNameRef, CrateType, Edition},
    cli,
    error::Result,
    utility::{default, note},
};
use owo_colors::OwoColorize;
use std::{
    ffi::OsStr,
    ops::{Deref, DerefMut},
    path::Path,
    process,
};

pub(crate) fn compile(
    path: &Path,
    crate_name: CrateNameRef<'_>,
    crate_type: CrateType,
    edition: Edition,
    extern_crates: &[NamedCrate<'_>],
    build_flags: &cli::BuildFlags,
    program_flags: &cli::ProgramFlags,
    verbatim_flags: VerbatimFlags<'_>,
    strictness: Strictness,
) -> Result {
    let mut command = Command::new("rustc", program_flags, strictness);

    command.set_env_vars(build_flags);
    command.set_toolchain(build_flags);

    command.arg(path);

    command.set_edition(edition);
    command.set_crate_type(crate_type);
    command.set_crate_name(crate_name, path);

    command.set_extern_crates(extern_crates);

    command.set_cfgs(build_flags);
    command.set_rustc_features(build_flags);
    command.set_cap_lints(build_flags);
    command.set_internals_mode(build_flags);

    command.set_verbatim_flags(verbatim_flags);
    command.execute()
}

pub(crate) fn document(
    path: &Path,
    crate_name: CrateNameRef<'_>,
    crate_type: CrateType,
    edition: Edition,
    extern_crates: &[NamedCrate<'_>],
    build_flags: &cli::BuildFlags,
    program_flags: &cli::ProgramFlags,
    verbatim_flags: VerbatimFlags<'_>,
    strictness: Strictness,
) -> Result {
    let mut command = Command::new("rustdoc", program_flags, strictness);

    command.set_env_vars(build_flags);
    command.set_toolchain(build_flags);

    command.arg(path.as_os_str());

    command.set_crate_name(crate_name, path);
    if crate_type != default() {
        command.set_crate_type(crate_type);
    }
    command.set_edition(edition);

    command.set_extern_crates(extern_crates);

    if build_flags.json {
        command.arg("--output-format");
        command.arg("json");
        command.uses_unstable_options = true;
    }

    if build_flags.private {
        command.arg("--document-private-items");
    }

    if build_flags.hidden {
        command.arg("--document-hidden-items");
        command.uses_unstable_options = true;
    }

    if build_flags.layout {
        command.arg("--show-type-layout");
        command.uses_unstable_options = true;
    }

    if build_flags.link_to_definition {
        command.arg("--generate-link-to-definition");
        command.uses_unstable_options = true;
    }

    if build_flags.normalize {
        command.arg("-Znormalize-docs");
    }

    if let Some(crate_version) = &build_flags.crate_version {
        command.arg("--crate-version");
        command.arg(crate_version);
    }

    command.arg("--default-theme");
    command.arg(&build_flags.theme);

    command.set_cfgs(build_flags);
    command.set_rustc_features(build_flags);
    command.set_cap_lints(build_flags);
    command.set_internals_mode(build_flags);

    command.set_verbatim_flags(verbatim_flags);
    command.execute()
}

pub(crate) fn open(crate_name: CrateNameRef<'_>, flags: &cli::ProgramFlags) -> Result {
    let path = std::env::current_dir()?
        .join("doc")
        .join(crate_name.as_str())
        .join("index.html");

    if flags.verbose {
        note();

        let title = match flags.dry_run {
            false => "opening",
            true => "skipping opening", // FIXME: awkward wording!
        };

        eprintln!("{title} {}", path.to_string_lossy().green());
    }

    if !flags.dry_run {
        open::that(path)?;
    }

    Ok(())
}

struct Command<'a> {
    command: process::Command,
    flags: &'a cli::ProgramFlags,
    strictness: Strictness,
    uses_unstable_options: bool,
}

impl<'a> Command<'a> {
    fn new(
        program: impl AsRef<OsStr>,
        flags: &'a cli::ProgramFlags,
        strictness: Strictness,
    ) -> Self {
        Self {
            command: process::Command::new(program),
            flags,
            strictness,
            uses_unstable_options: false,
        }
    }

    fn execute(mut self) -> Result {
        self.set_unstable_options();

        self.print(false); // FIXME partially inline this
        if !self.flags.dry_run {
            self.status()?.exit_ok()?;
        }

        Ok(())
    }

    fn print(&self, force: bool) {
        if !self.flags.verbose {
            return;
        }

        note();

        let title = match (self.flags.dry_run, force) {
            (false, _) => "running",
            (true, false) => "skipping running", // FIXME: awkward wording
            (true, true) => "force-running",
        };

        eprint!("{title} ");

        for (var, value) in self.get_envs() {
            // FIXME: Print `env -u VAR` for removed vars before
            // added vars just like `Command`'s `Debug` impl.
            let Some(value) = value else { continue };

            eprint!(
                "{}{}{} ",
                var.to_string_lossy().yellow().bold(),
                "=".yellow(),
                value.to_string_lossy().yellow()
            );
        }
        eprint!("{}", self.get_program().to_string_lossy().purple().bold());
        for arg in self.get_args() {
            eprint!(" {}", arg.to_string_lossy().green());
        }
        eprintln!();
    }

    fn set_toolchain(&mut self, flags: &cli::BuildFlags) {
        if let Some(toolchain) = &flags.toolchain {
            self.arg(format!("+{toolchain}"));
        }
    }

    fn set_crate_name(&mut self, crate_name: CrateNameRef<'_>, path: &Path) {
        // FIXME: unwrap
        let fiducial_crate_name = CrateName::from_path(path).unwrap();

        if crate_name != fiducial_crate_name.as_ref() {
            self.arg("--crate-name");
            self.arg(crate_name.as_str());
        }
    }

    fn set_crate_type(&mut self, crate_type: CrateType) {
        self.arg("--crate-type");
        self.arg(crate_type.to_str());
    }

    fn set_edition(&mut self, edition: Edition) {
        if edition != default() {
            self.arg("--edition");
            self.arg(edition.to_str());
        }
        if !edition.is_stable() {
            self.uses_unstable_options = true;
        }
    }

    fn set_extern_crates(&mut self, extern_crates: &[NamedCrate<'_>]) {
        // FIXME: should we skip this if Strictness::Strict?
        // What does `ui_test` do?
        if !extern_crates.is_empty() {
            // FIXME: Does this work with proc macro deps? I think so?
            self.arg("-Lcrate=.");
        }

        for NamedCrate { name, path } in extern_crates {
            self.arg("--extern");
            match path {
                Some(path) => self.arg(format!("{name}={path}")),
                None => self.arg(name.as_str()),
            };
        }
    }

    fn set_internals_mode(&mut self, flags: &cli::BuildFlags) {
        if flags.rustc_verbose_internals {
            self.arg("-Zverbose-internals");
        }
    }

    fn set_env_vars(&mut self, flags: &cli::BuildFlags) {
        if flags.log {
            self.env("RUSTC_LOG", "debug");
        }
        if flags.no_backtrace {
            self.env("RUST_BACKTRACE", "0");
        }
    }

    fn set_cfgs(&mut self, flags: &cli::BuildFlags) {
        for cfg in &flags.cfgs {
            self.arg("--cfg");
            self.arg(cfg);
        }
        for feature in &flags.cargo_features {
            // FIXME: Warn on conflicts with `cfgs` from `self.arguments.cfgs`.
            self.arg("--cfg");
            self.arg(format!("feature=\"{feature}\""));
        }
    }

    fn set_rustc_features(&mut self, flags: &cli::BuildFlags) {
        for feature in &flags.rustc_features {
            self.arg(format!("-Zcrate-attr=feature({feature})"));
        }
    }

    fn set_cap_lints(&mut self, flags: &cli::BuildFlags) {
        if let Some(level) = flags.cap_lints {
            self.arg("--cap-lints");
            self.arg(level.to_str());
        }
    }

    fn set_unstable_options(&mut self) {
        if let Strictness::Lenient = self.strictness
            && self.uses_unstable_options
        {
            self.arg("-Zunstable-options");
        }
    }

    fn set_verbatim_flags(&mut self, flags: VerbatimFlags<'_>) {
        self.envs(flags.rustc_envs.iter().copied());
        for key in flags.unset_rustc_env {
            self.env_remove(key);
        }
        self.args(flags.compile_flags);
    }
}

impl Deref for Command<'_> {
    type Target = process::Command;

    fn deref(&self) -> &Self::Target {
        &self.command
    }
}

impl DerefMut for Command<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.command
    }
}

pub(crate) struct NamedCrate<'src> {
    pub(crate) name: CrateNameRef<'src>,
    pub(crate) path: Option<String>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct VerbatimFlags<'a> {
    pub(crate) compile_flags: &'a [&'a str],
    pub(crate) rustc_envs: &'a [(&'a str, &'a str)],
    pub(crate) unset_rustc_env: &'a [&'a str],
}

pub(crate) enum Strictness {
    Strict,
    Lenient,
}
