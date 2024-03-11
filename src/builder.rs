//! High-level build commands.

use crate::{
    cli,
    command::{self, NamedCrate, Strictness, VerbatimFlags},
    directive::{Dependency, Directives},
    error::Result,
    utility::default,
};
use std::{borrow::Cow, cell::LazyCell, fmt, path::Path, str::FromStr};

pub(crate) fn build<'a>(
    mode: BuildMode,
    path: &Path,
    crate_name: CrateNameRef<'a>,
    crate_type: CrateType,
    edition: Edition,
    build_flags: &cli::BuildFlags,
    program_flags: &cli::ProgramFlags,
) -> Result<CrateNameCow<'a>> {
    match mode {
        BuildMode::Default => build_default_mode(
            path,
            crate_name,
            crate_type,
            edition,
            build_flags,
            program_flags,
        ),
        BuildMode::CrossCrate => build_cross_crate_mode(
            path,
            crate_name,
            crate_type,
            edition,
            build_flags,
            program_flags,
        ),
        BuildMode::UiTest => {
            build_ui_test_mode(path, crate_name, edition, build_flags, program_flags)
        }
    }
}

fn build_default_mode<'a>(
    path: &Path,
    crate_name: CrateNameRef<'a>,
    crate_type: CrateType,
    edition: Edition,
    build_flags: &cli::BuildFlags,
    program_flags: &cli::ProgramFlags,
) -> Result<CrateNameCow<'a>> {
    command::document(
        path,
        crate_name,
        crate_type,
        edition,
        crate_type.crates(),
        build_flags,
        program_flags,
        default(),
        Strictness::Lenient,
    )?;

    Ok(crate_name.map(Cow::Borrowed))
}

fn build_cross_crate_mode(
    path: &Path,
    crate_name: CrateNameRef<'_>,
    crate_type: CrateType,
    edition: Edition,
    build_flags: &cli::BuildFlags,
    program_flags: &cli::ProgramFlags,
) -> Result<CrateNameCow<'static>> {
    command::compile(
        path,
        crate_name,
        crate_type.to_non_executable(),
        edition,
        crate_type.crates(),
        build_flags,
        program_flags,
        default(),
        Strictness::Lenient,
    )?;

    let dependent_crate_name = CrateName::new(format!("u_{crate_name}"));
    let dependent_crate_path = path
        .with_file_name(dependent_crate_name.as_str())
        .with_extension("rs");

    if !program_flags.dry_run && !dependent_crate_path.exists() {
        // While we could omit the `extern crate` declaration in `edition >= Edition::Edition2018`,
        // we would need to recreate the file on each rerun if the edition was 2015 instead of
        // skipping that step since we wouldn't know whether the existing file if applicable was
        // created for a newer edition or not.
        std::fs::write(
            &dependent_crate_path,
            format!("extern crate {crate_name}; pub use {crate_name}::*;\n"),
        )?;
    };

    command::document(
        &dependent_crate_path,
        dependent_crate_name.as_ref(),
        default(),
        edition,
        &[NamedCrate {
            name: crate_name.as_ref(),
            path: None,
        }],
        build_flags,
        program_flags,
        default(),
        Strictness::Lenient,
    )?;

    Ok(dependent_crate_name.map(Cow::Owned))
}

fn build_ui_test_mode<'a>(
    path: &Path,
    crate_name: CrateNameRef<'a>,
    _edition: Edition, // FIXME: should we respect the edition or should we reject it with `clap`?
    build_flags: &cli::BuildFlags,
    program_flags: &cli::ProgramFlags,
) -> Result<CrateNameCow<'a>> {
    // FIXME: Add a flag `--all-revisions` or something like that.
    // FIXME: Make sure `// compile-flags: --extern name` works as expected
    let source = std::fs::read_to_string(path)?;
    let directives = Directives::parse(&source);

    // Theoretically speaking we should also pass Cargo-like features here after
    // having converted them to cfg specs but practically speaking it's not worth
    // the effort. // FIXME: This will be fixed once we eagerly expand `-f` to `--cfg`
    let directives = directives.into_instantiated(&build_flags.cfgs);

    // FIXME: unwrap
    let auxiliary_base_path = LazyCell::new(|| path.parent().unwrap().join("auxiliary"));

    // FIXME: we don't pass -L. if there are only "implicit deps", add a flag to make Command pass -L
    let dependencies = directives
        .dependencies
        .iter()
        .flat_map(|dependency| {
            build_ui_test_auxiliary(
                dependency,
                &auxiliary_base_path,
                directives.build_aux_docs,
                build_flags,
                program_flags,
            )
        })
        .collect::<Result<Vec<_>>>()?;

    command::document(
        path,
        crate_name,
        default(), // FIXME: respect `@compile-flags: --crate-type`
        directives.edition.unwrap_or_default(),
        &dependencies,
        build_flags,
        program_flags,
        VerbatimFlags {
            compile_flags: &directives.compile_flags,
            rustc_envs: &directives.rustc_env,
            unset_rustc_env: &directives.unset_rustc_env,
        },
        Strictness::Strict,
    )?;

    Ok(crate_name.map(Cow::Borrowed))
}

// FIXME: Support nested auxiliaries!
fn build_ui_test_auxiliary<'a>(
    dependency: &Dependency<'a>,
    base_path: &Path,
    document: bool,
    build_flags: &cli::BuildFlags,
    program_flags: &cli::ProgramFlags,
) -> Option<Result<NamedCrate<'a>>> {
    // FIXME: change the repr of directive::Dependency so we don't need to perform this logic here
    let path = match dependency {
        Dependency::Crate { path }
        | Dependency::NamedCrate {
            path: Some(path), ..
        } => base_path.join(path),
        Dependency::NamedCrate { name, path: None } => {
            base_path.join(name.as_str()).with_extension("rs")
        }
    };

    let source = std::fs::read_to_string(&path);

    // FIXME: unwrap
    let crate_name = CrateName::from_path(&path).unwrap();

    // FIXME: What about instantiation???
    let directives = source
        .as_ref()
        .map(|source| Directives::parse(source))
        .unwrap_or_default();

    let edition = directives.edition.unwrap_or_default();
    let verbatim_flags = VerbatimFlags {
        compile_flags: &directives.compile_flags,
        rustc_envs: &directives.rustc_env,
        unset_rustc_env: &directives.unset_rustc_env,
    };

    if let Err(error) = command::compile(
        &path,
        crate_name.as_ref(),
        // FIXME: Verify this works with `@compile-flags:--crate-type`
        // FIXME: I don't think it works rn
        default(),
        edition,
        &[],
        build_flags,
        program_flags,
        verbatim_flags,
        Strictness::Strict,
    ) {
        return Some(Err(error));
    }

    // FIXME: Is this how `//@ build-aux-docs` is supposed to work?
    if document
        && let Err(error) = command::document(
            &path,
            crate_name.as_ref(),
            // FIXME: Verify this works with `@compile-flags:--crate-type`
            // FIXME: I don't think it works rn
            default(),
            edition,
            &[],
            build_flags,
            program_flags,
            verbatim_flags,
            Strictness::Strict,
        )
    {
        return Some(Err(error));
    }

    match dependency {
        Dependency::Crate { .. } => None,
        &Dependency::NamedCrate { name, .. } => {
            // Note that `compiletest` probably doesn't handle this case correctly
            // contrary to us. I think it fails to link auxiliary crates if they
            // use `#![crate_name]` or `// compile-flags: --crate-name`.
            // FIXME: Respect `compile-flags: --crate-name`.
            // let crate_name = match auxiliary_file_data.crate_name {
            //     Some(name) => Cow::Borrowed(name),
            //     None => compute_crate_name_from_path(&auxiliary_crate_path).into(),
            // };
            // FIXME: unwrap
            let crate_name = CrateName::from_path(&path).unwrap();

            Some(Ok(NamedCrate {
                name,
                // FIXME: layer violation?? should this be the job of mod command?
                path: (name != crate_name.as_ref()).then(|| format!("lib{crate_name}.rlib")),
            }))
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum BuildMode {
    Default,
    CrossCrate,
    UiTest,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) enum Edition {
    #[default]
    Edition2015,
    Edition2018,
    Edition2021,
    Edition2024,
}

impl Edition {
    pub(crate) const LATEST_STABLE: Self = Self::Edition2021;

    pub(crate) fn is_stable(self) -> bool {
        self <= Self::LATEST_STABLE
    }

    pub(crate) const fn to_str(self) -> &'static str {
        match self {
            Self::Edition2015 => "2015",
            Self::Edition2018 => "2018",
            Self::Edition2021 => "2021",
            Self::Edition2024 => "2024",
        }
    }

    // FIXME: Derive this.
    pub(crate) const fn elements() -> &'static [Self] {
        &[
            Self::Edition2015,
            Self::Edition2018,
            Self::Edition2021,
            Self::Edition2024,
        ]
    }
}

impl FromStr for Edition {
    type Err = ();

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Ok(match source {
            "2015" => Self::Edition2015,
            "2018" => Self::Edition2018,
            "2021" => Self::Edition2021,
            "2024" => Self::Edition2024,
            _ => return Err(()),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(test, derive(Debug))]
pub(crate) enum CrateType {
    #[default]
    Bin,
    Lib,
    ProcMacro,
}

impl CrateType {
    pub(crate) const fn to_str(self) -> &'static str {
        match self {
            Self::Bin => "bin",
            Self::Lib => "lib",
            Self::ProcMacro => "proc-macro",
        }
    }

    fn crates(self) -> &'static [NamedCrate<'static>] {
        match self {
            // For convenience and just like Cargo we add `libproc_macro` to the external prelude.
            Self::ProcMacro => &[NamedCrate {
                name: CrateName("proc_macro"),
                path: None,
            }],
            _ => [].as_slice(),
        }
    }

    fn to_non_executable(self) -> Self {
        match self {
            Self::Bin => Self::Lib,
            Self::Lib | Self::ProcMacro => self,
        }
    }
}

impl FromStr for CrateType {
    type Err = ();

    // FIXME: Support `dylib`, `staticlib` etc.
    fn from_str(source: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match source {
            "bin" => Self::Bin,
            "lib" | "rlib" => Self::Lib,
            "proc-macro" => Self::ProcMacro,
            _ => return Err(()),
        })
    }
}

pub(crate) type CrateNameBuf = CrateName<String>;
pub(crate) type CrateNameRef<'a> = CrateName<&'a str>;
pub(crate) type CrateNameCow<'a> = CrateName<Cow<'a, str>>;

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub(crate) struct CrateName<T: AsRef<str>>(T);

impl<T: AsRef<str>> CrateName<T> {
    pub(crate) fn new(name: T) -> Self {
        Self(name)
    }

    pub(crate) fn map<U: AsRef<str>>(self, mapper: impl FnOnce(T) -> U) -> CrateName<U> {
        CrateName(mapper(self.0))
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl<'src> CrateNameRef<'src> {
    pub(crate) fn parse_strict(source: &'src str) -> Result<Self, ()> {
        let mut chars = source.chars();
        if let Some(char) = chars.next()
            && (char.is_ascii_alphabetic() || char == '_')
            && chars.all(|char| char.is_ascii_alphanumeric() || char == '_')
        {
            Ok(CrateName::new(source))
        } else {
            Err(())
        }
    }
}

impl CrateNameBuf {
    pub(crate) fn from_path(path: &Path) -> Result<Self, ()> {
        path.file_stem()
            .and_then(|name| name.to_str())
            .map(|name| Self(name.replace('-', "_")))
            .ok_or(())
    }

    pub(crate) fn parse_lenient(source: &str) -> Result<Self, ()> {
        let mut chars = source.chars();
        if let Some(char) = chars.next()
            && (char.is_ascii_alphabetic() || char == '_' || char == '-')
            && chars.all(|char| char.is_ascii_alphanumeric() || char == '_' || char == '-')
        {
            let crate_name = source.replace('-', "_");
            Ok(CrateName::new(crate_name))
        } else {
            Err(())
        }
    }
}

impl<T: AsRef<str>> CrateName<T> {
    pub(crate) fn as_ref(&self) -> CrateNameRef<'_> {
        CrateName(self.0.as_ref())
    }
}

impl From<CrateNameBuf> for CrateNameCow<'_> {
    fn from(name: CrateNameBuf) -> Self {
        name.map(Cow::Owned)
    }
}

impl<'a> From<CrateNameRef<'a>> for CrateNameCow<'a> {
    fn from(name: CrateNameRef<'a>) -> Self {
        name.map(Cow::Borrowed)
    }
}

impl<T: AsRef<str>> fmt::Display for CrateName<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy)]
pub(crate) enum LintLevel {
    Allow,
    Warn,
    Deny,
    Forbid,
}

impl LintLevel {
    pub(crate) const fn to_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Warn => "warn",
            Self::Deny => "deny",
            Self::Forbid => "forbid",
        }
    }

    // FIXME: Derive this.
    pub(crate) const fn elements() -> &'static [Self] {
        &[Self::Allow, Self::Warn, Self::Deny, Self::Forbid]
    }
}
