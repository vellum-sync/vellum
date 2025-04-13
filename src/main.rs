use std::{env, fs, io, path::Path, process::exit};

use clap::{Parser, Subcommand};
use env_logger::Target;
use log::error;

mod api;
mod config;
mod error;
mod server;

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
    History,

    /// Display the vellum configuration
    Config,

    /// Run the background history management server
    Server(server::Args),
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
        Commands::Store {
            shell_command: command,
        } => Ok(println!("store: {command}")),
        Commands::History => Ok(println!("history")),
        Commands::Config => config.show(),
        Commands::Server(args) => server::run(&config, args),
    } {
        error!("{e}");
        exit(1);
    }
}
