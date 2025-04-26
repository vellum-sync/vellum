use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{BufRead, BufReader, Write, stdin, stdout},
    path::{Path, PathBuf},
    process::{Command, exit},
};

use log::{debug, info, warn};
use tempfile::NamedTempFile;
use uuid::Uuid;
use which::which;

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
    let filter = Filter::new(args.filter)?;
    let mut conn = Connection::new(cfg)?;
    let history: Vec<Entry> = filter.history_request(&mut conn)?;

    if history.is_empty() {
        warn!("No history commands matched the provided options");
        return Ok(());
    }

    let changes = edit_history(&cfg.cache_dir, history)?;
    match changes.len() {
        0 => {
            info!("no entries modified");
            return Ok(());
        }
        1 => info!("1 entry modified"),
        n => info!("{n} entries modified"),
    };

    if !args.quiet {
        show_changes(&changes);
    }

    if args.dry_run {
        return Ok(());
    }

    if !args.force && !confirm_changes()? {
        info!("changes aborted");
        exit(0);
    }

    let session = Session::get()?.id;
    for entry in changes {
        conn.update(entry.id, entry.cmd, session.clone())?;
    }

    info!("changes saved");

    Ok(())
}

fn edit_history<P: AsRef<Path>>(dir: P, history: Vec<Entry>) -> Result<Vec<Entry>> {
    let temp_file = write_temp_file(dir, &history)?;

    edit_file(temp_file.path())?;

    let edited = parse_file(temp_file.path())?;

    // make sure temp_file exists until we have read the file back in
    drop(temp_file);

    Ok(get_changes(history, edited))
}

fn write_temp_file<P: AsRef<Path>>(dir: P, history: &[Entry]) -> Result<NamedTempFile> {
    fs::create_dir_all(dir.as_ref())?;
    let mut temp_file = NamedTempFile::new_in(dir)?;
    debug!("temp file: {:?}", temp_file.path());
    writeln!(temp_file, "{}", HEADER)?;
    for entry in history {
        writeln!(temp_file, "{}\t{}", entry.id, entry.cmd)?;
    }
    temp_file.flush()?;
    Ok(temp_file)
}

fn edit_file<P: AsRef<Path>>(path: P) -> Result<()> {
    let editor = get_editor()?;
    debug!("edit using {editor:?}");
    let status = Command::new(&editor).arg(path.as_ref()).status()?;
    if !status.success() {
        return Err(Error::Generic(format!("{editor:?} exited with an error")));
    }
    Ok(())
}

fn get_editor() -> Result<PathBuf> {
    if let Ok(editor) = env::var("VELLUM_EDITOR") {
        return Ok(editor.into());
    }
    if let Ok(visual) = env::var("VISUAL") {
        return Ok(visual.into());
    }
    if let Ok(editor) = env::var("EDITOR") {
        return Ok(editor.into());
    }
    if let Ok(nano) = which("nano") {
        return Ok(nano);
    }
    if let Ok(vi) = which("vi") {
        return Ok(vi);
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

fn show_changes(changes: &[Entry]) {
    for entry in changes {
        if &entry.cmd == "" {
            info!("{}: <deleted>", entry.id);
        } else {
            info!("{}: {}", entry.id, entry.cmd);
        }
    }
}

fn confirm_changes() -> Result<bool> {
    let mut buf = String::with_capacity(1024);
    loop {
        print!("Apply changes? (yes/no): ");
        stdout().flush()?;
        buf.clear();
        stdin().read_line(&mut buf)?;
        match buf.trim().to_lowercase().as_str() {
            "yes" => return Ok(true),
            "no" => return Ok(false),
            _ => println!("Please enter \"yes\" or \"no\""),
        }
    }
}
