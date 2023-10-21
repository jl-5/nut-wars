use std::{borrow::Cow, f32::consts::E};

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::float32x2_t;
use animation::Animation;
use wgpu::Texture;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
use std::path::Path;
use imageproc::rect::Rect;
use rusttype::{Font, Scale};
mod game_state;
mod input;
mod animation;
use rand::Rng;
use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer,
};
use wgpu::{
    CompositeAlphaMode, MultisampleState, 
};

// AsRef means we can take as parameters anything that cheaply converts into a Path,
// for example an &str.
fn load_texture(
    path: impl AsRef<std::path::Path>,
    label: Option<&str>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(wgpu::Texture, image::RgbaImage), image::ImageError> {
    // This ? operator will return the error if there is one, unwrapping the result otherwise.
    let img = image::open(path.as_ref())?.to_rgba8();

    let (width, height) = img.dimensions();
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &img,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );
    Ok((texture,img))
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct GPUSprite {
    screen_region: [f32;4],
    // Textures with a bunch of sprites are often called "sprite sheets"
    sheet_region: [f32;4]
}
pub struct Character {
    screen_region: [f32; 4],
    animation: Animation,
    speed: f32,
    facing_right: bool,
    sprites_index: usize,
}

impl Character {

    fn walk(&mut self){
        if self.facing_right {
            self.screen_region[0] += self.speed;
        }
        // if facing left
        else {
            self.screen_region[0] -= self.speed;
        }
    }
    fn face_left(&mut self) {
        self.facing_right = false;
        if self.screen_region[2] < 0.0 {
            self.screen_region[2] *= -1.0;
            self.screen_region[0] -= 60.0;
        }
        
    }
    fn face_right(&mut self) {
        self.facing_right = true;
        if self.screen_region[2] > 0.0 {
            self.screen_region[2] *= -1.0;
            self.screen_region[0] += 60.0;
        }
    }
    fn move_down(&mut self) {
        self.screen_region[1] -= self.speed;

        if self.screen_region[1] <= 0.0 {
            self.screen_region[1] = 768.0;
            self.screen_region[0] = rand::thread_rng().gen_range(0..1025) as f32;
        }
    }
    fn reset_y(&mut self){
        self.screen_region[1] = 768.0;
        self.screen_region[0] = rand::thread_rng().gen_range(0..1025) as f32;
    }
}

// In WGPU, we define an async function whose operation can be suspended and resumed.
// This is because on web, we can't take over the main event loop and must leave it to
// the browser.  On desktop, we'll just be running this function to completion.
async fn run(event_loop: EventLoop<()>, window: Window) {
    let size = window.inner_size();

    let mut gs = game_state::init_game_state();
    gs.typing = true;

    // An Instance is an instance of the graphics API.  It's the context in which other
    // WGPU values and operations take place, and there can be only one.
    // Its implementation of the Default trait automatically selects a driver backend.
    let instance = wgpu::Instance::default();

    // From the OS window (or web canvas) the graphics API can obtain a surface onto which
    // we can draw.  This operation is unsafe (it depends on the window not outliving the surface)
    // and it could fail (if the window can't provide a rendering destination).
    // The unsafe {} block allows us to call unsafe functions, and the unwrap will abort the program
    // if the operation fails.
    let surface = unsafe { instance.create_surface(&window) }.unwrap();

    // Next, we need to get a graphics adapter from the instance---this represents a physical
    // graphics card (GPU) or compute device.  Here we ask for a GPU that will be able to draw to the
    // surface we just obtained.
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            // Request an adapter which can render to our surface
            compatible_surface: Some(&surface),
        })
        // This operation can take some time, so we await the result. We can only await like this
        // in an async function.
        .await
        // And it can fail, so we panic with an error message if we can't get a GPU.
        .expect("Failed to find an appropriate adapter");

    // Create the logical device and command queue.  A logical device is like a connection to a GPU, and
    // we'll be issuing instructions to the GPU over the command queue.
    let (device, queue) = adapter
    .request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            features: wgpu::Features::empty(),
            // Bump up the limits to require the availability of storage buffers.
            limits: wgpu::Limits::downlevel_defaults()
                .using_resolution(adapter.limits()),
        },
        None,
    )
    .await
    .expect("Failed to create device");

    let (squirrel_tex, mut squirrel_img) = load_texture("content/spritesheet.png", Some("squirrel"), &device, &queue ).expect("Couldn't load squirrel sprite sheet");
    let view: wgpu::TextureView = squirrel_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

    let (tex_bg, mut img_bg) = load_texture("content/forest_background.png", Some("background"), &device, &queue ).expect("Couldn't load background");
    let view_bg = tex_bg.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler_bg = device.create_sampler(&wgpu::SamplerDescriptor::default());

    // The swapchain is how we obtain images from the surface we're drawing onto.
    // This is so we can draw onto one image while a different one is being presented
    // to the user on-screen.
    let swapchain_capabilities = surface.get_capabilities(&adapter);
    // We'll just use the first supported format, we don't have any reason here to use
    // one format or another.
    let swapchain_format = swapchain_capabilities.formats[0];

    // Our surface config lets us set up our surface for drawing with the device
    // we're actually using.  It's mutable in case the window's size changes later on.
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        width: size.width,
        height: size.height,
        // present_mode: wgpu::PresentMode::Fifo,
        present_mode: wgpu::PresentMode::Fifo,
        // alpha_mode: swapchain_capabilities.alpha_modes[0],
        alpha_mode: CompositeAlphaMode::Opaque,
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    //new
    let mut original_text = "Hello world! 👋\nThis is rendered with 🦅 glyphon 🦁\nThe text below should be partially clipped.\na b c d e f g h i j k l m n o p q r s t u v w x y z";
    let num_chars = original_text.len() as u32;
    let mut displayed_text:String = String::from("");
    // <li>Add/edit a set_text line before the run call, to set the buffer text to be the displayed text, which is currently an empty String.</li>
    // Set up text renderer
    let mut font_system = FontSystem::new();
    let mut cache = SwashCache::new();
    let mut atlas = TextAtlas::new(&device, &queue, swapchain_format);
    let mut text_renderer = TextRenderer::new(&mut atlas, &device, MultisampleState::default(), None);
    let mut buffer = Buffer::new(&mut font_system, Metrics::new(60.0, 42.0));
    
    let physical_width = (size.width as f64 * window.scale_factor()) as f32;
    let physical_height = (size.height as f64 * window.scale_factor()) as f32;
    
    buffer.set_size(&mut font_system, physical_width, physical_height);

    let score_text = format!("Score: {}", gs.score);
    // buffer.set_text(&mut font_system, "Hello world! 👋\nThis is rendered with 🦅 glyphon 🦁\nThe text below should be partially clipped.\na b c d e f g h i j k l m n o p q r s t u v w x y z", Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    // buffer.set_text(&mut font_system, &displayed_text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    // buffer.set_text(&mut font_system, &gs.score.to_string(), Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    buffer.set_text(&mut font_system, &score_text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    // <li>Create a file called game_state.rs, and add the following code. This will serve as a game state object that we can make calls to, see the variables of, and edit the variables of from any files. Although we could make these variables in the original main function, in more complicated games, it will be beneficial to have a gamestate, and these are some variables regarding text that make sense to go inside this class.</li>
    buffer.shape_until_scroll(&mut font_system);

    // Load the shaders from disk.  Remember, shader programs are things we compile for
    // our GPU so that it can compute vertices and colorize fragments.
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        // Cow is a "copy on write" wrapper that abstracts over owned or borrowed memory.
        // Here we just need to use it since wgpu wants "some text" to compile a shader from.
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let texture_bind_group_layout =
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        // This bind group's first entry is for the texture and the second is for the sampler.
        entries: &[
            // The texture binding
            wgpu::BindGroupLayoutEntry {
                // This matches the binding number in the shader
                binding: 0,
                // Only available in the fragment shader
                visibility: wgpu::ShaderStages::FRAGMENT,
                // It's a texture binding
                ty: wgpu::BindingType::Texture {
                    // We can use it with float samplers
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    // It's being used as a 2D texture
                    view_dimension: wgpu::TextureViewDimension::D2,
                    // This is not a multisampled texture
                    multisampled: false,
                },
                // This is not an array texture, so it has None for count
                count: None,
            },
            // The sampler binding
            wgpu::BindGroupLayoutEntry {
                // This matches the binding number in the shader
                binding: 1,
                // Only available in the fragment shader
                visibility: wgpu::ShaderStages::FRAGMENT,
                // It's a sampler
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                // No count
                count: None,
            },
        ],
    });
    let sprite_bind_group_layout =
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            // The camera binding
            wgpu::BindGroupLayoutEntry {
                // This matches the binding in the shader
                binding: 0,
                // Available in vertex shader
                visibility: wgpu::ShaderStages::VERTEX,
                // It's a buffer
                ty: wgpu::BindingType::Buffer {
                    // Specifically, a uniform buffer
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None
                },
                // No count, not a buffer array binding
                count: None,
            },
            // The sprite buffer binding
            wgpu::BindGroupLayoutEntry {
                // This matches the binding in the shader
                binding: 1,
                // Available in vertex shader
                visibility: wgpu::ShaderStages::VERTEX,
                // It's a buffer
                ty: wgpu::BindingType::Buffer {
                    // Specifically, a storage buffer
                    ty: wgpu::BufferBindingType::Storage{read_only:true},
                    has_dynamic_offset: false,
                    min_binding_size: None
                },
                // No count, not a buffer array binding
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&sprite_bind_group_layout, &texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline_layout_bg = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &texture_bind_group_layout,
        entries: &[
            // One for the texture, one for the sampler
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    let tex_bg_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &texture_bind_group_layout,
        entries: &[
            // One for the texture, one for the sampler
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view_bg),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler_bg),
            },
        ],
    });

    // Our specific "function" is going to be a draw call using our shaders. That's what we
    // set up here, calling the result a render pipeline.  It's not only what shaders to use,
    // but also how to interpret streams of vertices (e.g. as separate triangles or as a list of lines),
    // whether to draw both the fronts and backs of triangles, and how many times to run the pipeline for
    // things like multisampling antialiasing.
    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(swapchain_format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let render_pipeline_bg = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout_bg),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main_bg",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main_bg",
            targets: &[Some(swapchain_format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let mut input = input::Input::default();
    let mut nut_count = 0;
    let mut color = image::Rgba([255,0,0,255]);
    let mut brush_size = 10_i32;
    let (img_bg_w, img_bg_h) = img_bg.dimensions();

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
    struct GPUCamera {
        screen_pos: [f32;2],
        screen_size: [f32;2]
    }
    let camera = GPUCamera {
        screen_pos: [0.0, 0.0],
        // Consider using config.width and config.height instead,
        // it's up to you whether you want the window size to change what's visible in the game
        // or scale it up and down
        screen_size: [1024.0, 768.0],
    };

    // total squirrel is 36x133px with 6 frames
    // one frame of squirrel is 36x22px
    let sprite_sheet_dimensions = squirrel_img.dimensions();
    let squirrel_total_w: f32 = 35.0;
    let squirrel_total_h: f32 = 174.0;
    let squirrel_frame_w: f32 = 35.0;
    let squirrel_frame_h: f32 = 22.5;

    // frames will be a series of frames 
    let mut squirrel_sheet_positions: Vec<[f32; 4]> = vec![

        // frame 1 sheet position
        [126.0/162.0, 25.0/174.0, 32.0/162.0, 21.0/174.0],

        // frame 2 sheet position
        [126.0/162.0, 48.0/174.0, 32.0/162.0, 22.0/174.0],
 
        // frame 3 sheet position
        [126.0/162.0, 72.0/174.0, 28.0/162.0, 23.0/174.0],

        // frame 4 sheet position
        [126.0/162.0, 97.0/174.0, 35.0/162.0, 23.0/174.0],

        // frame 5 sheet position
        [126.0/162.0, 122.0/174.0, 33.0/162.0, 22.0/174.0],

    ];

    let mut sprites: Vec<GPUSprite> = vec![
        // SQUIRREL
    GPUSprite {
        screen_region: [32.0, 32.0, 100.0, 100.0],
        sheet_region: squirrel_sheet_positions[0],   
    },

        // NUT
    GPUSprite {
        screen_region: [20.0, 200.0, 55.0, 55.0],
        sheet_region: [0.0, 0.0, 123.0/sprite_sheet_dimensions.0 as f32, 172.0/sprite_sheet_dimensions.1 as f32],   
    }
    ];

    let squirrel_animation: Animation = Animation {
        states: squirrel_sheet_positions,
        frame_counter: 0,
        rate: 7,
        state_number: 0,
    };

    let acorn_animation: Animation = Animation {
        states: [sprites[1].sheet_region].to_vec(),
        frame_counter: 0,
        rate: 7,
        state_number: 0,
    };

    let mut squirrel: Character = Character {
        screen_region: sprites[0].screen_region,
        animation: squirrel_animation,
        speed: 2.0,
        facing_right: true,
        sprites_index: 0,
    };

    let mut acorn: Character = Character {
        screen_region: sprites[1].screen_region,
        animation: acorn_animation,
        speed: 2.0,
        facing_right: true,
        sprites_index: 1,
    };

    let buffer_camera = device.create_buffer(&wgpu::BufferDescriptor{
        label: None,
        size: bytemuck::bytes_of(&camera).len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false
    });
    let buffer_sprite = device.create_buffer(&wgpu::BufferDescriptor{
        label: None,
        size: bytemuck::cast_slice::<_,u8>(&sprites).len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false
    });

    queue.write_buffer(&buffer_camera, 0, bytemuck::bytes_of(&camera));
    queue.write_buffer(&buffer_sprite, 0, bytemuck::cast_slice(&sprites));

    let sprite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &sprite_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer_camera.as_entire_binding()
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: buffer_sprite.as_entire_binding()
            }
        ],
    });

    // Now our setup is all done and we can kick off the windowing event loop.
    // This closure is a "move closure" that claims ownership over variables used within its scope.
    // It is called once per iteration of the event loop.
    event_loop.run(move |event, _, control_flow| {
        // By default, tell the windowing system that there's no more work to do
        // from the application's perspective.
        // *control_flow = ControlFlow::Poll;
        *control_flow = ControlFlow::Poll;
        // Depending on the event, we'll need to do different things.
        // There is some pretty fancy pattern matching going on here,
        // so think back to CSCI054.

        match event {
            Event::WindowEvent {
                // For example, "if it's a window event and the specific window event is that
                // we have resized the window to a particular new size called `size`..."
                event: WindowEvent::Resized(size),
                // Ignoring the rest of the fields of Event::WindowEvent...
                ..
            } => {
                // Reconfigure the surface with the new size
                config.width = size.width;
                config.height = size.height;
                surface.configure(&device, &config);
                // On MacOS the window needs to be redrawn manually after resizing
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                // TODO: move sprites, maybe scroll camera
                

                // Then send the data to the GPU!
                queue.write_buffer(&buffer_camera, 0, bytemuck::bytes_of(&camera));
                queue.write_buffer(&buffer_sprite, 0, bytemuck::cast_slice(&sprites));
                // ...all the drawing stuff goes here...
                window.request_redraw();

                // if gs.typing{
                    // let chars_iter = original_text.chars();
                    // for char in chars_iter.skip(gs.chars_typed as usize){
                    //     displayed_text += &char.to_string();
                    //     break;
                    // }
                    // buffer.set_text(&mut font_system, &displayed_text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
                    // buffer.set_text(&mut font_system, &gs.score.to_string(), Attrs::new().family(Family::SansSerif), Shaping::Advanced);
                    // ADD TYPING LOGIC
                    // gs.chars_typed += 1;
                    // if gs.chars_typed == num_chars{
                    //     gs.typing = false;
                    // }
                // }

                // Leave now_keys alone, but copy over all changed keys
                input.next_frame();

                text_renderer.prepare(
                    &device,
                    &queue,
                    &mut font_system,
                    &mut atlas,
                    Resolution {
                        width: config.width,
                        height: config.height,
                    },
                    [TextArea {
                        buffer: &buffer,
                        left: 10.0,
                        top: 10.0,
                        scale: 1.0,
                        bounds: TextBounds {
                            left: 0,
                            top: 0,
                            right: 600,
                            bottom: 160,
                        },
                        default_color: Color::rgb(255, 255, 255),
                    }],
                    &mut cache,
                ).unwrap();

                // If the window system is telling us to redraw, let's get our next swapchain image
                let frame = surface
                    .get_current_texture()
                    .expect("Failed to acquire next swap chain texture");
                // And set up a texture view onto it, since the GPU needs a way to interpret those
                // image bytes for writing.
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                // From the queue we obtain a command encoder that lets us issue GPU commands
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                {
                    
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                // When loading this texture for writing, the GPU should clear
                                // out all pixels to a lovely green color
                                load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                                // The results of drawing should always be stored to persistent memory
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });
                    rpass.set_pipeline(&render_pipeline_bg);
                    // Attach the bind group for group 0
                    rpass.set_bind_group(0, &tex_bg_bind_group, &[]);
                    // Now draw two triangles!
                    rpass.draw(0..6, 0..2);

                    // Now we begin a render pass.  The descriptor tells WGPU that
                    // we want to draw onto our swapchain texture view (that's where the colors will go)
                    // and that there's no depth buffer or stencil buffer.

                    text_renderer.render(&atlas, &mut rpass).unwrap();

                    rpass.set_pipeline(&render_pipeline);
                    rpass.set_bind_group(0, &sprite_bind_group, &[]);
                    rpass.set_bind_group(1, &texture_bind_group, &[]);
                    // // draw two triangles per sprite, and sprites-many sprites.
                    // // this uses instanced drawing, but it would also be okay
                    // // to draw 6 * sprites.len() vertices and use modular arithmetic
                    // // to figure out which sprite we're drawing, instead of the instance index.
                    rpass.draw(0..6, 0..(sprites.len() as u32));
            }

                // Once the commands have been scheduled, we send them over to the GPU via the queue.
                queue.submit(Some(encoder.finish()));
                // Then we wait for the commands to finish and tell the windowing system to
                // present the swapchain image.
                frame.present();
                atlas.trim();
                window.request_redraw();
            }

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            // WindowEvent->KeyboardInput: Keyboard input!
            Event::WindowEvent {
                // Note this deeply nested pattern match
                event: WindowEvent::KeyboardInput {
                    input:key_ev,
                    ..
                },
                ..
            } => {
            input.handle_key_event(key_ev);
            },
            Event::WindowEvent {
                event: WindowEvent::MouseInput { state, button, .. },
                ..
            } => {
                input.handle_mouse_button(state, button);
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                input.handle_mouse_move(position);
            }
            Event::MainEventsCleared => {

                acorn.move_down();

                if input.is_key_down(winit::event::VirtualKeyCode::Left) {

                    squirrel.face_left();

                    // move the squirrel
                    squirrel.walk();

                    squirrel.animation.tick();
                    
                }
                else if input.is_key_down(winit::event::VirtualKeyCode::Right) {

                    squirrel.face_right();

                    // move the squirrel
                    squirrel.walk();

                    squirrel.animation.tick();

                }
                else if input.is_key_up(winit::event::VirtualKeyCode::Left)  || input.is_key_up(winit::event::VirtualKeyCode::Right){
                    squirrel.animation.stop();
                }

                sprites[squirrel.sprites_index].sheet_region = squirrel.animation.get_current_state();
                sprites[squirrel.sprites_index].screen_region = squirrel.screen_region;

                sprites[acorn.sprites_index].screen_region = acorn.screen_region;

                let acorn_x: f32 = sprites[acorn.sprites_index].screen_region[0];
                let acorn_y: f32 = sprites[acorn.sprites_index].screen_region[1];
                let acorn_width: f32 = sprites[acorn.sprites_index].screen_region[2];
                let acorn_height: f32 = sprites[acorn.sprites_index].screen_region[3];

                let mut squirrel_x: f32 = sprites[squirrel.sprites_index].screen_region[0];
                let squirrel_y: f32 = sprites[squirrel.sprites_index].screen_region[1];
                let mut squirrel_width: f32 = sprites[squirrel.sprites_index].screen_region[2];
                let squirrel_height: f32 = sprites[squirrel.sprites_index].screen_region[3];

                // adjusting for right facing squirrel
                if squirrel.facing_right {
                    squirrel_x += squirrel_width;
                    squirrel_width *= -1.0;
                }

                // Check for collisions
                if (acorn_x + acorn_width > squirrel_x) && (acorn_x < squirrel_x + squirrel_width)
                    && (acorn_y - acorn_height < squirrel_y) && (acorn_y > squirrel_y - squirrel_height) {
                    // Collision detected, handle it here
                    nut_count += 1;
                    acorn.speed += 0.1;
                    acorn.reset_y();

                    if !gs.score_changing{
                        gs.score += 1;
                        let score_text = format!("Score: {}", gs.score);
                        // buffer.set_text(&mut font_system, &gs.score.to_string(), Attrs::new().family(Family::SansSerif), Shaping::Advanced);    
                        buffer.set_text(&mut font_system, &score_text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
                        gs.score_changing = true;
                    }

                }
                else{gs.score_changing = false;}

                window.request_redraw();
            }
            _ => {}
        }
    });
}

// Main is just going to configure an event loop, open a window, set up logging,
// and kick off our `run` function.
fn main() {
    let event_loop = EventLoop::new();
    let window = winit::window::Window::new(&event_loop).unwrap();

    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        // On native, we just want to wait for `run` to finish.
        pollster::block_on(run(event_loop, window));
    }
    #[cfg(target_arch = "wasm32")]
    {
        // On web things are a little more complicated.
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        use winit::platform::web::WindowExtWebSys;
        // On wasm, append the canvas to the document body
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.body())
            .and_then(|body| {
                body.append_child(&web_sys::Element::from(window.canvas()))
                    .ok()
            })
            .expect("couldn't append canvas to document body");
        // Now we use the browser's runtime to spawn our async run function.
        wasm_bindgen_futures::spawn_local(run(event_loop, window));
    }
}