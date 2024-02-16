//! `lock` subcommand

use std::str::FromStr;

use crate::{commands::open_repository, status_err, Application, RUSTIC_APP};
use abscissa_core::{Command, Runnable, Shutdown};

use anyhow::Result;
use chrono::{DateTime, Duration, Local};

use rustic_core::LockOptions;

/// `prune` subcommand
#[allow(clippy::struct_excessive_bools)]
#[derive(clap::Parser, Command, Debug, Clone)]
pub(crate) struct LockCmd {
    /// Extend locks even if the files are already locked long enough
    #[clap(long)]
    always_extend_lock: bool,

    #[clap(long)]
    /// Duration for how long to extend the locks (e.g. "10d"). "forever" is also allowed
    duration: LockDuration,

    /// Snapshots to lock. If none is given, use filter options to filter from all snapshots
    #[clap(value_name = "ID")]
    ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct LockDuration(Option<DateTime<Local>>);

impl FromStr for LockDuration {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "forever" => Ok(Self(None)),
            d => {
                let duration = humantime::Duration::from_str(d)?;
                let duration = Duration::from_std(*duration)?;
                Ok(Self(Some(Local::now() + duration)))
            }
        }
    }
}

impl Runnable for LockCmd {
    fn run(&self) {
        if let Err(err) = self.inner_run() {
            status_err!("{}", err);
            RUSTIC_APP.shutdown(Shutdown::Crash);
        };
    }
}

impl LockCmd {
    fn inner_run(&self) -> Result<()> {
        let config = RUSTIC_APP.config();
        let repo = open_repository(&config.repository)?;

        let snapshots = if self.ids.is_empty() {
            repo.get_matching_snapshots(|sn| config.snapshot_filter.matches(sn))?
        } else {
            repo.get_snapshots(&self.ids)?
        };

        if config.global.dry_run {
            println!("lock is not supported in dry-run mode");
        } else {
            let lock_opts = LockOptions::default()
                .always_extend_lock(self.always_extend_lock)
                .until(self.duration.0);

            repo.lock(&lock_opts, &snapshots)?;
        }

        Ok(())
    }
}
