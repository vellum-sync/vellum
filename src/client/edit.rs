use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{BufRead, BufReader, Write, stdin},
    path::Path,
    process::Command,
};

use log::{debug, info, warn};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{
    api::Connection,
    config::Config,
    error::{Error, Result},
    history::Entry,
};

use super::{Filter, FilterArgs, Session};

const HEADER: &'static str = r#"# This file lists the commands that matched the provided options.
#
# Lines starting with '#' and blank lines are ignored, otherwise each line
# consists of an ID and the command, separated by a tab. Any other lines will
# cause an error.
#
# To edit an entry simply change the command, to delete an entry remove the
# line.
#
# If an ID is edited then it will be ignored if it was not originally selected
# for editing (i.e. only IDs in the file as originally written will be
# processed, unrecognised IDs will be ignored - probably resulting in commands
# being deleted).
#
"#;

#[derive(clap::Args, Debug)]
pub struct EditArgs {
    #[command(flatten)]
    filter: FilterArgs,

    /// Don't ask for confirmation before applying changes
    #[arg(short, long)]
    force: bool,

    /// Display the changes that would be made, but don't actually make them
    #[arg(short, long)]
    dry_run: bool,

    /// Don't show the changes being made
    #[arg(short, long)]
    quiet: bool,
}

pub fn edit(cfg: &Config, args: EditArgs) -> Result<()> {
    let session = Session::get()?.id;
    let filter = Filter::new(args.filter)?;
    let mut conn = Connection::new(cfg)?;
    let history: Vec<Entry> = filter.history_request(&mut conn)?;

    if history.is_empty() {
        warn!("No history commands matched the provided options");
        return Ok(());
    }

    fs::create_dir_all(&cfg.cache_dir)?;
    let mut temp_file = NamedTempFile::new_in(&cfg.cache_dir)?;
    writeln!(temp_file, "{}", HEADER)?;
    for entry in history.iter() {
        writeln!(temp_file, "{}\t{}", entry.id, entry.cmd)?;
    }
    temp_file.flush()?;

    debug!("temp file: {:?}", temp_file.path());

    let editor = get_editor()?;
    debug!("edit using {editor}");
    let status = Command::new(&editor).arg(temp_file.path()).status()?;
    if !status.success() {
        return Err(Error::Generic(format!("{editor} exited with an error")));
    }

    let edited = parse_file(temp_file.path())?;

    // make sure temp_file exists until we have read the file back in
    drop(temp_file);

    let changes = get_changes(history, edited);
    match changes.len() {
        0 => {
            info!("no entries modified");
            return Ok(());
        }
        1 => info!("1 entry modified"),
        n => info!("{n} entries modified"),
    };

    if !args.quiet {
        for entry in changes.iter() {
            if &entry.cmd == "" {
                info!("{}: <deleted>", entry.id);
            } else {
                info!("{}: {}", entry.id, entry.cmd);
            }
        }
    }

    if args.dry_run {
        return Ok(());
    }

    if !args.force {
        println!("Press enter to apply change, Ctrl-C to abort:");
        let mut buf = String::with_capacity(1024);
        stdin().read_line(&mut buf)?;
    }

    for entry in changes {
        conn.update(entry.id, entry.cmd, session.clone())?;
    }

    Ok(())
}

fn get_editor() -> Result<String> {
    if let Ok(editor) = env::var("VELLUM_EDITOR") {
        return Ok(editor);
    }
    if let Ok(visual) = env::var("VISUAL") {
        return Ok(visual);
    }
    if let Ok(editor) = env::var("EDITOR") {
        return Ok(editor);
    }
    Err(Error::Generic("unable to find editor".to_string()))
}

fn parse_file<P: AsRef<Path>>(path: P) -> Result<HashMap<Uuid, String>> {
    let mut entries = HashMap::new();
    for line in BufReader::new(File::open(path)?).lines() {
        let line = line?;
        if line.starts_with("#") || line.is_empty() {
            continue;
        }
        let (id, cmd) = match line.split_once('\t') {
            Some(s) => s,
            None => {
                return Err(Error::Generic(format!(
                    "line does not start with ID: {line}"
                )));
            }
        };
        let id = Uuid::parse_str(id)?;
        entries.insert(id, cmd.to_string());
    }
    Ok(entries)
}

fn get_changes(history: Vec<Entry>, edited: HashMap<Uuid, String>) -> Vec<Entry> {
    let mut changes = Vec::new();
    for mut entry in history {
        let changed = match edited.get(&entry.id) {
            Some(cmd) => {
                if &entry.cmd == cmd {
                    continue;
                } else {
                    cmd.as_str()
                }
            }
            None => "",
        };
        if changed.is_empty() {
            debug!("Entry {} was deleted", entry.id);
        } else {
            debug!("Entry {} changed to: {}", entry.id, changed);
        }
        entry.cmd = changed.to_string();
        changes.push(entry);
    }
    changes
}
