// Where is the nth vertex in normalized device coordinates?
var<private> VERTICES_bg:array<vec4<f32>,6> = array<vec4<f32>,6>(
    // In WGPU, the bottom left corner is -1,-1 and the top right is 1,1.
    vec4<f32>(-1., -1., 0., 1.),
    vec4<f32>(1., -1., 0., 1.),
    vec4<f32>(-1., 1., 0., 1.),
    vec4<f32>(-1., 1., 0., 1.),
    vec4<f32>(1., -1., 0., 1.),
    vec4<f32>(1., 1., 0., 1.)
);

// How does each vertex map onto the texture's corners?
var<private> TEX_COORDS_bg:array<vec2<f32>,6> = array<vec2<f32>,6>(
    // Texture coordinates are a bit different---they go from 0,0 at the top left to 1,1 at the bottom right,
    // but if they are outside that bound they may clamp, or repeat the texture, or something else
    // depending on the sampler.
    vec2<f32>(0., 1.),
    vec2<f32>(1., 1.),
    vec2<f32>(0., 0.),
    vec2<f32>(0., 0.),
    vec2<f32>(1., 1.),
    vec2<f32>(1., 0.)
);

// Now we're outputting more than just a position,
// so we'll define a struct
struct VertexOutput_bg {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

// vs_main now produces an instance of that struct...
@vertex
fn vs_main_bg(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    // We'll just look up the vertex data in those constant arrays
    return VertexOutput_bg(
        VERTICES_bg[in_vertex_index],
        TEX_COORDS_bg[in_vertex_index]
    );
}

// A square made of two rectangles. This makes our vertex shader
// code simpler since we can look up the corners by number.
var<private> VERTICES:array<vec2<f32>,6> = array<vec2<f32>,6>(
    // Bottom left, bottom right, top left; then top left, bottom right, top right..
    vec2<f32>(0., 0.),
    vec2<f32>(1., 0.),
    vec2<f32>(0., 1.),
    vec2<f32>(0., 1.),
    vec2<f32>(1., 0.),
    vec2<f32>(1., 1.)
);

// Our camera struct
struct Camera {
    screen_pos: vec2<f32>,
    screen_size: vec2<f32>
}

// GPUSprite, from before
struct GPUSprite {
    to_rect:vec4<f32>,
    from_rect:vec4<f32>
}

// One binding for the camera...
@group(0) @binding(0)
var<uniform> camera: Camera;
// And another for the sprite buffer
@group(0) @binding(1)
var<storage, read> sprites: array<GPUSprite>;

// Same as before
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32,
           // Which instance, i.e. which specific sprite are we drawing now?
           @builtin(instance_index) sprite_index:u32) -> VertexOutput {
    // The corner and size of the sprite in world space.
    // Which sprite? sprites[sprite_index]
    let corner:vec4<f32> = vec4(sprites[sprite_index].to_rect.xy,0.,1.);
    let size:vec2<f32> = sprites[sprite_index].to_rect.zw;
    // The corner and size of the texture area in UVs
    let tex_corner:vec2<f32> = sprites[sprite_index].from_rect.xy;
    let tex_size:vec2<f32> = sprites[sprite_index].from_rect.zw;
    // Which corner of the square we need to draw now (in_vertex_index is in 0..6)
    let which_vtx:vec2<f32> = VERTICES[in_vertex_index];
    // Which corner of the UV square we need to draw (UV coordinates are flipped in Y)
    let which_uv: vec2<f32> = vec2(VERTICES[in_vertex_index].x, 1.0 - VERTICES[in_vertex_index].y);
    return VertexOutput(
        // Offset corner by size * which_vtx to get the right corner, then do camera stuff. Dividing screen size by 2 and the last subtraction are to deal with the NDC coordinate space, which goes from -1 to 1 in WGPU.
        ((corner + vec4(which_vtx*size,0.,0.) - vec4(camera.screen_pos,0.,0.)) / vec4(camera.screen_size/2., 1.0, 1.0)) - vec4(1.0, 1.0, 0.0, 0.0),
        // Offset texture corner by tex_size * which_uv to get the right corner
        tex_corner + which_uv*tex_size
    );
}

// Now our fragment shader needs two "global" inputs to be bound:
// A texture...
@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
// And a sampler.
@group(1) @binding(1)
var s_diffuse: sampler;
// Both are in the same binding group here since they go together naturally.

// Our fragment shader takes an interpolated `VertexOutput` as input now
@fragment
fn fs_main(in:VertexOutput) -> @location(0) vec4<f32> {
    // And we use the tex coords from the vertex output to sample from the texture.
    let color:vec4<f32> = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    // This is new: if the alpha value of the color is very low, don't draw any fragment here.
    // This is like "cutout" transparency.
    if color.w < 0.2 { discard; }
    return color;
}

// Now our fragment shader needs two "global" inputs to be bound:
// A texture...
@group(0) @binding(0)
var t_diffuse_bg: texture_2d<f32>;
// And a sampler.
@group(0) @binding(1)
var s_diffuse_bg: sampler;
// Both are in the same binding group here since they go together naturally.

// Our fragment shader takes an interpolated `VertexOutput` as input now
@fragment
fn fs_main_bg(in:VertexOutput) -> @location(0) vec4<f32> {
    // And we use the tex coords from the vertex output to sample from the texture.
    return textureSample(t_diffuse_bg, s_diffuse_bg, in.tex_coords);
}
