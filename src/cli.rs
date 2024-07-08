use std::{path::PathBuf, time::Duration};

use anyhow::{bail, Result};
use clap::{arg, Parser};

#[derive(Parser)]
pub struct Cli {
    /// Interval in seconds between image switches
    #[arg(short, long, default_value_t = 60)]
    interval: u64,

    /// Directory of images
    dir: PathBuf,
}

impl Cli {
    pub fn parse_and_validate() -> Result<(PathBuf, Duration)> {
        let args = Cli::parse();
        if !args.dir.is_dir() {
            bail!("{} is not an existing directory", args.dir.display());
        }
        Ok((args.dir, Duration::from_secs(args.interval)))
    }
}
