use clap;

use crate::config::Config;

#[derive(clap::Args, Debug)]
pub struct Args {}

pub fn run(config: &Config, args: Args) {
    println!("server: config={config:?} args={args:?}");
}
