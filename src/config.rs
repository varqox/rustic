//! Rustic Config
//!
//! See instructions in `commands.rs` to specify the path to your
//! application's configuration file and/or command-line options
//! for specifying it.

pub(crate) mod progress_options;

use std::str::FromStr;
use std::{collections::HashMap, path::PathBuf};

use abscissa_core::config::Config;
use abscissa_core::path::AbsPathBuf;
use abscissa_core::FrameworkError;
use clap::Parser;
use directories::ProjectDirs;
use itertools::Itertools;
use log::{trace, warn, Level};
use merge::Merge;
use rustic_backend::BackendOptions;
use rustic_core::RepositoryOptions;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr, OneOrMany, PickFirst};

#[cfg(feature = "webdav")]
use crate::commands::webdav::WebDavCmd;
use crate::{
    commands::{backup::BackupCmd, copy::Targets, forget::ForgetOptions},
    config::progress_options::ProgressOptions,
    filtering::SnapshotFilter,
};

/// Rustic Configuration
///
/// Further documentation can be found [here](https://github.com/rustic-rs/rustic/blob/main/config/README.md).
///
/// # Example
// TODO: add example
#[derive(Clone, Default, Debug, Parser, Deserialize, Serialize, Merge)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct RusticConfig {
    /// Global options
    #[clap(flatten, next_help_heading = "Global options")]
    pub global: GlobalOptions,

    /// Repository options
    #[clap(flatten, next_help_heading = "Repository options")]
    pub repository: AllRepositoryOptions,

    /// Snapshot filter options
    #[clap(flatten, next_help_heading = "Snapshot filter options")]
    pub snapshot_filter: SnapshotFilter,

    /// Backup options
    #[clap(skip)]
    pub backup: BackupCmd,

    /// Copy options
    #[clap(skip)]
    pub copy: Targets,

    /// Forget options
    #[clap(skip)]
    pub forget: ForgetOptions,

    #[cfg(feature = "webdav")]
    /// webdav options
    #[clap(skip)]
    pub webdav: WebDavCmd,
}

#[derive(Clone, Default, Debug, Parser, Serialize, Deserialize, Merge)]
#[serde(default, rename_all = "kebab-case")]
pub struct AllRepositoryOptions {
    /// Backend options
    #[clap(flatten)]
    #[serde(flatten)]
    pub be: BackendOptions,

    /// Repository options
    #[clap(flatten)]
    #[serde(flatten)]
    pub repo: RepositoryOptions,
}

impl RusticConfig {
    /// Merge a profile into the current config by reading the corresponding config file.
    /// Also recursively merge all profiles given within this config file.
    ///
    /// # Arguments
    ///
    /// * `profile` - name of the profile to merge
    /// * `merge_logs` - Vector to collect logs during merging
    /// * `level_missing` - The log level to use if this profile is missing. Recursive calls will produce a Warning.
    pub fn merge_profile(
        &mut self,
        profile: &str,
        merge_logs: &mut Vec<(Level, String)>,
        level_missing: Level,
    ) -> Result<(), FrameworkError> {
        let profile_filename = profile.to_string() + ".toml";
        let paths = get_config_paths(&profile_filename);

        if let Some(path) = paths.iter().find(|path| path.exists()) {
            merge_logs.push((Level::Info, format!("using config {}", path.display())));
            let mut config = Self::load_toml_file(AbsPathBuf::canonicalize(path)?)?;
            // if "use_profile" is defined in config file, merge the referenced profiles first
            for profile in &config.global.use_profile.clone() {
                config.merge_profile(profile, merge_logs, Level::Warn)?;
            }
            self.merge(config);
        } else {
            let paths_string = paths.iter().map(|path| path.display()).join(", ");
            merge_logs.push((
                level_missing,
                format!(
                    "using no config file, none of these exist: {}",
                    &paths_string
                ),
            ));
        };
        Ok(())
    }
}

/// Global options
///
/// These options are available for all commands.
#[serde_as]
#[derive(Default, Debug, Parser, Clone, Deserialize, Serialize, Merge)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct GlobalOptions {
    /// Config profile to use. This parses the file `<PROFILE>.toml` in the config directory.
    /// [default: "rustic"]
    #[clap(
        short = 'P',
        long,
        global = true,
        value_name = "PROFILE",
        env = "RUSTIC_USE_PROFILE"
    )]
    #[merge(strategy = merge::vec::append)]
    #[serde_as(as = "OneOrMany<_>")]
    pub use_profile: Vec<String>,

    /// Only show what would be done without modifying anything. Does not affect read-only commands.
    #[clap(long, short = 'n', global = true, env = "RUSTIC_DRY_RUN")]
    #[merge(strategy = merge::bool::overwrite_false)]
    pub dry_run: bool,

    /// Check if index matches pack files and read pack headers if neccessary
    #[clap(long, global = true, env = "RUSTIC_CHECK_INDEX")]
    #[merge(strategy = merge::bool::overwrite_false)]
    pub check_index: bool,

    /// Use this log level [default: info]
    #[clap(long, global = true, env = "RUSTIC_LOG_LEVEL")]
    pub log_level: Option<String>,

    /// Write log messages to the given file instead of printing them.
    ///
    /// # Note
    ///
    /// Warnings and errors are still additionally printed unless they are ignored by `--log-level`
    #[clap(long, global = true, env = "RUSTIC_LOG_FILE", value_name = "LOGFILE")]
    pub log_file: Option<PathBuf>,

    /// Settings to customize progress bars
    #[clap(flatten)]
    #[serde(flatten)]
    pub progress_options: ProgressOptions,

    /// List of environment variables to set (only in config file)
    #[clap(skip)]
    #[merge(strategy = extend)]
    pub env: HashMap<String, String>,

    /// Call this command before every rustic operation
    #[clap(long, global = true, env = "RUSTIC_RUN_BEFORE", default_value = "")]
    pub run_before: CommandInput,

    /// Call this command after every rustic operation
    #[clap(long, global = true, env = "RUSTIC_RUN_BEFORE", default_value = "")]
    pub run_after: CommandInput,
}

/// Extend the contents of a [`HashMap`] with the contents of another
/// [`HashMap`] with the same key and value types.
fn extend(left: &mut HashMap<String, String>, right: HashMap<String, String>) {
    left.extend(right);
}

/// Get the paths to the config file
///
/// # Arguments
///
/// * `filename` - name of the config file
///
/// # Returns
///
/// A vector of [`PathBuf`]s to the config files
fn get_config_paths(filename: &str) -> Vec<PathBuf> {
    [
        ProjectDirs::from("", "", "rustic")
            .map(|project_dirs| project_dirs.config_dir().to_path_buf()),
        get_global_config_path(),
        Some(PathBuf::from(".")),
    ]
    .into_iter()
    .filter_map(|path| {
        path.map(|mut p| {
            p.push(filename);
            p
        })
    })
    .collect()
}

/// Get the path to the global config directory on Windows.
///
/// # Returns
///
/// The path to the global config directory on Windows.
/// If the environment variable `PROGRAMDATA` is not set, `None` is returned.
#[cfg(target_os = "windows")]
fn get_global_config_path() -> Option<PathBuf> {
    std::env::var_os("PROGRAMDATA").map(|program_data| {
        let mut path = PathBuf::from(program_data);
        path.push(r"rustic\config");
        path
    })
}

/// Get the path to the global config directory on ios and wasm targets.
///
/// # Returns
///
/// `None` is returned.
#[cfg(any(target_os = "ios", target_arch = "wasm32"))]
fn get_global_config_path() -> Option<PathBuf> {
    None
}

/// Get the path to the global config directory on non-Windows,
/// non-iOS, non-wasm targets.
///
/// # Returns
///
/// "/etc/rustic" is returned.
#[cfg(not(any(target_os = "windows", target_os = "ios", target_arch = "wasm32")))]
fn get_global_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("/etc/rustic"))
}

/// A command to be called which can be given as CLI option as well as in config files
/// `CommandInput` implements Serialize/Deserialize as well as FromStr.

// Note: we use CommandInputInternal here which itself impls FromStr in order to use serde_as PickFirst for CommandInput.
#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize, Merge)]
pub struct CommandInput(#[serde_as(as = "PickFirst<(_,DisplayFromStr)>")] CommandInputInternal);

impl CommandInput {
    pub fn is_set(&self) -> bool {
        !self.0 .0.is_empty()
    }

    pub fn command(&self) -> &str {
        &self.0 .0[0]
    }

    pub fn args(&self) -> &[String] {
        &self.0 .0[1..]
    }

    pub fn run(&self, info: &str) -> Result<(), std::io::Error> {
        if !self.is_set() {
            trace!("not calling command {info} - not set");
            return Ok(());
        }
        trace!("calling command {info}: {self:?}");
        let status = std::process::Command::new(self.command())
            .args(self.args())
            .status()?;
        if !status.success() {
            warn!("running command {info} was not successful. {status}");
        }
        Ok(())
    }
}

impl FromStr for CommandInput {
    type Err = shell_words::ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(CommandInputInternal::from_str(s)?))
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Merge)]
struct CommandInputInternal(#[merge(strategy = merge::vec::overwrite_empty)] Vec<String>);

impl FromStr for CommandInputInternal {
    type Err = shell_words::ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let vec = shell_words::split(s)?;
        Ok(Self(vec))
    }
}
