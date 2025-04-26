use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::Path,
    process::Command,
};

use log::{debug, info};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::{
    api::Connection,
    config::Config,
    error::{Error, Result},
    history::Entry,
};

use super::{Filter, FilterArgs, Session};

#[derive(clap::Args, Debug)]
pub struct EditArgs {
    #[command(flatten)]
    filter: FilterArgs,
}

pub fn edit(cfg: &Config, args: EditArgs) -> Result<()> {
    let session = Session::get()?.id;
    let filter = Filter::new(args.filter)?;
    let mut conn = Connection::new(cfg)?;
    let history: Vec<Entry> = filter.history_request(&mut conn)?;

    fs::create_dir_all(&cfg.cache_dir)?;
    let mut temp_file = NamedTempFile::new_in(&cfg.cache_dir)?;
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

    let mut modified = 0;
    for entry in history {
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
        modified += 1;
        if changed.is_empty() {
            debug!("Entry {} was deleted", entry.id);
            conn.update(entry.id, "".to_string(), session.clone())?;
            continue;
        }
        debug!("Entry {} changed to: {}", entry.id, changed);
        conn.update(entry.id, changed.to_string(), session.clone())?;
    }

    info!("{modified} entries modified");

    // make sure temp_file exists to the end of the function
    drop(temp_file);
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
        if line.starts_with("#") {
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
