struct Uniform {
  alpha: f32,
  surface_to_image_a_arr: f32,
  surface_to_image_b_arr: f32,
}


struct VertexInput {
  @location(0) position: vec3<f32>,
  @location(1) tex_coords: vec2<f32>,
};
struct VertexOutput {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) tex_coords_a: vec2<f32>,
  @location(1) tex_coords_b: vec2<f32>,
};

@group(1) @binding(0)
var<uniform> uniform: Uniform;


fn scale(coords: vec2<f32>, surface_to_image_arr: f32) -> vec2<f32> {
  if (surface_to_image_arr > 1.0) {
    let scale = 1.0 / surface_to_image_arr;
    return coords * vec2<f32>(1.0, scale) * vec2f(1.0, -1.0) / 2.0 + 0.5;
  } else {
    let scale = surface_to_image_arr;
    return coords * vec2<f32>(scale, 1.0) * vec2f(1.0, -1.0) / 2.0 + 0.5;
  }
}

@vertex
fn vs_main(
  model: VertexInput,
) -> VertexOutput {
  var out: VertexOutput;
  out.clip_position = vec4<f32>(model.position, 1.0);

  out.tex_coords_a = scale(model.position.xy, uniform.surface_to_image_a_arr);
  out.tex_coords_b = scale(model.position.xy, uniform.surface_to_image_b_arr);

  return out;
}

@group(0) @binding(0)
var a_view: texture_2d<f32>;
@group(0) @binding(1)
var a_sampler: sampler;
@group(0) @binding(2)
var b_view: texture_2d<f32>;
@group(0) @binding(3)
var b_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
  let a_color = textureSample(a_view, a_sampler, in.tex_coords_a);
  let b_color = textureSample(b_view, b_sampler, in.tex_coords_b);

  let combined = b_color * uniform.alpha + a_color * (1.0 - uniform.alpha);

  return vec4<f32>(combined.xyz, 1.0);
}
