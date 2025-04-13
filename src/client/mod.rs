use crate::{api::Connection, config::Config, error::Result};

pub fn store(cfg: &Config, cmd: String) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.store(cmd)
}

pub fn stop_server(cfg: &Config) -> Result<()> {
    let mut conn = Connection::new(cfg)?;
    conn.exit()
}
