use chrono::{DateTime, Local};
use colored::Colorize;
use minijinja::{Environment, Template};

use crate::error::{Error, Result};
use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use similar::TextDiff;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum TemplateSource {
    /// A template to be read from the file system
    FileSystem(PathBuf),

    /// A template to be read from standard input
    StdIn,

    /// A pre-compiled template that's been added to the environment
    /// and is referenced by a name
    Cached {
        name: PathBuf,
        contains_trailing_newline: bool,
    },
}

impl TemplateSource {
    pub fn from_path<S: AsRef<str>>(path: S) -> Self {
        match path.as_ref() {
            "-" => Self::StdIn,
            other => Self::FileSystem(other.into()),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            TemplateSource::FileSystem(path) => Some(path.as_ref()),
            TemplateSource::Cached { name: path, .. } => Some(path.as_ref()),
            TemplateSource::StdIn => None,
        }
    }

    /// Ensure the template is loaded, and return a cached template source.
    fn ensure_cached(&mut self, env: &mut Environment) -> Result<()> {
        let mut template = String::new();

        let name = match self {
            TemplateSource::FileSystem(path) => {
                std::fs::OpenOptions::new()
                    .read(true)
                    .open(&path)?
                    .read_to_string(&mut template)?;
                path.clone()
            }
            TemplateSource::StdIn => {
                log::info!("Reading template from standard input");
                io::stdin().lock().read_to_string(&mut template)?;
                PathBuf::from("-")
            }
            TemplateSource::Cached { .. } => return Ok(()),
        };

        let template_name = name.to_string_lossy().to_string();
        let contains_trailing_newline = template.chars().last().map(|c| c == '\n').unwrap_or(false);
        env.add_template_owned(template_name, template)?;

        *self = TemplateSource::Cached {
            name,
            contains_trailing_newline,
        };

        Ok(())
    }

    /// Get the name of a cached template
    ///
    /// # Panics
    /// Panics if this template is not [cached](TemplateSource::Cached).
    pub fn get_cached_name(&self) -> Cow<str> {
        match self {
            TemplateSource::Cached { ref name, .. } => name.to_string_lossy(),
            _ => panic!("get_cached_name called on a non-cached template"),
        }
    }

    /// Returns whether the original template contained a trailing newline
    ///
    /// # Panics
    /// Panics if this template is not [cached](TemplateSource::Cached).
    pub fn get_cached_contains_trailing_newline(&self) -> bool {
        match self {
            TemplateSource::Cached {
                contains_trailing_newline,
                ..
            } => *contains_trailing_newline,
            _ => panic!("get_cached_name called on a non-cached template"),
        }
    }

    pub fn get_template<'env, 'source>(
        &self,
        env: &'env Environment<'source>,
    ) -> Result<Template<'env, 'source>> {
        Ok(env.get_template(self.get_cached_name().as_ref())?)
    }
}

fn colorize_diff(diff: &mut String) {
    let mut out = String::with_capacity(diff.len());
    for line in diff.lines() {
        let line = match line.chars().next() {
            Some('+') => line.green(),
            Some('-') => line.red(),
            Some('@') => line.yellow(),
            _ => line.clear(),
        };
        out.push_str(&format!("{line}\n"));
    }
    *diff = out;
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TemplateDestination {
    FileSystem(PathBuf),
    StdOut,
}

impl TemplateDestination {
    pub fn from_path<S: AsRef<str>>(path: S) -> Self {
        match path.as_ref() {
            "-" => Self::StdOut,
            other => Self::FileSystem(other.into()),
        }
    }

    pub fn path(&self) -> Cow<Path> {
        match self {
            TemplateDestination::FileSystem(path) => Cow::Borrowed(path.as_ref()),
            TemplateDestination::StdOut => Cow::Owned(PathBuf::from("-")),
        }
    }

    pub fn is_stdout(&self) -> bool {
        matches!(self, TemplateDestination::StdOut)
    }

    /// Whether the template Destination supports re-rendering
    pub fn supports_notify(&self) -> bool {
        !self.is_stdout()
    }

    fn diff(
        &self,
        filename: &Path,
        file: &mut std::fs::File,
        templated: &String,
        log: bool,
    ) -> Result<bool> {
        let mut existing = String::with_capacity(templated.len());
        file.read_to_string(&mut existing)?;
        file.rewind()?;

        let changed = &existing != templated;

        if log && changed {
            let diff = TextDiff::from_lines(&existing, templated);
            let modified: DateTime<Local> = file.metadata()?.modified()?.into();
            let now: DateTime<Local> = SystemTime::now().into();
            let old = format!(
                "{}\t{}",
                filename.to_string_lossy(),
                modified.format("+%Y-%m-%d %H:%M:%S %z")
            );
            let new = format!("{:?}\t{}", filename, now.format("+%Y-%m-%d %H:%M:%S %z"));

            let mut diff = diff.unified_diff().header(&old, &new).to_string();
            if atty::is(atty::Stream::Stderr) {
                colorize_diff(&mut diff)
            }
            eprint!("{diff}");
        }

        Ok(changed)
    }

    /// Write the template to the destination
    ///
    /// Will only write to the destination if it would be changed.
    /// If `log_diff` is true, also write a diff to the standard error.
    /// Returns true if the destination was changed.
    pub fn write_templated(&self, templated: String, log_diff: bool) -> Result<bool> {
        let ret = match self {
            TemplateDestination::FileSystem(path) => {
                let mut f = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(path)?;

                if self.diff(path, &mut f, &templated, log_diff)? {
                    f.set_len(0)?;
                    f.write_all(templated.as_bytes())?;
                    true
                } else {
                    false
                }
            }
            TemplateDestination::StdOut => {
                write!(io::stdout().lock(), "{templated}")?;
                true
            }
        };

        Ok(ret)
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct TemplateOperation {
    pub source: TemplateSource,
    pub dest: TemplateDestination,

    /// An extension to add to the source, if a backup should be made.
    pub backup: Option<String>,
}

impl TemplateOperation {
    /// Create a template operation
    pub fn new(source: TemplateSource, dest: TemplateDestination) -> Self {
        Self {
            source,
            dest,
            backup: None,
        }
    }

    pub fn new_in_place<S: AsRef<str>>(path: S, backup: Option<&str>) -> Self {
        let ret = Self::new(
            TemplateSource::from_path(path.as_ref()),
            TemplateDestination::from_path(path.as_ref()),
        );
        if let Some(extension) = backup {
            return ret.with_backup_extension(extension.into());
        }
        ret
    }

    /// Backup the source file by adding the given extension to it
    pub fn with_backup_extension(mut self, extension: String) -> Self {
        self.backup = Some(extension);
        self
    }

    /// Represents templating a template from stdin to stdout.
    pub fn stdio() -> Self {
        Self {
            source: TemplateSource::StdIn,
            dest: TemplateDestination::StdOut,
            backup: None,
        }
    }

    pub fn ensure_cached(&mut self, env: &mut Environment) -> Result<()> {
        self.source.ensure_cached(env)
    }

    fn do_backup(&mut self) -> Result<()> {
        let Some(extension) = self.backup.take().map(OsString::from) else {
            return Ok(());
        };

        let Some(source_path) = self.source.path() else {
            return Ok(());
        };

        let Some(destination_filename) =
            source_path
                .file_name()
                .map(OsStr::to_owned)
                .map(|mut filename| {
                    filename.push(OsString::from("."));
                    filename.push(extension);
                    filename
                })
        else {
            return Ok(());
        };

        let mut destination_path = source_path.to_owned();
        destination_path.set_file_name(destination_filename);

        if destination_path.exists() {
            let mut source = File::open(source_path)?;
            let mut destination = File::open(&destination_path)?;

            if !file_diff::diff_files(&mut source, &mut destination) {
                return Err(Error::BackupWouldBeOverwritten(destination_path));
            }
        }

        log::info!("Backing up: {source_path:?} -> {destination_path:?}");
        std::fs::copy(source_path, destination_path)?;

        Ok(())
    }

    /// Apply a template operation.
    ///
    /// If `dry_run` is specified, no change will be made.
    /// If `log_diff` is specified, a diff with changes to be made is written to standard error.
    /// Returns true if the destination was changed.
    pub fn apply(
        &mut self,
        env: &mut Environment,
        ctx: &serde_json::Value,
        dry_run: bool,
        log_diff: bool,
    ) -> Result<bool> {
        self.ensure_cached(env)?;

        let mut templated = self.source.get_template(env)?.render(ctx)?;

        if self.source.get_cached_contains_trailing_newline() {
            templated.push('\n');
        }

        let mut ret = false;
        if !dry_run {
            self.do_backup()?;
            ret = self.dest.write_templated(templated, log_diff)?;
        }

        Ok(ret)
    }
}

#[derive(Default, Debug, Clone, Hash, Eq, PartialEq)]
pub struct Plan {
    operations: Vec<TemplateOperation>,
}

impl Plan {
    /// Create a plan with a single operation, templating standard input to standard output.
    pub fn stdio() -> Self {
        Self {
            operations: vec![TemplateOperation::stdio()],
        }
    }

    pub fn add_template(&mut self, source: TemplateSource, dest: TemplateDestination) {
        self.operations.push(TemplateOperation::new(source, dest));
    }

    pub fn ensure_cached(&mut self, env: &mut Environment) -> Result<()> {
        for op in self.operations.iter_mut() {
            op.ensure_cached(env)?;
        }

        Ok(())
    }

    /// Apply all template operations, ignoring errors.
    ///
    /// Returns a list of all template operations that caused a change.
    pub fn execute(
        &mut self,
        env: &mut Environment,
        ctx: &serde_json::Value,
        dry_run: bool,
        log_diff: bool,
    ) -> Vec<&TemplateOperation> {
        self.operations
            .iter_mut()
            .filter_map(|operation| {
                if operation
                    .apply(env, ctx, dry_run, log_diff)
                    .map_err(|e| {
                        log::warn!(
                            "Could not apply template operation {:?} -> {:?}: {e}",
                            operation.source,
                            operation.dest
                        )
                    })
                    .unwrap_or(false)
                {
                    Some(&*operation)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Apply all templating operations, returning on the first error.
    ///
    /// Returns a list of all template operations that caused a change.
    pub fn try_execute(
        &mut self,
        env: &mut Environment,
        ctx: &serde_json::Value,
        dry_run: bool,
        log_diff: bool,
    ) -> Result<Vec<&TemplateOperation>> {
        let changed = self
            .operations
            .iter_mut()
            .filter_map(|operation| {
                match operation.apply(env, ctx, dry_run, log_diff).map(|changed| {
                    if changed {
                        Some(&*operation)
                    } else {
                        None
                    }
                }) {
                    Ok(None) => None,
                    Ok(Some(t)) => Some(Ok(t)),
                    Err(e) => Some(Err(e)),
                }
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(changed)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, TemplateOperation> {
        self.operations.iter()
    }
}

impl From<Vec<TemplateOperation>> for Plan {
    fn from(operations: Vec<TemplateOperation>) -> Self {
        Self { operations }
    }
}
