

struct VertexInput {
  @location(0) position: vec3<f32>,
  @location(1) tex_coords: vec2<f32>,
};
struct VertexOutput {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(
  model: VertexInput,
) -> VertexOutput {
  var out: VertexOutput;
  out.tex_coords = model.tex_coords;
  out.clip_position = vec4<f32>(model.position, 1.0);
  return out;
}

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;
@group(1) @binding(0)
var<uniform> surface_to_image_arr: f32;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  var tex_coords: vec2<f32>;
  if (surface_to_image_arr > 1.0) {
    let scale = 1.0 / surface_to_image_arr;
    tex_coords = in.tex_coords * vec2<f32>(1.0, scale) + vec2<f32>(0.0, 0.5 * (1.0 - scale));
  } else {
    let scale = surface_to_image_arr;
    tex_coords = in.tex_coords * vec2<f32>(scale, 1.0) + vec2<f32>(0.5 * (1.0 - scale), 0.0);
  }
  return textureSample(t_diffuse, s_diffuse, tex_coords);
}
