use std::{env, fs, io, path::Path, process::exit};

use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use env_logger::{Env, Target};
use log::error;

mod api;
mod assets;
mod client;
mod complete;
mod config;
mod error;
mod history;
mod init;
mod process;
mod server;
mod sync;

const CLAP_STYLING: clap::builder::styling::Styles = clap::builder::styling::Styles::styled()
    .header(clap_cargo::style::HEADER)
    .usage(clap_cargo::style::USAGE)
    .literal(clap_cargo::style::LITERAL)
    .placeholder(clap_cargo::style::PLACEHOLDER)
    .error(clap_cargo::style::ERROR)
    .valid(clap_cargo::style::VALID)
    .invalid(clap_cargo::style::INVALID);

const LONG_ABOUT: &str = r#"vellum syncs shell command history between hosts using a git repository as a
 central synchronisation point."#;

#[derive(Parser)]
#[command(version, about, long_about = LONG_ABOUT)]
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

    /// Edit stored history
    Edit(client::EditArgs),

    /// Mark specified history entries as deleted
    ///
    /// NOTE: When entries are deleted they are only marked as deleted. This
    /// means that they are no longer visible from a client, but the deleted
    /// entry is still stored on disk / in git (in encrypted form). If you want
    /// to completely erase the entry you will also need to run `vellum rebuild`
    /// to rebuild the on-disk data.
    Delete {
        /// IDs of entries to be marked as deleted
        #[arg(required = true, value_hint = ValueHint::Other)]
        ids: Vec<String>,
    },

    /// Import command history from stdin or a file
    Import(client::ImportArgs),

    /// Display the vellum configuration
    Config,

    /// Commands to setup/initialise vellum
    Init(init::Args),

    /// Generate shell completion file
    Complete(complete::Args),

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

    /// Request the server rebuild the sync data
    Rebuild,

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
    env_logger::Builder::from_env(
        Env::new()
            .filter_or("VELLUM_LOG", "info")
            .write_style("VELLUM_LOG_STYLE"),
    )
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
        Commands::Edit(args) => client::edit(&config, args),
        Commands::Delete { ids } => client::delete(&config, ids),
        Commands::Import(args) => client::import(&config, args),
        Commands::Config => config.show(),
        Commands::Init(args) => init::init(args, Cli::command()),
        Commands::Complete(args) => complete::complete(args, Cli::command()),
        Commands::Ping { wait } => client::ping(&config, wait),
        Commands::Sync { force } => client::sync(&config, force),
        Commands::Rebuild => client::rebuild(&config),
        Commands::Server(args) => server::run(&config, args),
        Commands::Stop { no_sync } => client::stop_server(&config, no_sync),
    } {
        error!("{e}");
        exit(1);
    }
}
