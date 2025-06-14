use std::time::Duration;

use clap::crate_version;
use log::{debug, info};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    api::{self, Connection},
    config::Config,
    error::Result,
    process::{server_is_running, wait_for_server_exit},
    server,
};

mod edit;
mod filter;
mod get;
mod history;
mod import;
mod r#move;
mod session;

pub use edit::*;
pub use get::*;
pub use history::*;
pub use import::*;
pub use r#move::*;

use filter::*;
use session::*;

pub fn store(cfg: &Config, cmd: String) -> Result<()> {
    if cmd.is_empty() {
        return Ok(());
    }
    let mut conn = server::ensure_ready(cfg)?;
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
    let mut conn = server::ensure_ready(cfg)?;
    conn.sync(force)
}

pub fn ping(cfg: &Config, wait: bool) -> Result<()> {
    let wait = match wait {
        true => Some(Duration::from_secs(30)),
        false => None,
    };
    api::ping(cfg, wait)?;
    info!("got pong from server");
    Ok(())
}

pub fn delete(cfg: &Config, ids: Vec<String>) -> Result<()> {
    let mut conn = server::ensure_ready(cfg)?;
    let session = Session::get()?;
    for id in ids {
        debug!("delete id: {id}");
        let id = Uuid::parse_str(&id)?;
        conn.update(id, "".to_string(), session.id.clone())?;
    }
    Ok(())
}

pub fn rebuild(cfg: &Config) -> Result<()> {
    let mut conn = server::ensure_ready(cfg)?;
    for status in conn.rebuild()? {
        let status = status?;
        info!("{status}");
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct Version {
    client: String,
    server: String,
}

pub fn version(cfg: &Config, json: bool) -> Result<()> {
    let mut conn = server::ensure_ready(cfg)?;
    let server_version = conn.version_request()?;
    if json {
        let version = Version {
            client: crate_version!().to_string(),
            server: server_version,
        };
        print!("{}", serde_json::to_string(&version)?);
    } else {
        println!("Client: {}", crate_version!());
        println!("Server: {server_version}");
    }
    Ok(())
}
