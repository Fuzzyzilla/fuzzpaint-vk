use crate::vulkano_prelude::*;
use std::sync::Arc;
use crate::gpu_err::*;

pub struct EguiEventAccumulator {
    events: Vec<egui::Event>,
    last_mouse_pos : Option<egui::Pos2>,
    last_modifiers : egui::Modifiers,
    //egui keys are 8-bit, so allocate 256 bools.
    held_keys : bitvec::array::BitArray<[u64; 4]>,
    has_focus : bool,
    hovered_files : Vec<egui::HoveredFile>,
    dropped_files : Vec<egui::DroppedFile>,
    screen_rect : Option<egui::Rect>,
    pixels_per_point: f32,

    is_empty: bool,
}
impl EguiEventAccumulator {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            last_mouse_pos: None,
            held_keys: bitvec::array::BitArray::ZERO,
            last_modifiers: egui::Modifiers::NONE,
            has_focus: true,
            hovered_files: Vec::new(),
            dropped_files: Vec::new(),
            screen_rect: None,
            pixels_per_point: 1.0,
            is_empty: false,
        }
    }
    pub fn accumulate(&mut self, event : &winit::event::Event<()>) {
        use egui::Event as GuiEvent;
        use winit::event::Event as SysEvent;
        //TODOS: Copy/Cut/Paste, IME, and Scroll + Zoom + MouseWheel confusion, Touch, AssistKit.
        match event {
            SysEvent::WindowEvent { event, .. } => {
                use winit::event::WindowEvent as WinEvent;
                match event {
                    WinEvent::Resized(size) => {
                        self.screen_rect = Some(egui::Rect{
                            min: egui::pos2(0.0, 0.0),
                            max: egui::pos2(size.width as f32, size.height as f32),
                        });
                        self.is_empty = false;
                    }
                    WinEvent::ScaleFactorChanged { scale_factor, .. } => {
                        self.pixels_per_point = *scale_factor as f32;
                        self.is_empty = false;
                    }
                    WinEvent::CursorLeft { .. } => {
                        self.last_mouse_pos = None;
                        self.events.push(
                            GuiEvent::PointerGone
                        );
                        self.is_empty = false;
                    }
                    WinEvent::CursorMoved { position, .. } => {
                        let position = egui::pos2(position.x as f32, position.y as f32);
                        self.last_mouse_pos = Some(position);
                        self.events.push(
                            GuiEvent::PointerMoved(position)
                        );
                        self.is_empty = false;
                    }
                    WinEvent::MouseInput { state, button, .. } => {
                        let Some(pos) = self.last_mouse_pos else {return};
                        let Some(button) = Self::winit_to_egui_mouse_button(*button) else {return};
                        self.events.push(
                            GuiEvent::PointerButton {
                                pos,
                                button,
                                pressed: if let winit::event::ElementState::Pressed = state {true} else {false},
                                modifiers: self.last_modifiers,
                            }
                        );
                        self.is_empty = false;
                    }
                    WinEvent::ModifiersChanged(state) => {
                        self.last_modifiers = egui::Modifiers{
                            alt: state.alt(),
                            command: state.ctrl(),
                            ctrl: state.ctrl(),
                            mac_cmd: false,
                            shift: state.shift(),
                        };
                        self.is_empty = false;
                    }
                    WinEvent::ReceivedCharacter(ch) => {
                        //Various ascii codes that winit emits which break Egui
                        if ('\x00'..'\x20').contains(ch) || *ch == '\x7F' {
                            return;
                        };
                        self.events.push(
                            GuiEvent::Text(
                                ch.to_string()
                            )
                        );
                        self.is_empty = false;
                    }
                    WinEvent::KeyboardInput { input, .. } => {
                        let Some(key) = input.virtual_keycode.and_then(Self::winit_to_egui_key) else {return};
                        let pressed = if let winit::event::ElementState::Pressed = input.state {true} else {false};

                        let prev_pressed = {
                            let mut key_state = self.held_keys.get_mut(key as u8 as usize).unwrap();
                            let prev_pressed = key_state.clone();
                            *key_state = pressed;
                            prev_pressed
                        };

                        self.events.push(
                            GuiEvent::Key {
                                key,
                                pressed,
                                repeat: prev_pressed && pressed,
                                modifiers: self.last_modifiers,
                            }
                        );
                        self.is_empty = false;
                    }
                    WinEvent::MouseWheel { delta, .. } => {
                        let (unit, delta) = match delta {
                            winit::event::MouseScrollDelta::LineDelta(x, y)
                                => (egui::MouseWheelUnit::Line, egui::vec2(*x, *y)),
                            winit::event::MouseScrollDelta::PixelDelta(delta)
                                => (egui::MouseWheelUnit::Point, egui::vec2(delta.x as f32, delta.y as f32)),
                        };
                        self.events.push(
                            GuiEvent::MouseWheel {
                                unit,
                                delta,
                                modifiers: self.last_modifiers,
                            }
                        );
                        self.is_empty = false;
                    }
                    WinEvent::TouchpadMagnify { delta, .. } => {
                        self.events.push(
                            GuiEvent::Zoom(*delta as f32)
                        );
                        self.is_empty = false;
                    }
                    WinEvent::Focused( has_focus ) => {
                        self.has_focus = *has_focus;
                        self.events.push(
                            GuiEvent::WindowFocused(self.has_focus)
                        );
                        self.is_empty = false;
                    }
                    WinEvent::HoveredFile(path) => {
                        self.hovered_files.push(
                            egui::HoveredFile{
                                mime: String::new(),
                                path: Some(path.clone()),
                            }
                        );
                        self.is_empty = false;
                    }
                    WinEvent::DroppedFile(path) => {
                        use std::io::Read;
                        let Ok(file) = std::fs::File::open(path) else {return};
                        //Surely there's a better way
                        let last_modified = if let Ok(Ok(modified)) = file.metadata().map(|md| md.modified()) {
                            Some(modified)
                        } else {
                            None
                        };

                        let bytes : Option<std::sync::Arc<[u8]>> = {
                            let mut reader = std::io::BufReader::new(file);
                            let mut data = Vec::new();

                            if let Ok(_) = reader.read_to_end(&mut data) {
                                Some(data.into())
                            } else {
                                None
                            }
                        };

                        self.dropped_files.push(
                            egui::DroppedFile{
                                name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                                bytes,
                                last_modified,
                                path: Some(path.clone()),
                            }
                        );
                        self.is_empty = false;
                    }
                    _ => ()
                }
            }
            _ => ()
        }
    }
    pub fn winit_to_egui_mouse_button(winit_button : winit::event::MouseButton) -> Option<egui::PointerButton> {
        use winit::event::MouseButton as WinitButton;
        use egui::PointerButton as EguiButton;
        match winit_button {
            WinitButton::Left => Some(EguiButton::Primary),
            WinitButton::Right => Some(EguiButton::Secondary),
            WinitButton::Middle => Some(EguiButton::Middle),
            WinitButton::Other(id) => {
                match id {
                    0 => Some(EguiButton::Extra1),
                    1 => Some(EguiButton::Extra2),
                    _ => None,
                }
            }
        }
    }
    pub fn winit_to_egui_key(winit_button : winit::event::VirtualKeyCode) -> Option<egui::Key> {
        use winit::event::VirtualKeyCode as winit_key;
        use egui::Key as egui_key;
        match winit_button {
            winit_key::Key0 | winit_key::Numpad0 => Some(egui_key::Num0),
            winit_key::Key1 | winit_key::Numpad1 => Some(egui_key::Num1),
            winit_key::Key2 | winit_key::Numpad2 => Some(egui_key::Num2),
            winit_key::Key3 | winit_key::Numpad3 => Some(egui_key::Num5),
            winit_key::Key4 | winit_key::Numpad4 => Some(egui_key::Num6),
            winit_key::Key5 | winit_key::Numpad5 => Some(egui_key::Num4),
            winit_key::Key6 | winit_key::Numpad6 => Some(egui_key::Num3),
            winit_key::Key7 | winit_key::Numpad7 => Some(egui_key::Num7),
            winit_key::Key8 | winit_key::Numpad8 => Some(egui_key::Num8),
            winit_key::Key9 | winit_key::Numpad9 => Some(egui_key::Num9),

            winit_key::Up => Some(egui_key::ArrowUp),
            winit_key::Down => Some(egui_key::ArrowDown),
            winit_key::Left => Some(egui_key::ArrowLeft),
            winit_key::Right => Some(egui_key::ArrowRight),

            winit_key::PageUp => Some(egui_key::PageUp),
            winit_key::PageDown => Some(egui_key::PageDown),

            winit_key::Home => Some(egui_key::Home),
            winit_key::End => Some(egui_key::End),

            winit_key::NumpadEnter | winit_key::Return => Some(egui_key::Enter),

            winit_key::Escape => Some(egui_key::Escape),

            winit_key::Space => Some(egui_key::Space),
            winit_key::Tab => Some(egui_key::Tab),

            winit_key::Delete => Some(egui_key::Delete),
            winit_key::Back => Some(egui_key::Backspace),

            winit_key::Insert => Some(egui_key::Insert),

            //Help
            winit_key::A => Some(egui_key::A),
            winit_key::B => Some(egui_key::B),
            winit_key::C => Some(egui_key::C),
            winit_key::D => Some(egui_key::D),
            winit_key::E => Some(egui_key::E),
            winit_key::F => Some(egui_key::F),
            winit_key::G => Some(egui_key::G),
            winit_key::H => Some(egui_key::H),
            winit_key::I => Some(egui_key::I),
            winit_key::J => Some(egui_key::J),
            winit_key::K => Some(egui_key::K),
            winit_key::L => Some(egui_key::L),
            winit_key::M => Some(egui_key::M),
            winit_key::N => Some(egui_key::N),
            winit_key::O => Some(egui_key::O),
            winit_key::P => Some(egui_key::P),
            winit_key::Q => Some(egui_key::Q),
            winit_key::R => Some(egui_key::R),
            winit_key::S => Some(egui_key::S),
            winit_key::T => Some(egui_key::T),
            winit_key::U => Some(egui_key::U),
            winit_key::V => Some(egui_key::V),
            winit_key::W => Some(egui_key::W),
            winit_key::X => Some(egui_key::X),
            winit_key::Y => Some(egui_key::Y),
            winit_key::Z => Some(egui_key::Z),
            
            _ => {
                eprintln!("Unimplemented Key {winit_button:?}");
                None
            },
        }
    }
    pub fn is_empty(&self) -> bool {
        self.is_empty
    }
    pub fn take_raw_input(&mut self) -> egui::RawInput {
        self.is_empty = true;
        egui::RawInput {
            modifiers : self.last_modifiers,
            events: std::mem::take(&mut self.events),
            focused: self.has_focus,
            //Unclear whether this should be taken or cloned.
            hovered_files: std::mem::take(&mut self.hovered_files),
            dropped_files: std::mem::take(&mut self.dropped_files),

            predicted_dt: 1.0 / 60.0,
            time: None,

            screen_rect: self.screen_rect,
            pixels_per_point: Some(self.pixels_per_point),
            max_texture_side: Some(4096),
            //We cannot know yet!
            ..Default::default()
        }
    }
}
impl Default for EguiEventAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn egui_to_winit_cursor(cursor : egui::CursorIcon) -> Option<winit::window::CursorIcon> {
    use egui::CursorIcon as GuiCursor;
    use winit::window::CursorIcon as WinCursor;
    match cursor {
        GuiCursor::Alias => Some(WinCursor::Alias),
        GuiCursor::AllScroll => Some(WinCursor::AllScroll),
        GuiCursor::Cell => Some(WinCursor::Cell),
        GuiCursor::ContextMenu => Some(WinCursor::ContextMenu),
        GuiCursor::Copy => Some(WinCursor::Copy),
        GuiCursor::Crosshair => Some(WinCursor::Crosshair),
        GuiCursor::Default => Some(WinCursor::Default),
        GuiCursor::Grab => Some(WinCursor::Grab),
        GuiCursor::Grabbing => Some(WinCursor::Grabbing),
        GuiCursor::Help => Some(WinCursor::Help),
        GuiCursor::Move => Some(WinCursor::Move),
        GuiCursor::NoDrop => Some(WinCursor::NoDrop),
        GuiCursor::None => None,
        GuiCursor::NotAllowed => Some(WinCursor::NotAllowed),
        GuiCursor::PointingHand => Some(WinCursor::Hand),
        GuiCursor::Progress => Some(WinCursor::Progress),
        GuiCursor::ResizeColumn => Some(WinCursor::ColResize),
        GuiCursor::ResizeEast => Some(WinCursor::EResize),
        GuiCursor::ResizeHorizontal => Some(WinCursor::EwResize),
        GuiCursor::ResizeNeSw => Some(WinCursor::NeswResize),
        GuiCursor::ResizeNorth => Some(WinCursor::NResize),
        GuiCursor::ResizeNorthEast => Some(WinCursor::NeResize),
        GuiCursor::ResizeNorthWest => Some(WinCursor::NwResize),
        GuiCursor::ResizeNwSe => Some(WinCursor::NwseResize),
        GuiCursor::ResizeRow => Some(WinCursor::RowResize),
        GuiCursor::ResizeSouth => Some(WinCursor::SResize),
        GuiCursor::ResizeSouthEast => Some(WinCursor::SeResize),
        GuiCursor::ResizeSouthWest => Some(WinCursor::SwResize),
        GuiCursor::ResizeVertical => Some(WinCursor::NsResize),
        GuiCursor::ResizeWest => Some(WinCursor::WResize),
        GuiCursor::Text => Some(WinCursor::Text),
        GuiCursor::VerticalText => Some(WinCursor::VerticalText),
        GuiCursor::Wait => Some(WinCursor::Wait),
        GuiCursor::ZoomIn => Some(WinCursor::ZoomIn),
        GuiCursor::ZoomOut => Some(WinCursor::ZoomOut),
    }
}

use anyhow::{Result as AnyResult, Context};
mod fs {
    vulkano_shaders::shader!{
        ty: "fragment",
        src:
        r"#version 460

        layout(binding = 0, set = 0) uniform sampler2D tex;

        layout(location = 0) in vec2 uv;
        layout(location = 1) in vec4 vertex_color;
        layout(location = 0) out vec4 out_color;

        void main() {
            out_color = vertex_color * texture(tex, uv);
        }",
    }
}
mod vs {
    vulkano_shaders::shader!{
        ty: "vertex",
        src:
        r"#version 460

        layout(push_constant) uniform Matrix {
            mat4 ortho;
        } matrix;

        layout(location = 0) in vec2 pos;
        layout(location = 1) in vec4 color;
        layout(location = 2) in vec2 uv;

        layout(location = 0) out vec2 out_uv;
        layout(location = 1) out vec4 vertex_color;

        void main() {
            gl_Position = matrix.ortho * vec4(pos, 0.0, 1.0);
            out_uv = uv;
            //Color is premultiplied. Undo that
            vertex_color = color.a == 0 ? vec4(0.0) : vec4(color.rgb/color.a, color.a);
        }",
    }
}
#[derive(vk::BufferContents, vk::Vertex)]
#[repr(C)]
struct EguiVertex {
    #[format(R32G32_SFLOAT)]
    pos : [f32; 2],
    #[format(R8G8B8A8_UNORM)]
    color : [u8; 4],
    #[format(R32G32_SFLOAT)]
    uv : [f32; 2],
}
impl From<egui::epaint::Vertex> for EguiVertex {
    fn from(value: egui::epaint::Vertex) -> Self {
        Self {
            pos : value.pos.into(),
            color: value.color.to_array(),
            uv: value.uv.into()
        }
    }
}
struct EguiTexture {
    image : Arc<vk::StorageImage>,
    view : Arc<vk::ImageView<vk::StorageImage>>,
    sampler: Arc<vk::Sampler>,

    descriptor_set: Arc<vk::PersistentDescriptorSet>,
}
pub struct EguiRenderer {
    images : std::collections::HashMap<egui::TextureId, EguiTexture>,
    render_context : Arc<super::RenderContext>,

    render_pass : Arc<vk::RenderPass>,
    pipeline: Arc<vk::GraphicsPipeline>,
    framebuffers: Vec<Arc<vk::Framebuffer>>,
}
impl EguiRenderer {
    pub fn new(render_context: Arc<super::RenderContext>, surface_format: vk::Format) -> GpuResult<Self> {
        let device = render_context.device.clone();
        let renderpass = vulkano::single_pass_renderpass!(
            device.clone(),
            attachments : {
                swapchain_color : {
                    load: Clear,
                    store: Store,
                    format: surface_format,
                    samples: 1,
                },
            },
            pass: {
                color: [swapchain_color],
                depth_stencil: {},
            },
        ).fatal()?;

        let fragment = fs::load(device.clone()).fatal()?;
        let vertex = vs::load(device.clone()).fatal()?;

        let fragment_entry = fragment.entry_point("main").unwrap();
        let vertex_entry = vertex.entry_point("main").unwrap();

        let pipeline = vk::GraphicsPipeline::start()
            .vertex_shader(vertex_entry, vs::SpecializationConstants::default())
            .fragment_shader(fragment_entry, fs::SpecializationConstants::default())
            .vertex_input_state(EguiVertex::per_vertex())
            .render_pass(vk::Subpass::from(renderpass.clone(), 0).unwrap())
            .rasterization_state(
                vk::RasterizationState{
                    cull_mode: vk::StateMode::Fixed(vk::CullMode::None),
                    ..Default::default()
                }
            )
            .input_assembly_state(
                vk::InputAssemblyState {
                    topology: vk::PartialStateMode::Fixed(vk::PrimitiveTopology::TriangleList),
                    primitive_restart_enable: vk::StateMode::Fixed(false),
                }
            )
            .color_blend_state(
                vk::ColorBlendState::new(1).blend_alpha()
            )
            .viewport_state(
                vk::ViewportState::Dynamic {
                    count: 1,
                    viewport_count_dynamic: false,
                    scissor_count_dynamic: false,
                }
            )
            .build(render_context.device.clone())
            .fatal()?;

        Ok(
            Self {
                images: Default::default(),
                render_pass: renderpass,
                pipeline,
                render_context: render_context.clone(),
                framebuffers: Vec::new(),
            }
        )
    }
    pub fn gen_framebuffers(&mut self, surface: &super::RenderSurface) -> GpuResult<()> {
        let framebuffers : AnyResult<Vec<_>> =
            surface.swapchain_images
            .iter()
            .map(|image| -> AnyResult<_> {
                let fb = vk::Framebuffer::new(
                    self.render_pass.clone(),
                    vk::FramebufferCreateInfo {
                        attachments: vec![
                            vk::ImageView::new_default(image.clone())?
                        ],
                        ..Default::default()
                    }
                )?;

                Ok(fb)
            }).collect();
        
        //Treat error as fatal
        self.framebuffers = framebuffers.map_gpu_err(|err| {
            (true, GpuRemedy::BlameTheDev)
        })?;

        Ok(())
    }
    pub fn upload_and_render(
        &self,
        present_img_index: u32,
        tesselated_geom: &[egui::epaint::ClippedPrimitive],
    ) -> GpuResult<vk::PrimaryAutoCommandBuffer> {
        let mut vert_buff_size = 0;
        let mut index_buff_size = 0;
        for clipped in tesselated_geom {
            match &clipped.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    vert_buff_size += mesh.vertices.len();
                    index_buff_size += mesh.indices.len();
                },
                egui::epaint::Primitive::Callback(..) => {
                    //Todo. But I'm not sure I mind this feature being unimplemented :P
                    unimplemented!("Primitive Callback is not supported.");
                },
            }
        }

        if vert_buff_size == 0 || index_buff_size == 0 {
            let builder = vk::AutoCommandBufferBuilder::primary(
                &self.render_context.command_buffer_alloc,
                self.render_context.queues.graphics().idx(),
                vk::CommandBufferUsage::OneTimeSubmit
            ).fatal()?;
            return Ok(
                builder.build().fatal()?
            )
        }

        let mut vertex_vec = Vec::with_capacity(vert_buff_size);
        let mut index_vec = Vec::with_capacity(index_buff_size);


        for clipped in tesselated_geom {
            if let egui::epaint::Primitive::Mesh(mesh) = &clipped.primitive {
                vertex_vec.extend(
                    mesh.vertices.iter()
                    .cloned()
                    .map(EguiVertex::from)
                );
                index_vec.extend_from_slice(&mesh.indices);
            }
        }
        let vertices = vk::Buffer::from_iter(
            &self.render_context.memory_alloc,
            vk::BufferCreateInfo {
                usage: vk::BufferUsage::VERTEX_BUFFER,
                ..Default::default()
            },
            vk::AllocationCreateInfo {
                usage: vk::MemoryUsage::Upload,
                ..Default::default()
            },
            vertex_vec
        ).fatal()?;
        let indices = vk::Buffer::from_iter(
            &self.render_context.memory_alloc,
            vk::BufferCreateInfo {
                usage: vk::BufferUsage::INDEX_BUFFER,
                ..Default::default()
            },
            vk::AllocationCreateInfo {
                usage: vk::MemoryUsage::Upload,
                ..Default::default()
            },
            index_vec
        ).fatal()?;

        let framebuffer = self.framebuffers.get(present_img_index as usize).expect("Present image out-of-bounds.").clone();

        let matrix = cgmath::ortho(0.0, framebuffer.extent()[0] as f32, 0.0, framebuffer.extent()[1] as f32, -1.0, 1.0);

        let (texture_set_idx, _) = self.texture_set_layout();
        let pipeline_layout = self.pipeline.layout();

        let mut command_buffer_builder = vk::AutoCommandBufferBuilder::primary(
                &self.render_context.command_buffer_alloc,
                self.render_context.queues.graphics().idx(),
                vk::CommandBufferUsage::OneTimeSubmit
            )?;
        command_buffer_builder
            .begin_render_pass(
                vk::RenderPassBeginInfo{
                    clear_values: vec![
                        Some(
                            vk::ClearValue::Float([0.2, 0.2, 0.2, 1.0])
                        )
                    ],
                    ..vk::RenderPassBeginInfo::framebuffer(
                        framebuffer.clone()
                    )
                },
                vk::SubpassContents::Inline
            )?
            .bind_pipeline_graphics(self.pipeline.clone())
            .bind_vertex_buffers(0, [vertices])
            .bind_index_buffer(indices)
            .set_viewport(
                0,
                [vk::Viewport{
                    depth_range: 0.0..1.0,
                    dimensions: framebuffer.extent().map(|dim| dim as f32),
                    origin: [0.0; 2],
                }]
            )
            .push_constants(pipeline_layout.clone(), 0, vs::Matrix{
                ortho: matrix.into()
            });

        let mut start_vertex_buffer_offset : usize = 0;
        let mut start_index_buffer_offset : usize = 0;


        for clipped in tesselated_geom {
            if let egui::epaint::Primitive::Mesh(mesh) = &clipped.primitive {
                // *Technically* it wants a float scissor rect. But.. oh well
                let origin = clipped.clip_rect.left_top();
                let origin = [
                    origin.x.max(0.0) as u32,
                    origin.y.max(0.0) as u32
                ];

                let dimensions = clipped.clip_rect.size();
                let dimensions = [
                    dimensions.x as u32,
                    dimensions.y as u32
                ];

                command_buffer_builder
                    .set_scissor(
                        0,
                        [
                            vk::Scissor{
                                origin,
                                dimensions
                            }
                        ]
                    )
                    //Maybe there's a better way than rebinding every draw.
                    //shaderSampledImageArrayDynamicIndexing perhaps?
                    .bind_descriptor_sets(
                        self.pipeline.bind_point(),
                        pipeline_layout.clone(),
                        texture_set_idx,
                        self.images.get(&mesh.texture_id)
                            .expect("Egui draw requested non-existent texture")
                            .descriptor_set.clone()
                    )
                    .draw_indexed(
                        mesh.indices.len() as u32,
                        1,
                        start_index_buffer_offset as u32,
                        start_vertex_buffer_offset as i32,
                        0
                    )?;
                start_index_buffer_offset += mesh.indices.len();
                start_vertex_buffer_offset += mesh.vertices.len();
            }
        }

        command_buffer_builder.end_render_pass()?;
        let command_buffer = command_buffer_builder.build()?;

        Ok(command_buffer)
    }
    ///Get the descriptor set layout for the texture uniform. (set_idx, layout)
    fn texture_set_layout(&self) -> (u32, Arc<vk::DescriptorSetLayout>) {
        let pipe_layout = self.pipeline.layout();
        let layout = pipe_layout.set_layouts().get(0).expect("Egui shader needs a sampler!").clone();
        (0, layout)
    }
    /// Apply image deltas, optionally returning a command buffer filled with any
    /// transfers as needed.
    pub fn do_image_deltas(
        &mut self,
        deltas : egui::TexturesDelta
    )  -> Option<GpuResult<vk::PrimaryAutoCommandBuffer>> {
        for free in deltas.free.iter() {
            self.images.remove(&free).unwrap();
        }

        if deltas.set.is_empty() {
            None
        } else {
            Some(
                self.do_image_deltas_set(deltas)
            )
        }
    }
    fn do_image_deltas_set(
        &mut self,
        deltas : egui::TexturesDelta,
    ) -> GpuResult<vk::PrimaryAutoCommandBuffer> {
        //Free is handled by do_image_deltas

        //Pre-allocate on the heap so we don't end up re-allocating a bunch as we populate
        let mut total_delta_size = 0;
        for (_, delta) in &deltas.set {
            total_delta_size += match &delta.image {
                egui::ImageData::Color(color) => color.width() * color.height() * 4,
                //We'll covert to 8bpp on upload
                egui::ImageData::Font(grey) => grey.width() * grey.height() * 1,
            };
        }

        let mut data_vec = Vec::with_capacity(total_delta_size);
        for (_, delta) in &deltas.set {
            match &delta.image {
                egui::ImageData::Color(data) => {
                    data_vec.extend_from_slice(bytemuck::cast_slice(&data.pixels[..]));
                }
                egui::ImageData::Font(data) => {
                    //Convert f32 image to u8 norm image
                    data_vec.extend(
                        data.pixels.iter()
                            .map(|&f| {
                                (f * 255.0).clamp(0.0, 255.0) as u8
                            })
                    );
                }
            }
        }

        //This is  dumb. Why can't i use the data directly? It's a slice of [u8]. Maybe (hopefully) it optimizes out?
        //TODO: Maybe mnually implement unsafe trait BufferContents to allow this without byte-by-byte iterator copying.
        let staging_buffer = vk::Buffer::from_iter(
            &self.render_context.memory_alloc,
            vk::BufferCreateInfo {
                sharing: vk::Sharing::Exclusive,
                usage: vk::BufferUsage::TRANSFER_SRC,
                ..Default::default()
            },
            vk::AllocationCreateInfo {
                usage: vk::MemoryUsage::Upload,
                ..Default::default()
            },
            data_vec.into_iter()
        )?;

        let mut command_buffer =
            vk::AutoCommandBufferBuilder::primary(
                &self.render_context.command_buffer_alloc,
                self.render_context.queues.transfer().idx(),
                vk::CommandBufferUsage::OneTimeSubmit
            )?;
        
        //In case we need to allocate new textures.
        let (texture_set_idx, texture_set_layout) = self.texture_set_layout();

        let mut current_base_offset = 0;
        for (id, delta) in &deltas.set {
            let entry = self.images.entry(*id);
            //Generate if non-existent yet!
            let image : AnyResult<_> = match entry {
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    let format = match delta.image {
                        egui::ImageData::Color(_) => vk::Format::R8G8B8A8_UNORM,
                        egui::ImageData::Font(_) => vk::Format::R8_UNORM,
                    };
                    let dimensions = {
                        let mut dimensions = delta.pos.unwrap_or([0, 0]);
                        dimensions[0] += delta.image.width();
                        dimensions[1] += delta.image.height();

                        vk::ImageDimensions::Dim2d {
                            width: dimensions[0] as u32,
                            height: dimensions[1] as u32,
                            array_layers: 1
                        }
                    };
                    let image = vk::StorageImage::with_usage(
                        &self.render_context.memory_alloc,
                        dimensions,
                        format,
                        //We will not be using this StorageImage for storage :P
                        vk::ImageUsage::TRANSFER_DST | vk::ImageUsage::SAMPLED,
                        vk::ImageCreateFlags::empty(),
                        std::iter::empty() //A puzzling difference in API from buffers - this just means Exclusive access.
                    )?;

                    let egui_to_vk_filter = |egui_filter : egui::epaint::textures::TextureFilter| {
                        match egui_filter {
                            egui::TextureFilter::Linear => vk::Filter::Linear,
                            egui::TextureFilter::Nearest => vk::Filter::Nearest,
                        }
                    };
                    
                    let mapping = if let egui::ImageData::Font(_) = delta.image {
                        //Font is one channel, representing percent coverage of white.
                        vk::ComponentMapping {
                            a: vk::ComponentSwizzle::Red,
                            r: vk::ComponentSwizzle::One,
                            g: vk::ComponentSwizzle::One,
                            b: vk::ComponentSwizzle::One,
                        }
                    } else {
                        vk::ComponentMapping::identity()
                    };

                    let view = vk::ImageView::new(
                        image.clone(),
                        vk::ImageViewCreateInfo {
                            component_mapping: mapping,
                            ..vk::ImageViewCreateInfo::from_image(&image)
                        }
                    )?;

                    //Could optimize here, re-using the four possible options of sampler.
                    let sampler = vk::Sampler::new(
                        self.render_context.device.clone(),
                        vk::SamplerCreateInfo {
                            mag_filter: egui_to_vk_filter(delta.options.magnification),
                            min_filter: egui_to_vk_filter(delta.options.minification),

                            ..Default::default()
                        }
                    )?;

                    let descriptor_set = vk::PersistentDescriptorSet::new(
                        &self.render_context.descriptor_set_alloc,
                        texture_set_layout.clone(), 
                        [
                            vk::WriteDescriptorSet::image_view_sampler(
                                texture_set_idx, view.clone(), sampler.clone()
                            )
                        ]
                    )?;
                    Ok(
                        vacant.insert(         
                            EguiTexture {
                                image,
                                view,
                                sampler,
                                descriptor_set
                            }
                        ).image.clone()
                    )
                },
                std::collections::hash_map::Entry::Occupied(occupied) => {
                    Ok(occupied.get().image.clone())
                }
            };
            let image = image.context("Failed to allocate Egui texture")?;

            let size = match &delta.image {
                egui::ImageData::Color(color) => color.width() * color.height() * 4,
                egui::ImageData::Font(grey) => grey.width() * grey.height() * 1,
            };
            let start_offset = current_base_offset as u64;
            current_base_offset += size;

            //The only way to get a struct of this is to call this method -
            //we need to redo many of the fields however.
            let transfer_info = 
                vk::CopyBufferToImageInfo::buffer_image(
                    staging_buffer.clone(),
                    image
                );
            
            let transfer_offset = delta.pos.unwrap_or([0, 0]);

            command_buffer
                .copy_buffer_to_image(
                    vk::CopyBufferToImageInfo {
                        //Update regions according to delta
                        regions: smallvec::smallvec![
                            vk::BufferImageCopy {
                                buffer_offset: start_offset,
                                image_offset: [
                                    transfer_offset[0] as u32,
                                    transfer_offset[1] as u32,
                                    0
                                ],
                                buffer_image_height: delta.image.height() as u32,
                                buffer_row_length: delta.image.width() as u32,
                                image_extent: [
                                    delta.image.width() as u32,
                                    delta.image.height() as u32,
                                    1
                                ],
                                ..transfer_info.regions[0].clone()
                            }
                        ],
                        ..transfer_info
                    }
                )?;
        }

        Ok(
            command_buffer.build()?
        )
    }
}