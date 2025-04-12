use std::process::exit;

use clap::{Parser, Subcommand};
use log::error;

mod config;
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
    Store{
        /// the shell command to be stored
        shell_command: String,
    },

    /// List all the stored commands
    History,

    /// Run the background history management server
    Server(server::Args),
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    let config = match config::Config::load(cli.config.as_ref()) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load config: {e}");
            exit(1);
        }
    };

    match cli.command {
        Commands::Store{shell_command: command} => println!("store: {command}"),
        Commands::History => println!("history"),
        Commands::Server(args) => server::run(&config, args),
    }
}
