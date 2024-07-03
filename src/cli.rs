use std::{path::PathBuf, time::Duration};

use anyhow::{bail, Result};
use clap::{arg, Parser};

use crate::image_loader::ImageLoader;

#[derive(Parser)]
pub struct Cli {
    /// Interval in seconds between image switches
    #[arg(short, long, default_value_t = 60)]
    interval: u64,

    /// Directory of images
    dir: PathBuf,
}

impl Cli {
    pub fn parse_and_validate() -> Result<(ImageLoader, Duration)> {
        let args = Cli::parse();
        if !args.dir.is_dir() {
            bail!("{} is not an existing directory", args.dir.display());
        }
        Ok((
            ImageLoader::new(args.dir),
            Duration::from_secs(args.interval),
        ))
    }
}
