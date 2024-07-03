use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use image::DynamicImage;
use rand::seq::SliceRandom;

use crate::animation::{Fade, Static};

pub struct ImageLoader {
    image_dir: PathBuf,
    current_img: Option<DynamicImage>,
}

impl ImageLoader {
    pub fn new(image_dir: PathBuf) -> Self {
        Self {
            image_dir,
            current_img: None,
        }
    }
    fn get_next_img(&self) -> Result<DynamicImage> {
        let mut rng = rand::thread_rng();
        let mut files: Vec<_> = self
            .image_dir
            .read_dir()?
            .filter_map(Result::ok)
            .map(|d| d.path())
            .filter(|p| p.is_file())
            .collect();
        files.shuffle(&mut rng);
        files
            .into_iter()
            .filter_map(|p| {
                println!("Attempting to load {}", p.display());
                image::open(p).ok()
            })
            .next()
            .map(|i| {
                println!("success");
                i
            })
            .with_context(|| {
                format!(
                    "Unable to open any file from {} as an image",
                    self.image_dir.display()
                )
            })
    }
    pub fn load_fade(&mut self) -> Result<Fade> {
        let current_img =
            std::mem::take(&mut self.current_img).context("Previous image not set yet")?;
        let next_img = self.get_next_img()?;
        self.current_img = Some(next_img.clone());
        Ok(Fade::new(Duration::from_secs(5), 24, next_img, current_img))
    }

    pub fn load_static(&mut self) -> Result<Static> {
        let img = self.get_next_img()?;
        self.current_img = Some(img.clone());
        Ok(Static::new(img))
    }
}
