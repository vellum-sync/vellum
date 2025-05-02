use std::io::stdout;

use clap::Command;
use clap_complete::{Shell, generate};

use crate::error::Result;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Which shell to generate completion for
    #[clap(value_enum)]
    shell: Shell,
}

pub fn complete(args: Args, mut cmd: Command) -> Result<()> {
    let bin_name = cmd.get_name().to_string();
    generate(args.shell, &mut cmd, bin_name, &mut stdout());
    Ok(())
}
