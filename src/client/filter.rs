use crate::{error::Result, history::Entry};

use super::Session;

#[derive(clap::Args, Debug)]
pub struct FilterArgs {
    /// Only include commands stored by the current session
    #[arg(short, long)]
    session: bool,
}

pub struct Filter {
    args: FilterArgs,

    current_session: Session,
}

impl Filter {
    pub fn new(args: FilterArgs) -> Result<Self> {
        let current_session = Session::get()?;
        Ok(Self {
            args,
            current_session,
        })
    }

    pub fn includes_entry(&self, entry: &Entry) -> bool {
        !self.args.session || self.current_session.includes_entry(entry)
    }
}
