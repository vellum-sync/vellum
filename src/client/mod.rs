use log::{debug, info};
use uuid::Uuid;

use crate::{
    api::{self, Connection},
    config::Config,
    error::Result,
    process::{server_is_running, wait_for_server_exit},
    server,
};

mod filter;
mod history;
mod import;
mod r#move;
mod session;

pub use history::*;
pub use import::*;
pub use r#move::*;

use filter::*;
use session::*;

pub fn store(cfg: &Config, cmd: String) -> Result<()> {
    if cmd.is_empty() {
        return Ok(());
    }
    server::ensure_ready(cfg)?;
    let mut conn = Connection::new(cfg)?;
    conn.store(cmd, Session::get()?.id)
}

pub fn stop_server(cfg: &Config, no_sync: bool) -> Result<()> {
    if !server_is_running(cfg)? {
        debug!("server isn't running");
        return Ok(());
    }
    debug!("server is running");
    let mut conn = Connection::new(cfg)?;
    conn.exit(no_sync)?;
    debug!("wait for server exit");
    wait_for_server_exit(cfg)
}

pub fn sync(cfg: &Config, force: bool) -> Result<()> {
    server::ensure_ready(cfg)?;
    let mut conn = Connection::new(cfg)?;
    conn.sync(force)
}

pub fn ping(cfg: &Config, wait: bool) -> Result<()> {
    api::ping(cfg, wait)?;
    info!("got pong from server");
    Ok(())
}

pub fn delete(cfg: &Config, ids: Vec<String>) -> Result<()> {
    server::ensure_ready(cfg)?;
    let mut conn = Connection::new(cfg)?;
    let session = Session::get()?;
    for id in ids {
        debug!("delete id: {id}");
        let id = Uuid::parse_str(&id)?;
        conn.update(id, "".to_string(), session.id.clone())?;
    }
    Ok(())
}
