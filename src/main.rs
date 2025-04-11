use clap::{Args, Parser, Subcommand};

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
    Server(ServerArgs),
}

#[derive(Args, Debug)]
struct ServerArgs {
    /// Server configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<String>,

}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Store{shell_command: command} => println!("store: {command}"),
        Commands::History => println!("history"),
        Commands::Server(args) => server::run(args.config),
    }
}
