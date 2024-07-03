use std::time::{Duration, Instant};

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use keyframe::{ease, functions::EaseInOut};
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};

fn resize(image: DynamicImage, dimensions: (u32, u32)) -> DynamicImage {
    if image.dimensions() == dimensions {
        return image;
    }
    let (width, height) = dimensions;
    image.resize_to_fill(width, height, FilterType::Lanczos3)
}

pub trait Animation {
    fn next(&mut self, dimensions: (u32, u32)) -> Option<Vec<u8>>;
    fn finished(&self) -> bool;
}

pub struct Static {
    finished: bool,
    img: Option<DynamicImage>,
}

impl Static {
    pub fn new(img: DynamicImage) -> Self {
        let finished = false;
        Self {
            img: Some(img),
            finished,
        }
    }
}

impl Animation for Static {
    fn next(&mut self, dimensions: (u32, u32)) -> Option<Vec<u8>> {
        let frame = std::mem::take(&mut self.img);
        self.finished = true;
        frame.map(|p| resize(p, dimensions).to_rgba8().as_raw().to_vec())
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

pub struct Fade {
    start: Option<Instant>,
    fps: f32,
    finished: bool,
    frames: Vec<Option<f32>>,
    current_img: DynamicImage,
    next_img: DynamicImage,
}

fn blend(fg: &[u8], bg: &[u8], alpha: f32) -> Vec<u8> {
    fg.par_iter()
        .zip(bg.par_iter())
        .map(|(fg, bg)| (*fg as f32 * alpha + *bg as f32 * (1.0 - alpha)) as u8)
        .collect()
}

impl Fade {
    pub fn new(
        time: Duration,
        fps: u32,
        next_img: DynamicImage,
        current_img: DynamicImage,
    ) -> Self {
        let frames = (fps as f32 * time.as_secs_f32()) as u32;
        let frames = (0..frames)
            .map(|i| i as f32 / frames as f32)
            .map(|f| ease(EaseInOut, 0.0, 1.0, f))
            .map(Some)
            .collect();
        Self {
            start: None,
            fps: fps as f32,
            finished: false,
            frames,
            next_img,
            current_img,
        }
    }

    fn get(&mut self, idx: usize) -> Option<Vec<u8>> {
        if idx >= self.frames.len() {
            self.finished = true;
            return None;
        }
        let alpha = std::mem::take(self.frames.get_mut(idx)?)?;
        let fg_img = self.next_img.to_rgba8();
        let bg_img = self.current_img.to_rgba8();
        if idx == 0 {
            return Some(bg_img.as_raw().to_vec());
        }
        if idx == self.frames.len() - 1 {
            return Some(fg_img.as_raw().to_vec());
        }
        Some(blend(
            fg_img.as_raw().as_slice(),
            bg_img.as_raw().as_slice(),
            alpha,
        ))
    }
}

impl Animation for Fade {
    fn next(&mut self, dimensions: (u32, u32)) -> Option<Vec<u8>> {
        let current_img = std::mem::take(&mut self.current_img);
        let next_img = std::mem::take(&mut self.next_img);
        self.current_img = resize(current_img, dimensions);
        self.next_img = resize(next_img, dimensions);
        if self.start.is_none() {
            self.start = Some(Instant::now());
            return self.get(0);
        }
        let start = self.start.unwrap();
        let idx = (start.elapsed().as_secs_f32() * self.fps).round() as usize;
        self.get(idx)
    }

    fn finished(&self) -> bool {
        self.finished
    }
}
