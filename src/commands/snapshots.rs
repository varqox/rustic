use std::time::Duration;

use anyhow::Result;
use bytesize::ByteSize;
use clap::Parser;
use humantime::format_duration;
use prettytable::{cell, format, row, Table};

use crate::backend::DecryptReadBackend;
use crate::repo::{SnapshotFile, SnapshotFilter, SnapshotGroup, SnapshotGroupCriterion};

#[derive(Parser)]
pub(super) struct Opts {
    #[clap(flatten)]
    filter: SnapshotFilter,

    /// group snapshots by any combination of host,paths,tags
    #[clap(long, short = 'g', value_name = "CRITERION", default_value = "")]
    group_by: SnapshotGroupCriterion,

    /// show detailed information about snapshots
    #[clap(long)]
    long: bool,

    /// Snapshots to list
    #[clap(value_name = "ID")]
    ids: Vec<String>,
}

pub(super) async fn execute(be: &impl DecryptReadBackend, opts: Opts) -> Result<()> {
    let groups = match opts.ids.is_empty() {
        true => SnapshotFile::group_from_backend(be, &opts.filter, &opts.group_by).await?,
        false => vec![(
            SnapshotGroup::default(),
            SnapshotFile::from_ids(be, &opts.ids).await?,
        )],
    };
    let bytes = |b| ByteSize(b).to_string_as(true);

    for (group, mut snapshots) in groups {
        if !group.is_empty() {
            println!("\nsnapshots for {:?}", group);
        }
        snapshots.sort_unstable();
        let count = snapshots.len();

        if opts.long {
            for snap in snapshots {
                display_snap(snap);
            }
        } else {
            let mut table: Table = snapshots
                .into_iter()
                .map(|sn| {
                    let tags = sn.tags.formatln();
                    let paths = sn.paths.formatln();
                    let time = sn.time.format("%Y-%m-%d %H:%M:%S");
                    let (files, dirs, size) = sn
                        .summary
                        .map(|s| {
                            (
                                s.total_files_processed.to_string(),
                                s.total_dirs_processed.to_string(),
                                bytes(s.total_bytes_processed),
                            )
                        })
                        .unwrap_or_else(|| ("?".to_string(), "?".to_string(), "?".to_string()));
                    row![sn.id, time, sn.hostname, tags, paths, r->files, r->dirs, r->size]
                })
                .collect();
            table.set_titles(
                row![b->"ID", b->"Time", b->"Host", b->"Tags", b->"Paths", br->"Files",br->"Dirs", br->"Size"],
            );
            table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
            table.printstd();
        }
        println!("{} snapshot(s)", count);
    }

    Ok(())
}

fn display_snap(sn: SnapshotFile) {
    let mut table = Table::new();
    let bytes = |b| ByteSize(b).to_string_as(true);

    table.add_row(row![b->"Snapshot", b->sn.id.to_hex()]);
    table.add_row(row![b->"Time", sn.time.format("%Y-%m-%d %H:%M:%S")]);
    table.add_row(row![b->"Host", sn.hostname]);
    table.add_row(row![b->"Tags", sn.tags.formatln()]);
    table.add_row(row![b->"Paths", sn.paths.formatln()]);
    if let Some(summary) = sn.summary {
        table.add_row(row![]);
        table.add_row(row![b->"Command", summary.command]);

        let source = format!(
            "files: {} / dirs: {} / size: {}",
            summary.total_files_processed,
            summary.total_dirs_processed,
            bytes(summary.total_bytes_processed)
        );
        table.add_row(row![b->"Source", source]);

        table.add_row(row![]);

        let files = format!(
            "new: {:>10} / changed: {:>10} / unchanged: {:>10}",
            summary.files_new, summary.files_changed, summary.files_unmodified,
        );
        table.add_row(row![b->"Files", files]);

        let trees = format!(
            "new: {:>10} / changed: {:>10} / unchanged: {:>10}",
            summary.dirs_new, summary.dirs_changed, summary.dirs_unmodified,
        );
        table.add_row(row![b->"Dirs", trees]);

        table.add_row(row![]);

        let written = format!(
            "data:  {:>10} blobs / {}\ntree:  {:>10} blobs / {}\ntotal: {:>10} blobs / {}",
            summary.data_blobs,
            bytes(summary.data_files_added),
            summary.tree_blobs,
            bytes(summary.data_trees_added),
            summary.tree_blobs + summary.data_blobs,
            bytes(summary.data_added),
        );
        table.add_row(row![b->"Added to repo", written]);

        let duration = format!(
            "backup start: {} / backup end: {} / backup duration: {}\ntotal duration: {}",
            summary.backup_start.format("%Y-%m-%d %H:%M:%S"),
            summary.backup_end.format("%Y-%m-%d %H:%M:%S"),
            format_duration(Duration::from_secs_f64(summary.backup_duration)),
            format_duration(Duration::from_secs_f64(summary.total_duration))
        );
        table.add_row(row![b->"Duration", duration]);
    }
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.printstd();
    println!();
}
