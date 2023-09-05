use std::collections::HashSet;
use std::env;
use std::ffi::CString;
use std::hash::Hash;

use crate::datasource::k8s::Secret;
use crate::datasource::{ConfigMap, Environment, File, Source, SourceRegistry};
use crate::error::{Error, Result};
use crate::plan::{Plan, TemplateDestination, TemplateOperation, TemplateSource};
use crate::reload::{OnReloadAction, OnReloadSignalTarget};
use clap::error::ErrorKind;
use clap::{value_parser, Arg, ArgAction, ArgGroup, ArgMatches, Command, ValueHint};
use clap_complete::{generate, Generator, Shell};
use indoc::indoc;
use nix::sys::signal::Signal;
use shadow_rs::shadow;

shadow!(build);

pub struct Cli {
    matches: ArgMatches,
}

impl Cli {
    pub fn new() -> Result<Self> {
        let mut app = command();
        let matches = app.try_get_matches_from_mut(env::args_os())?;
        Self { matches }.validate(&mut app)
    }

    #[cfg(test)]
    pub(crate) fn new_from<I, T>(itr: I) -> Result<Self>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let mut app = command();
        let matches: ArgMatches = app.try_get_matches_from_mut(itr)?;
        Self { matches }.validate(&mut app)
    }

    fn validate(self, cmd: &mut Command) -> Result<Self> {
        // no two operations write to the same output
        let plan = self.plan();
        let destinations: Vec<_> = plan.iter().map(|op| &op.dest).collect();
        if !elements_are_unique(destinations) {
            let e = cmd.error(
                ErrorKind::ValueValidation,
                "Template destinations are not unique!",
            );
            Err(Error::ClapError(e))?
        }

        let notify_unsupported: Vec<_> = plan
            .iter()
            .filter(|op| !op.dest.supports_notify())
            .collect();
        if self.watch_mode() && !notify_unsupported.is_empty() {
            let e = cmd.error(
                ErrorKind::ValueValidation,
                format!("Watch mode specified, but the following template operations don't support it: {notify_unsupported:?}"),
            );
            Err(Error::ClapError(e))?
        }

        Ok(self)
    }

    fn get_source_from_spec<S1, S2>(
        &self,
        source_type: S1,
        arg: Option<S2>,
    ) -> Box<dyn Source + Send + Sync>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        match source_type.as_ref() {
            "file" => Box::new(File::new(arg.unwrap().as_ref())),
            "environment" => Box::new(Environment::new(match arg {
                None => None,
                Some(prefix) if prefix.as_ref().is_empty() => None,
                prefix => prefix,
            })),
            "k8s-configmap" => Box::new(ConfigMap::new(arg.unwrap(), self.k8s_namespace())),
            "k8s-secret" => Box::new(Secret::new(arg.unwrap(), self.k8s_namespace())),
            _ => unreachable!(),
        }
    }

    fn parse_source_env_variable(&self, value: &str) -> Vec<Box<dyn Source + Send + Sync>> {
        value
            .split(',')
            .map(|source_spec| {
                let mut split = source_spec.splitn(2, ':');
                let source_type = split.next().unwrap();
                let arg = split.next();
                self.get_source_from_spec(source_type, arg)
            })
            .collect()
    }

    /// Get a `SourceRegistry` with all sources specified.
    ///
    /// Sources are taken from command line arguments and the `CONTEMPLATE_DATASOURCES` environment variable.
    /// Sources specified later override earlier ones, and command line arguments override environment variables.
    pub fn sources(&self) -> SourceRegistry {
        let sources_from_env = env::var("CONTEMPLATE_DATASOURCES")
            .ok()
            .map(|value| self.parse_source_env_variable(&value))
            .into_iter()
            .flatten();

        let mut sources = ["file", "environment", "k8s-configmap", "k8s-secret"]
            .into_iter()
            .flat_map(|source_type| {
                let files = std::iter::zip(
                    self.matches
                        .get_occurrences::<String>(source_type)
                        .unwrap_or_default()
                        .map(|mut occurrence| occurrence.next()),
                    self.matches.indices_of(source_type).unwrap_or_default(),
                )
                .map(move |(value, index)| (source_type, value, index));
                files
            })
            .collect::<Vec<(&str, Option<&String>, usize)>>();

        sources.sort_by(|(_, _, a), (_, _, b)| a.cmp(b));

        let sources_from_args =
            sources
                .into_iter()
                .map(|(source_type, arg, _)| -> Box<dyn Source + Sync + Send> {
                    self.get_source_from_spec(source_type, arg)
                });

        SourceRegistry::new(sources_from_env.chain(sources_from_args))
    }

    pub fn template_args(&self) -> Vec<TemplateOperation> {
        let Some(occurrences) = self.matches.get_occurrences::<String>("template") else {
            return vec![];
        };

        let in_place = &self.in_place();

        occurrences
            .map(|occurrence| {
                let occurrence: Vec<&String> = occurrence.collect();
                match occurrence.len() {
                    1 => {
                        if in_place.into() {
                            TemplateOperation::new_in_place(occurrence[0], in_place.extension())
                        } else {
                            TemplateOperation::new(
                                TemplateSource::from_path(occurrence[0]),
                                TemplateDestination::StdOut,
                            )
                        }
                    }
                    2 => TemplateOperation::new(
                        TemplateSource::from_path(occurrence[0]),
                        TemplateDestination::from_path(occurrence[1]),
                    ),
                    _ => unreachable!(),
                }
            })
            .collect()
    }

    pub fn intput_output_args(&self) -> Vec<TemplateOperation> {
        let output = self
            .matches
            .get_one::<String>("output")
            .map(String::to_owned);

        let Some(inputs) = self.matches.get_many::<String>("input") else {
            return output
                .map(|output| {
                    TemplateOperation::new(
                        TemplateSource::StdIn,
                        TemplateDestination::from_path(output),
                    )
                })
                .into_iter()
                .collect();
        };

        let output = output.unwrap_or("-".into());

        let in_place = &self.in_place();

        inputs
            .into_iter()
            .map(|input| {
                if in_place.into() {
                    TemplateOperation::new_in_place(input, in_place.extension())
                } else {
                    TemplateOperation::new(
                        TemplateSource::from_path(input),
                        TemplateDestination::from_path(&output),
                    )
                }
            })
            .collect()
    }

    pub fn signal_arg(&self) -> Result<Option<(Signal, OnReloadSignalTarget)>> {
        let Some(args) = self.matches.get_raw("on-reload-signal") else {
            return Ok(None);
        };

        let args = args.into_iter().collect::<Vec<_>>();

        if let Some(signal) = args[0]
            .to_str()
            .and_then(|s| s.parse().ok())
            .and_then(|signum: i32| Signal::try_from(signum).ok())
        {
            let target = args.get(1).map(|s| (*s).into()).unwrap_or_default();
            return Ok(Some((signal, target)));
        }

        if let Some(signal) = args[0].to_str().and_then(|s| s.to_uppercase().parse().ok()) {
            let target = args.get(1).map(|s| (*s).into()).unwrap_or_default();
            return Ok(Some((signal, target)));
        }

        if let Some(signal) = args[0]
            .to_str()
            .and_then(|s| format!("SIG{}", s.to_uppercase()).parse().ok())
        {
            let target = args.get(1).map(|s| (*s).into()).unwrap_or_default();
            return Ok(Some((signal, target)));
        }

        Err(Error::CliInvalidSignal)
    }

    /// Return the user-specified on-reload action, if available.
    pub fn on_reload(&self) -> Result<OnReloadAction> {
        if let Some(command) = self
            .matches
            .get_raw("on-reload-command")
            .and_then(|mut opt| opt.next())
        {
            return Ok(OnReloadAction::ShellCommand(command.to_owned()));
        }

        if let Some(executable) = self
            .matches
            .get_raw("on-reload-exec")
            .and_then(|mut opt| opt.next())
        {
            return Ok(OnReloadAction::Executable(executable.to_owned()));
        }

        if let Some((signal, target)) = self.signal_arg()? {
            return Ok(OnReloadAction::Signal { signal, target });
        }

        Ok(OnReloadAction::None)
    }

    /// Get the value of the `--and-then-exec` / `-x` argument.
    pub fn and_then_exec(&self) -> Option<(CString, Vec<CString>)> {
        let mut values = self.matches.get_many::<String>("and-then-exec")?;

        let binary = values.next().unwrap().as_str();

        let mut args = vec![CString::new(binary).unwrap()];
        let binary = which::which(binary)
            .inspect_err(|e| log::error!("Cannot find the given binary: {e}"))
            .map(|ref path| CString::new(path.to_str().unwrap()))
            .unwrap_or(CString::new(""))
            .unwrap();

        args.extend(
            values
                .map(|arg| CString::new(arg.as_str()))
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap(),
        );

        Some((binary, args))
    }

    /// The k8s-namespace argument
    ///
    /// Attempts to take this from the `--k8s-namespace` argument, falling back to the `CONTEMPLATE_K8S_NAMESPACE` environment variable.
    pub fn k8s_namespace(&self) -> Option<String> {
        self.matches
            .get_one::<String>("k8s-namespace")
            .map(ToOwned::to_owned)
            .or_else(|| env::var("CONTEMPLATE_K8S_NAMESPACE").ok())
    }

    /// Should editing be done in-place
    pub fn in_place(&self) -> InPlace {
        if self.matches.get_occurrences::<String>("in-place").is_some() {
            let suffix = self.matches.get_one::<String>("in-place");
            match suffix {
                None => InPlace::WithoutSuffix,
                Some(suffix) => InPlace::WithSuffix(suffix.to_owned()),
            }
        } else {
            InPlace::No
        }
    }

    /// Was watch arg given
    pub fn watch_mode(&self) -> bool {
        if let Some(watch) = self.matches.get_one("watch") {
            *watch
        } else {
            false
        }
    }

    /// Was diff arg given
    pub fn diff(&self) -> bool {
        if let Some(diff) = self.matches.get_one("diff") {
            *diff
        } else {
            false
        }
    }

    /// Was dry_run arg given
    pub fn dry_run(&self) -> bool {
        if let Some(dry_run) = self.matches.get_one("dry-run") {
            *dry_run
        } else {
            false
        }
    }

    /// Was daemonize arg given
    pub fn daemonize(&self) -> bool {
        if let Some(daemonize) = self.matches.get_one("daemonize") {
            *daemonize
        } else {
            false
        }
    }

    pub fn plan(&self) -> Plan {
        let mut ops = self.intput_output_args();
        ops.extend(self.template_args());

        if ops.is_empty() {
            Plan::stdio()
        } else {
            Plan::from(ops)
        }
    }

    /// Generate the shell completions and print them to standard output, if requested.
    ///
    /// Will exit after generating the shell completions.
    pub fn generate_shell_completions(&self) {
        if let Some(generator) = self
            .matches
            .get_one::<Shell>("print-shell-completions")
            .copied()
        {
            let mut cmd = command();
            log::info!("Generating completion file for {generator}");
            print_completions(generator, &mut cmd);
            std::process::exit(0);
        }
    }

    pub fn verbosity(&self) -> log::LevelFilter {
        let verbose = self.matches.get_count("verbose") as u16;
        let quiet = self.matches.get_count("quiet") as u16;

        match i16::try_from(verbose)
            .unwrap()
            .checked_sub(quiet.try_into().unwrap())
            .expect("Shenanigans!")
        {
            i if i < -2 => log::LevelFilter::Off,
            -2 => log::LevelFilter::Error,
            -1 => log::LevelFilter::Warn,
            0 => log::LevelFilter::Info,
            1 => log::LevelFilter::Debug,
            _ => log::LevelFilter::Trace,
        }
    }
}

fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut std::io::stdout());
}

fn command() -> Command {
    Command::new("contemplate")
        .about("The friendly cloud-native config templating tool")
        .author("infra.run")
        .version(build::CLAP_LONG_VERSION)
        .arg(
            Arg::new("in-place")
                .short('i')
                .long("in-place")
                .help("edit files in place.")
                .long_help(indoc! {
                    "Edit files in-place.

                    Specified input files are overwritten with the templated output. If SUFFIX is
                    specified, a backup of each input is made by appending SUFFIX to the respective
                    file name.

                    When specified, multiple positional arguments are valid."
                })
                .action(ArgAction::Set)
                .value_name("SUFFIX")
                .value_hint(ValueHint::Other)
                .require_equals(true)
                .num_args(0..=1),
        )
        .arg(
            Arg::new("diff")
                .long("diff")
                .help("Log diffs to standard error")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .short('n')
                .help("Don't write to any files")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .alias("env")
                .help("Take values from the environment")
                .long_help(indoc! {
                    "Take values from environment variables.
                    
                    If PREFIX is specified, only environment variables starting with PREFIX will be
                    passed to the template. The PREFIX will be stripped from the variable names.
                    
                    Can be specified multiple times with distinct PREFIX values."
                })
                .num_args(0..=1)
                .value_name("PREFIX")
                .default_missing_value("")
                .value_hint(ValueHint::Other)
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("k8s-namespace")
                .long("k8s-namespace")
                .alias("ns")
                .help("Specify a k8s namespace to use")
                .long_help(indoc! {
                    "When using k8s datasources, specify a namespace to use"
                })
                .value_name("NAME")
                .value_hint(ValueHint::Other)
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("k8s-configmap")
                .long("k8s-configmap")
                .alias("cm")
                .help("Add a kubernetes configmap as data source")
                .long_help(indoc! {
                    "Add a kubernetes configmap as a data source for template variables.
                    A kubernetes service account credential needs to be present in
                    /var/run/secrets/kubernetes.io/serviceaccount/token.
                    
                    Can be specified multiple times to add multiple config maps"
                })
                .value_name("NAME")
                .value_hint(ValueHint::Other)
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("k8s-secret")
                .long("k8s-secret")
                .help("Add a kubernetes secret as data source")
                .long_help(indoc! {
                    "Add a kubernetes secret as a data source for template variables.
                    A kubernetes service account credential needs to be present in
                    /var/run/secrets/kubernetes.io/serviceaccount/token.

                    Can be specified multiple times to add multiple secret"
                })
                .value_name("NAME")
                .value_hint(ValueHint::Other)
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("file")
                .short('f')
                .long("file")
                .help("Add a file as a data source")
                .long_help(indoc! {
                    "Add a file as a data source. The file must be a valid JSON, YAML, TOML, ini,
                    JSON5 or RON file. The file format is guessed using its file extension.
                    
                    Can be specified multiple times to add multiple file data sources"
                })
                .value_name("PATH")
                .value_hint(ValueHint::FilePath)
                .action(ArgAction::Append),
        )
        .group(
            ArgGroup::new("datasources")
                .args(["k8s-configmap", "k8s-secret", "environment", "file"])
                .multiple(true),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .help("Set the output file")
                .long_help(indoc! {
                    "Set the output file. Defaults to '-', which signifies standard output.
                    
                    Cannot be specified if multiple templates are specified."
                })
                .value_name("PATH")
                .value_hint(ValueHint::FilePath)
                .default_missing_value("-")
                .action(ArgAction::Set)
                .conflicts_with("template")
                .conflicts_with("in-place"),
        )
        .arg(
            Arg::new("template")
                .long("template")
                .short('t')
                .help("Specify a file to template.")
                .long_help(indoc! {
                    "Specify a file to template.
                    
                    Can be passed multiple times to pass multiple template files.
                    The OUTPUT variable is optional, and defaults to '-', or standard output
                    or, if in-place editing is enabled, the original path.
                    
                    It is an error to pass standard output as the OUTPUT for multiple template
                    files."
                })
                .value_names(["INPUT", "OUTPUT"])
                .value_hint(ValueHint::FilePath)
                .num_args(1..=2)
                .action(ArgAction::Append)
                .conflicts_with("output")
                .conflicts_with("input"),
        )
        .arg(
            Arg::new("input")
                .value_name("TEMPLATE")
                .value_hint(ValueHint::FilePath)
                .help("Specify input files to template.")
                .long_help(indoc! {
                    "Specify an input file to template.
                    Multiple input files can be passed when in-place editing is enabled. To specify
                    multiple input files along with respective output paths, use the --template
                    option instead."
                })
                .num_args(1..)
                .conflicts_with("template"),
        )
        .arg(
            Arg::new("on-reload-command")
                .long("on-reload-command")
                .short('r')
                .value_name("COMMAND")
                .value_hint(ValueHint::CommandString)
                .help("Execute the specified shell command on reload")
                .long_help(indoc! {
                    "Execute the specified shell command on reload.
                    
                    The path to the templated config files is specified in
                    the CONTEMPLATED_FILES environment variable.
                    If the command is still running while a new change is
                    detected, the SIGINT signal will be sent to it before re-executing,
                    enabling the downstream hook to debounce changes
                    by sleeping.
                    
                    Example: 'killall -SIGHUP nginx'"
                })
                .num_args(1),
        )
        .arg(
            Arg::new("on-reload-exec")
                .long("on-reload-exec")
                .short('R')
                .value_name("EXECUTABLE")
                .value_hint(ValueHint::ExecutablePath)
                .help("Execute the specified executable on reload without a shell")
                .long_help(indoc! {
                    "Execute the specified executable on reload without a shell

                    See -r for environment variables and signals.
                    
                    Example: '/usr/local/bin/reload-nginx'"
                }),
        )
        .arg(
            Arg::new("on-reload-signal")
                .long("on-reload-signal")
                .value_names(["SIGNAL", "PID|PROCNAME"])
                .help("On reload, send a signal to the specified PID or process name.")
                .long_help(indoc! {
                    "On reload, send a signal to the specified PID or process name.

                    The special process name ':parent' can be used to specify the
                    parent process when -x is specified to signal the executed process.
                    This is the default when no PID or process name is given."
                })
                .num_args(1..=2),
        )
        .group(ArgGroup::new("on-reload").args([
            "on-reload-command",
            "on-reload-exec",
            "on-reload-signal",
        ]))
        .arg(
            Arg::new("and-then-exec")
                .long("and-then-exec")
                .short('x')
                .help("Execute the given executable instead of exiting")
                .long_help(indoc! {
                    "Execute the given executable instead of exiting.

                    This argument takes a variable amount of values. The first value
                    is the target executable, and any following values are passed
                    verbatim as arguments up to the delimiter ';'.

                    Recommended for use in scratch containers,
                    where an entrypoint script cannot be used to
                    start the real entrypoint."
                })
                .action(ArgAction::Set)
                .num_args(1..)
                .value_hint(ValueHint::CommandName)
                .value_terminator(";")
                .allow_hyphen_values(true),
        )
        .arg(
            Arg::new("print-shell-completions")
                .long("print-shell-completions")
                .action(ArgAction::Set)
                .value_name("SHELL")
                .help("Print shell completions")
                .exclusive(true)
                .value_parser(value_parser!(Shell)),
        )
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .short('v')
                .action(ArgAction::Count)
                .help("Increase verbosity level. Can be specified multiple times.")
                .conflicts_with("quiet"),
        )
        .arg(
            Arg::new("quiet")
                .long("quiet")
                .short('q')
                .action(ArgAction::Count)
                .help("Suppress verbose output. Can be specified multiple times.")
                .conflicts_with("verbose"),
        )
        .arg(
            Arg::new("watch")
                .long("watch")
                .short('w')
                .action(ArgAction::SetTrue)
                .help("Re-render templates when data sources change"),
        )
        .arg(
            Arg::new("daemonize")
                .long("daemonize")
                .short('d')
                .action(ArgAction::SetTrue)
                .help("Run as a daemon")
                .requires("watch"),
        )
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum InPlace {
    No,
    WithoutSuffix,
    WithSuffix(String),
}

impl InPlace {
    fn extension(&self) -> Option<&str> {
        match self {
            InPlace::No => None,
            InPlace::WithoutSuffix => None,
            InPlace::WithSuffix(suffix) => Some(suffix.as_str()),
        }
    }
}

impl From<InPlace> for bool {
    fn from(value: InPlace) -> Self {
        match value {
            InPlace::No => false,
            InPlace::WithoutSuffix => true,
            InPlace::WithSuffix(_) => true,
        }
    }
}

impl From<&InPlace> for bool {
    fn from(value: &InPlace) -> Self {
        match value {
            InPlace::No => false,
            InPlace::WithoutSuffix => true,
            InPlace::WithSuffix(_) => true,
        }
    }
}

// Utility function to check whether an iterator has unique elements
fn elements_are_unique<T>(iter: T) -> bool
where
    T: IntoIterator,
    T::Item: Eq + Hash,
{
    let mut uniq = HashSet::new();
    iter.into_iter().all(move |x| uniq.insert(x))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_in_place_suffix() {
        let cli = Cli::new_from(vec!["contemplate"]).unwrap();
        assert_eq!(cli.in_place(), InPlace::No);

        let cli = Cli::new_from(vec!["contemplate", "--in-place"]).unwrap();
        assert_eq!(cli.in_place(), InPlace::WithoutSuffix);

        let cli = Cli::new_from(vec!["contemplate", "--in-place=sfx"]).unwrap();
        assert_eq!(cli.in_place(), InPlace::WithSuffix("sfx".into()));
    }

    #[test]
    fn test_input_args() {
        let cli = Cli::new_from(vec!["contemplate"]).unwrap();
        assert_eq!(cli.intput_output_args(), vec![]);

        let cli = Cli::new_from(vec!["contemplate", "in"]).unwrap();
        assert_eq!(
            cli.intput_output_args(),
            vec![TemplateOperation::new(
                TemplateSource::FileSystem(PathBuf::from("in")),
                TemplateDestination::StdOut
            )]
        );
    }

    #[test]
    fn test_output_arg() {
        let cli = Cli::new_from(vec!["contemplate", "--output", "out"]).unwrap();
        assert_eq!(
            cli.intput_output_args(),
            vec![TemplateOperation::new(
                TemplateSource::StdIn,
                TemplateDestination::FileSystem(PathBuf::from("out"))
            )]
        );
    }

    #[test]
    fn test_input_output_args() {
        let cli = Cli::new_from(vec!["contemplate", "--output", "out", "in"]).unwrap();
        assert_eq!(
            cli.intput_output_args(),
            vec![TemplateOperation::new(
                TemplateSource::FileSystem(PathBuf::from("in")),
                TemplateDestination::FileSystem(PathBuf::from("out"))
            )]
        );
    }

    #[test]
    fn test_template_args() {
        let cli = Cli::new_from(vec!["contemplate", "--template", "in", "out"]).unwrap();
        assert_eq!(cli.intput_output_args(), vec![]);
    }

    #[test]
    fn template_input_output_args_in_place() {
        let cli = Cli::new_from(vec!["contemplate", "--in-place", "--", "in1", "in2"]).unwrap();
        assert_eq!(
            cli.intput_output_args(),
            vec![
                TemplateOperation::new(
                    TemplateSource::FileSystem(PathBuf::from("in1")),
                    TemplateDestination::FileSystem(PathBuf::from("in1"))
                ),
                TemplateOperation::new(
                    TemplateSource::FileSystem(PathBuf::from("in2")),
                    TemplateDestination::FileSystem(PathBuf::from("in2"))
                )
            ]
        );
    }

    #[test]
    fn template_args() {
        let cli = Cli::new_from(vec![
            "contemplate",
            "--template",
            "in1",
            "out1",
            "--template",
            "in2",
            "out2",
        ])
        .unwrap();
        assert_eq!(
            cli.template_args(),
            vec![
                TemplateOperation::new(
                    TemplateSource::FileSystem(PathBuf::from("in1")),
                    TemplateDestination::FileSystem(PathBuf::from("out1"))
                ),
                TemplateOperation::new(
                    TemplateSource::FileSystem(PathBuf::from("in2")),
                    TemplateDestination::FileSystem(PathBuf::from("out2"))
                )
            ]
        );

        let cli: Cli = Cli::new_from(vec!["contemplate", "--template", "in"]).unwrap();
        assert_eq!(
            cli.template_args(),
            vec![TemplateOperation::new(
                TemplateSource::FileSystem(PathBuf::from("in")),
                TemplateDestination::StdOut
            )]
        );
    }

    #[test]
    fn template_args_in_place() {
        let cli: Cli =
            Cli::new_from(vec!["contemplate", "--in-place", "--template", "in"]).unwrap();
        assert_eq!(
            cli.template_args(),
            vec![TemplateOperation::new(
                TemplateSource::FileSystem(PathBuf::from("in")),
                TemplateDestination::FileSystem(PathBuf::from("in")),
            )]
        );
    }

    #[test]
    fn no_template_args_and_positional_args() {
        assert!(Cli::new_from(vec!["contemplate", "--template", "in1", "--", "in2"]).is_err());
        assert!(Cli::new_from(vec!["contemplate", "--template", "in1", "out1", "in2"]).is_err());
        assert!(Cli::new_from(vec![
            "contemplate",
            "--template",
            "in1",
            "out1",
            "--output",
            "out2"
        ])
        .is_err());
    }

    #[test]
    fn no_duplicate_destinations() {
        assert!(Cli::new_from(vec!["contemplate", "in1", "in2"]).is_err());
        assert!(Cli::new_from(vec!["contemplate", "--output", "out", "in1", "in2"]).is_err());
        assert!(Cli::new_from(vec![
            "contemplate",
            "--template",
            "in1",
            "--template",
            "in2"
        ])
        .is_err());
        assert!(Cli::new_from(vec![
            "contemplate",
            "--template",
            "in1",
            "out",
            "--template",
            "in2",
            "out"
        ])
        .is_err());
    }

    #[test]
    fn no_watch_to_stdout() {
        assert!(Cli::new_from(vec!["contemplate", "--watch"]).is_err());
        assert!(Cli::new_from(vec!["contemplate", "--watch", "--template", "-", "-"]).is_err());
    }
}
