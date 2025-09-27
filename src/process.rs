use std::{
    fs::{exists, read_to_string},
    path::Path,
    process,
};

use log::debug;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::{config::Config, error::Result};

pub fn server_is_running(cfg: &Config) -> Result<bool> {
    let pid_file = Path::new(&cfg.state_dir).join("server.pid");
    debug!("Check server pid file: {pid_file:?}");
    if !exists(&pid_file)? {
        return Ok(false);
    }
    let raw_pid = read_pid_file(pid_file)?;
    debug!("Got server pid: {raw_pid}");
    if raw_pid == 0 || raw_pid == process::id() {
        // not running, or re-execing in the same process
        return Ok(false);
    }
    let pid = Pid::from_u32(raw_pid);

    let mut s = System::new();
    s.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );

    match s.process(pid) {
        Some(p) => Ok(p.name().to_string_lossy().contains("vellum")),
        None => Ok(false),
    }
}

pub fn wait_for_server_exit(cfg: &Config) -> Result<()> {
    let pid_file = Path::new(&cfg.state_dir).join("server.pid");
    let pid = Pid::from_u32(read_pid_file(pid_file)?);

    let mut s = System::new();
    s.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );

    if let Some(p) = s.process(pid) {
        p.wait();
    };

    Ok(())
}

fn read_pid_file<P: AsRef<Path>>(path: P) -> Result<u32> {
    let buf = read_to_string(path)?;
    if buf.is_empty() {
        return Ok(0);
    }
    let pid: u32 = buf.parse()?;
    Ok(pid)
}
