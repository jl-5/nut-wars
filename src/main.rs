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
mod char_action;
mod gpus;
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

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct GPUSprite {
    screen_region: [f32;4],
    // Textures with a bunch of sprites are often called "sprite sheets"
    sheet_region: [f32;4]
}

// In WGPU, we define an async function whose operation can be suspended and resumed.
// This is because on web, we can't take over the main event loop and must leave it to
// the browser.  On desktop, we'll just be running this function to completion.
async fn run(event_loop: EventLoop<()>, window: Window) {
    let mut gpu = gpus::WGPU::new(&window).await;
    let mut gs = game_state::init_game_state();

    let (squirrel_tex, mut squirrel_img) = gpus::WGPU::load_texture("content/spritesheet.png", Some("squirrel"), &gpu.device, &gpu.queue).await.expect("Couldn't load squirrel sprite sheet");
    let view: wgpu::TextureView = squirrel_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor::default());

    let (tex_bg, mut img_bg) = gpus::WGPU::load_texture("content/forest_background.png", Some("background"), &gpu.device, &gpu.queue ).await.expect("Couldn't load background");
    let view_bg = tex_bg.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler_bg = gpu.device.create_sampler(&wgpu::SamplerDescriptor::default());

    // Set up text renderer
    let mut font_system = FontSystem::new();
    let mut cache = SwashCache::new();
    let mut atlas = TextAtlas::new(&gpu.device, &gpu.queue, gpu.config.format);
    let mut text_renderer = TextRenderer::new(&mut atlas, &gpu.device, MultisampleState::default(), None);
    let mut buffer = Buffer::new(&mut font_system, Metrics::new(60.0, 42.0));
    
    let physical_width = (gpu.config.width as f64 * window.scale_factor()) as f32;
    let physical_height = (gpu.config.height as f64 * window.scale_factor()) as f32;
    
    buffer.set_size(&mut font_system, physical_width, physical_height);

    let score_text = format!("Score: {}", gs.score);
    buffer.set_text(&mut font_system, &score_text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    buffer.shape_until_scroll(&mut font_system);

    // Load the shaders from disk.  Remember, shader programs are things we compile for
    // our GPU so that it can compute vertices and colorize fragments.
    let shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        // Cow is a "copy on write" wrapper that abstracts over owned or borrowed memory.
        // Here we just need to use it since wgpu wants "some text" to compile a shader from.
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let texture_bind_group_layout =
    gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
    gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
    let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&sprite_bind_group_layout, &texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline_layout_bg = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let texture_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
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

    let tex_bg_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
    let render_pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            targets: &[Some(gpu.config.format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let render_pipeline_bg = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            targets: &[Some(gpu.config.format.into())],
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

    let mut squirrel = char_action::Char_action::new(
        sprites[0].screen_region,
        squirrel_animation,
        2.0,
        true,
        0,
    );

    let mut acorn = char_action::Char_action::new(
        sprites[1].screen_region,
        acorn_animation,
        2.0,
        true,
        1,
    );

    let buffer_camera = gpu.device.create_buffer(&wgpu::BufferDescriptor{
        label: None,
        size: bytemuck::bytes_of(&camera).len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false
    });
    let buffer_sprite = gpu.device.create_buffer(&wgpu::BufferDescriptor{
        label: None,
        size: bytemuck::cast_slice::<_,u8>(&sprites).len() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false
    });

    gpu.queue.write_buffer(&buffer_camera, 0, bytemuck::bytes_of(&camera));
    gpu.queue.write_buffer(&buffer_sprite, 0, bytemuck::cast_slice(&sprites));

    let sprite_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                gpu.resize(size);
                // On MacOS the window needs to be redrawn manually after resizing
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                // TODO: move sprites, maybe scroll camera
                

                // Then send the data to the GPU!
                gpu.queue.write_buffer(&buffer_camera, 0, bytemuck::bytes_of(&camera));
                gpu.queue.write_buffer(&buffer_sprite, 0, bytemuck::cast_slice(&sprites));
                // ...all the drawing stuff goes here...
                window.request_redraw();

                // Leave now_keys alone, but copy over all changed keys
                input.next_frame();

                text_renderer.prepare(
                    &gpu.device,
                    &gpu.queue,
                    &mut font_system,
                    &mut atlas,
                    Resolution {
                        width: gpu.config.width,
                        height: gpu.config.height,
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
                let frame = gpu.surface
                    .get_current_texture()
                    .expect("Failed to acquire next swap chain texture");
                // And set up a texture view onto it, since the GPU needs a way to interpret those
                // image bytes for writing.
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                // From the queue we obtain a command encoder that lets us issue GPU commands
                let mut encoder =
                gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
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
                gpu.queue.submit(Some(encoder.finish()));
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
                    squirrel.walk();
                    squirrel.animation.tick();
                    
                }
                else if input.is_key_down(winit::event::VirtualKeyCode::Right) {

                    squirrel.face_right();
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