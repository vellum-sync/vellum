use std::env;

use crate::{api::Connection, config::Config, error::Result};

fn get_session() -> String {
    match env::var("VELLUM_SESSION") {
        Ok(s) => s,
        Err(_) => "NO-SESSION".to_string(),
    }
}

pub fn store(cfg: &Config, cmd: String) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.store(cmd, get_session())
}

pub fn stop_server(cfg: &Config, no_sync: bool) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.exit(no_sync)
}

pub fn history(cfg: &Config, session: bool) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    let history = conn.history_request()?;
    let current_session = get_session();
    for entry in history
        .into_iter()
        .filter(|entry| !session || entry.session == current_session)
    {
        println!("{}", entry.cmd);
    }
    Ok(())
}

pub fn sync(cfg: &Config, force: bool) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.sync(force)
}
