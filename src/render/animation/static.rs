use std::{iter::once, path::Path};

use crate::render::{animation::INDICES, Context, Texture};
use anyhow::Result;
use log::*;

use super::{
    create_index_buffer, create_pipeline, create_texture_binds, create_uniform_binds,
    create_vertex_buffer, Animation,
};

pub struct Static {
    texture: Texture,
    texture_bind_group: wgpu::BindGroup,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    render_pipeline: wgpu::RenderPipeline,

    finished: bool,
}

impl Static {
    pub fn open(img: &Path, ctx: &Context) -> Result<Self> {
        let texture = Texture::open(img, ctx)?;
        let finished = false;

        let (texture_bind_group_layout, texture_bind_group) = create_texture_binds(&texture, ctx);

        let vertex_buffer = create_vertex_buffer(ctx);
        let index_buffer = create_index_buffer(ctx);

        let (uniform_buffer, uniform_bind_group_layout, uniform_bind_group) =
            create_uniform_binds(32, ctx);

        let shader = ctx
            .device()
            .create_shader_module(wgpu::include_wgsl!("./shaders/static.wgsl"));

        let render_pipeline = create_pipeline(
            ctx,
            &[&texture_bind_group_layout, &uniform_bind_group_layout],
            &shader,
            ctx.config(),
        );

        Ok(Self {
            texture,
            finished,
            texture_bind_group,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            uniform_bind_group,
            render_pipeline,
        })
    }
}

impl Animation for Static {
    fn render(&mut self, ctx: &Context) {
        if self.finished {
            return;
        }

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

        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[ctx.surface_aspect_ratio() / self.texture.aspect_ratio()]),
        );

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
