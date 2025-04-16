use crate::{api::Connection, config::Config, error::Result};

pub fn store(cfg: &Config, cmd: String) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.store(cmd)
}

pub fn stop_server(cfg: &Config, no_sync: bool) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.exit(no_sync)
}

pub fn history(cfg: &Config) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    let history = conn.history_request()?;
    for command in history {
        println!("{command}")
    }
    Ok(())
}

pub fn sync(cfg: &Config, force: bool) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.sync(force)
}
