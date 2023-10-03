use std::borrow::Cow;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
use imageproc::rect::Rect;
use rusttype::{Font, Scale};
mod input;

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

// In WGPU, we define an async function whose operation can be suspended and resumed.
// This is because on web, we can't take over the main event loop and must leave it to
// the browser.  On desktop, we'll just be running this function to completion.
async fn run(event_loop: EventLoop<()>, window: Window) {
    let size = window.inner_size();

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

    let (tex_47, mut img_47) = load_texture("content/king.png", Some("47 image"), &device, &queue).expect("Couldn't load 47 img");
    let view_47 = tex_47.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler_47 = device.create_sampler(&wgpu::SamplerDescriptor::default());
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
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: swapchain_capabilities.alpha_modes[0],
        view_formats: vec![],
    };
    surface.configure(&device, &config);

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

    let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &texture_bind_group_layout,
        entries: &[
            // One for the texture, one for the sampler
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view_47),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler_47),
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

    let mut input = input::Input::default();
    let mut color = image::Rgba([255,0,0,255]);
    let mut brush_size = 10_i32;
    let (img_47_w, img_47_h) = img_47.dimensions();

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
    struct GPUSprite {
        screen_region: [f32;4],
        // Textures with a bunch of sprites are often called "sprite sheets"
        sheet_region: [f32;4]
    }
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
    let mut sprites = vec![
    GPUSprite {
        screen_region: [32.0, 32.0, 64.0, 64.0],
        sheet_region: [0.0, 16.0/32.0, 16.0/32.0, 16.0/32.0],
    }/* ,
    GPUSprite {
        screen_region: [32.0, 128.0, 64.0, 64.0],
        sheet_region: [16.0/32.0, 16.0/32.0, 16.0/32.0, 16.0/32.0],
    },
    GPUSprite {
        screen_region: [128.0, 32.0, 64.0, 64.0],
        sheet_region: [0.0, 16.0/32.0, 16.0/32.0, 16.0/32.0],
    },
    GPUSprite {
        screen_region: [128.0, 128.0, 64.0, 64.0],
        sheet_region: [16.0/32.0, 16.0/32.0, 16.0/32.0, 16.0/32.0],
    },
    */];

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
                // if input.is_key_down(winit::event::VirtualKeyCode::Left) {
                //     sprites[0].screen_region[0] -= 1.0;
                // }

                // let mut step = 0;

                

                // Then send the data to the GPU!
                queue.write_buffer(&buffer_camera, 0, bytemuck::bytes_of(&camera));
                queue.write_buffer(&buffer_sprite, 0, bytemuck::cast_slice(&sprites));
                // ...all the drawing stuff goes here...
                window.request_redraw();
                // Leave now_keys alone, but copy over all changed keys
                input.next_frame();

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
                    // Now we begin a render pass.  The descriptor tells WGPU that
                    // we want to draw onto our swapchain texture view (that's where the colors will go)
                    // and that there's no depth buffer or stencil buffer.
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });
                    rpass.set_pipeline(&render_pipeline);
                    rpass.set_bind_group(0, &sprite_bind_group, &[]);
                    rpass.set_bind_group(1, &texture_bind_group, &[]);
                    // draw two triangles per sprite, and sprites-many sprites.
                    // this uses instanced drawing, but it would also be okay
                    // to draw 6 * sprites.len() vertices and use modular arithmetic
                    // to figure out which sprite we're drawing, instead of the instance index.
                    rpass.draw(0..6, 0..(sprites.len() as u32));
            }

                // Once the commands have been scheduled, we send them over to the GPU via the queue.
                queue.submit(Some(encoder.finish()));
                // Then we wait for the commands to finish and tell the windowing system to
                // present the swapchain image.
                frame.present();
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

                if input.is_key_down(winit::event::VirtualKeyCode::Left) {
                    sprites[0].screen_region[0] -= 2.0;

                    if sprites[0].sheet_region[0] == (16.0/32.0) {
                        sprites[0].sheet_region[0]= (0.0);
                    }
                    else if sprites[0].sheet_region[0] == (0.0) {
                        sprites[0].sheet_region[0]= (16.0/32.0);
                    }
                    window.request_redraw();

                }

                // if input.is_key_down(winit::event::VirtualKeyCode::Right) {
                //     sprites[0].screen_region[0] += 1.0;
                // }


                if input.is_key_down(winit::event::VirtualKeyCode::Right) {
                    sprites[0].screen_region[0] += 2.0;

                    if sprites[0].sheet_region[0] == (16.0/32.0) {
                        sprites[0].sheet_region[0]= (0.0);
                    }
                    else if sprites[0].sheet_region[0] == (0.0) {
                        sprites[0].sheet_region[0]= (16.0/32.0);
                    }
                    // if step==2 {
                    //     if sprites[0].sheet_region[0] == (16.0/32.0) {
                    //         sprites[0].sheet_region[0]= (0.0);
                    //     }
                    //     else if sprites[0].sheet_region[0] == (0.0) {
                    //         sprites[0].sheet_region[0]= (16.0/32.0);
                    //     }
                    //     step=0;
                    // }
                    // else if (step==1) | (step==0) {
                    //     step+=1;
                    // }
                    window.request_redraw();

                }

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