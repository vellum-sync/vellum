use std::{env, fs, io, path::Path, process::exit};

use clap::{Parser, Subcommand};
use env_logger::Target;
use log::error;

mod api;
mod assets;
mod client;
mod config;
mod error;
mod history;
mod init;
mod process;
mod server;
mod sync;

pub const CLAP_STYLING: clap::builder::styling::Styles = clap::builder::styling::Styles::styled()
    .header(clap_cargo::style::HEADER)
    .usage(clap_cargo::style::USAGE)
    .literal(clap_cargo::style::LITERAL)
    .placeholder(clap_cargo::style::PLACEHOLDER)
    .error(clap_cargo::style::ERROR)
    .valid(clap_cargo::style::VALID)
    .invalid(clap_cargo::style::INVALID);

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(styles = CLAP_STYLING)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Store a shell command in the history
    Store {
        /// the shell command to be stored
        shell_command: String,
    },

    /// List all the stored commands
    History(client::HistoryArgs),

    /// Move through the history relative to a given point
    Move(client::MoveArgs),

    /// Display the vellum configuration
    Config,

    /// Commands to setup/initialise vellum
    Init(init::Args),

    /// Ping the server
    Ping {
        /// Wait for the ping to succeed
        #[arg(short, long)]
        wait: bool,
    },

    /// Request the server sync the history immediately
    Sync {
        /// Force a sync, even if things seem to be up-to-date
        #[arg(short, long)]
        force: bool,
    },

    /// Run the background history management server
    Server(server::Args),

    /// Stop the currently running server
    Stop {
        /// Don't sync the history before exiting
        #[arg(short, long)]
        no_sync: bool,
    },
}

fn create_log_file(log_file: &str) -> io::Result<fs::File> {
    fs::create_dir_all(Path::new(log_file).parent().unwrap())?;
    fs::File::options().append(true).create(true).open(log_file)
}

fn log_target() -> Target {
    if let Ok(log_file) = env::var("VELLUM_LOG_FILE") {
        return match create_log_file(&log_file) {
            Ok(f) => Target::Pipe(Box::new(f)),
            Err(e) => panic!("Failed to open log file {log_file}: {e}"),
        };
    }
    Target::Stderr
}

fn main() {
    env_logger::Builder::from_env("VELLUM_LOG")
        .target(log_target())
        .init();

    let cli = Cli::parse();

    let config = match config::Config::load(cli.config.as_ref()) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {e}");
            exit(1);
        }
    };

    if let Err(e) = match cli.command {
        Commands::Store { shell_command } => client::store(&config, shell_command),
        Commands::History(args) => client::history(&config, args),
        Commands::Move(args) => client::do_move(&config, args),
        Commands::Config => config.show(),
        Commands::Init(args) => init::init(args),
        Commands::Ping { wait } => client::ping(&config, wait),
        Commands::Sync { force } => client::sync(&config, force),
        Commands::Server(args) => server::run(&config, args),
        Commands::Stop { no_sync } => client::stop_server(&config, no_sync),
    } {
        error!("{e}");
        exit(1);
    }
}
