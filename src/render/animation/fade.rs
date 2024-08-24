use std::{
    iter::once,
    time::{Duration, Instant},
};

use image::DynamicImage;
use keyframe::functions::EaseInOut;
use log::*;

use crate::render::{self, Context, Texture};

use super::{
    create_index_buffer, create_pipeline, create_texture_binds, create_uniform_binds,
    create_vertex_buffer, Animation, INDICES,
};

pub struct Fade {
    start_time: Option<Instant>,
    duration: Duration,

    texture_a: Texture,
    texture_b: Texture,
    texture_bind_group: wgpu::BindGroup,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    render_pipeline: wgpu::RenderPipeline,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    alpha: f32,
    surface_to_texture_a_arr: f32,
    surface_to_texture_b_arr: f32,
}

impl Fade {
    pub fn new(
        img_a: &DynamicImage,
        img_b: &DynamicImage,
        duration: Duration,
        ctx: &render::Context,
    ) -> Self {
        let start_time = None;
        let texture_a = Texture::from_image(img_a, ctx);
        let texture_b = Texture::from_image(img_b, ctx);

        let (texture_bind_group_layout, texture_bind_group) =
            create_texture_binds(&[&texture_a, &texture_b], ctx);

        let vertex_buffer = create_vertex_buffer(ctx);
        let index_buffer = create_index_buffer(ctx);

        let (uniform_buffer, uniform_bind_group_layout, uniform_bind_group) =
            create_uniform_binds(std::mem::size_of::<Uniform>() as u64, ctx);

        let shader = ctx
            .device()
            .create_shader_module(wgpu::include_wgsl!("./shaders/fade.wgsl"));
        let render_pipeline = create_pipeline(
            ctx,
            &[&texture_bind_group_layout, &uniform_bind_group_layout],
            &shader,
            ctx.config(),
        );

        Self {
            start_time,
            duration,

            texture_a,
            texture_b,
            texture_bind_group,

            vertex_buffer,
            index_buffer,

            uniform_buffer,
            uniform_bind_group,

            render_pipeline,
        }
    }

    fn update_uniform(&mut self, ctx: &Context) {
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
        let alpha = self
            .start_time
            .map(|t| t.elapsed().as_secs_f32() / self.duration.as_secs_f32())
            .map(|s| {
                if s >= 1.0 {
                    1.0
                } else {
                    keyframe::ease(EaseInOut, 0.0, 1.0, s) as f32
                }
            })
            .unwrap_or(0.0);

        debug!("alpha = {alpha}");

        let surface_to_texture_a_arr = ctx.surface_aspect_ratio() / self.texture_a.aspect_ratio();
        let surface_to_texture_b_arr = ctx.surface_aspect_ratio() / self.texture_b.aspect_ratio();

        let data = Uniform {
            alpha,
            surface_to_texture_a_arr,
            surface_to_texture_b_arr,
        };
        ctx.queue()
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[data]));
    }
}
impl Animation for Fade {
    fn is_finished(&self) -> bool {
        self.start_time
            .map(|x| x.elapsed().as_secs_f32() / self.duration.as_secs_f32() > 1.1)
            .unwrap_or(false)
    }
    fn update_img(&mut self, img: &DynamicImage, ctx: &Context) {
        self.start_time = None;
        let texture = Texture::from_image(img, ctx);
        std::mem::swap(&mut self.texture_a, &mut self.texture_b);
        self.texture_b = texture;
        let (_, bindgroup) = create_texture_binds(&[&self.texture_a, &self.texture_b], ctx);
        self.texture_bind_group = bindgroup;
    }

    fn render(&mut self, ctx: &Context) {
        let queue = ctx.queue();
        let device = ctx.device();
        let surface = ctx.surface();

        let output = surface.get_current_texture();
        if let Err(e) = output {
            error!("Could not get texture from surface: {e}");
            return;
        }
        let output = output.unwrap();
        let view = output.texture.create_view(&Default::default());

        self.update_uniform(ctx);

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
            render_pass.set_bind_group(1, &self.uniform_bind_group, &[]);
            render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        }
        queue.submit(once(encoder.finish()));
        output.present();
    }
}
