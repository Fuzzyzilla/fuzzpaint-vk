use crate::*;

mod shaders {
    pub mod vertex {
        vulkano_shaders::shader! {
            ty: "vertex",
            src:r"
            #version 460
            
            layout(push_constant) uniform Matrix {
                mat4 mat;
            } matrix;

            layout(location = 0) out vec2 out_uv;

            void main() {
                vec4 pos = vec4(
                    float(gl_VertexIndex & 1),
                    float(gl_VertexIndex & 2),
                    0.0,
                    1.0
                );
                out_uv = vec2(pos.x, 1.0 - pos.y);
                gl_Position = matrix.mat * pos;
            }"
        }
    }
    pub mod fragment {
        vulkano_shaders::shader! {
            ty: "fragment",
            src:r"
            #version 460

            const float LIGHT = 0.8;
            const float DARK = 0.7;
            const uint SIZE = uint(16);

            layout(set = 0, binding = 0) uniform sampler2D image;

            layout(location = 0) in vec2 uv;

            layout(location = 0) out vec4 color;

            void main() {
                uvec2 grid_coords = uvec2(gl_FragCoord.xy) / SIZE;
                bool is_light = (grid_coords.x + grid_coords.y) % 2 == 0;
                vec3 grid_color = vec3(vec3(is_light ? LIGHT : DARK));

                vec4 col = texture(image, uv);
                color = vec4(grid_color * (1.0 - col.a) + col.rgb, 1.0);
            }"
        }
    }
}

/// An acquired image from the proxy. Will become the current image when dropped,
/// after a user-provided GPU future.
struct DocumentViewportPreviewProxyGuard {

}

/// An double-buffering interface between the asynchronous edit->render pipeline of documents
/// and the synchronous redrawing of the many swapchain images.
/// (Because dealing with one image is easier than potentially many, as we don't care about excess framerate)
pub struct DocumentViewportPreviewProxy {
    render_context: Arc<render_device::RenderContext>,

    document_images: [Arc<vk::ImageView<vk::StorageImage>>; 2],
    document_image_bindings: [Arc<vk::PersistentDescriptorSet>; 2],

    read_buf: std::sync::atomic::AtomicU8,

    render_pass: Arc<vk::RenderPass>,
    // List of framebuffers for the swapchain, lazily created as they're needed.
    framebuffers: Vec<Arc<vk::Framebuffer>>,
    prerecorded_command_buffers: Vec<[Arc<vk::PrimaryAutoCommandBuffer>; 2]>,

    viewport_dimensions: [u32; 2],

    pipeline: Arc<vk::GraphicsPipeline>,

    document_to_preview_matrix: cgmath::Matrix4<f32>,
    transform_matrix: [[f32; 4]; 4],
}

impl DocumentViewportPreviewProxy {
    pub fn new(render_surface: &render_device::RenderSurface) -> AnyResult<Self> {
        let render_pass = vulkano::single_pass_renderpass!(
            render_surface.context().device().clone(),
            attachments: {
                document: {
                    load: Clear,
                    store: Store,
                    format: render_surface.format(),
                    samples: 1,
                },
            },
            pass: {
                color: [document],
                depth_stencil: {},
            },
        )?;

        // Only one frame-in-flight - Keep an additional buffer for writing to.
        let num_document_buffers = 2u32;

        let document_image_array = vk::StorageImage::with_usage(
            render_surface.context().allocators().memory(),
            vk::ImageDimensions::Dim2d {
                width: crate::DOCUMENT_DIMENSION,
                height: crate::DOCUMENT_DIMENSION,
                array_layers: num_document_buffers,
            },
            vk::Format::R16G16B16A16_SFLOAT,
            vk::ImageUsage::COLOR_ATTACHMENT
                | vk::ImageUsage::SAMPLED
                | vk::ImageUsage::TRANSFER_DST
                | vk::ImageUsage::STORAGE,
            vk::ImageCreateFlags::empty(),
            [render_surface.context().queues().graphics().idx()],
        )?;

        let document_image_views = [
            vk::ImageView::new(
                document_image_array.clone(),
                vk::ImageViewCreateInfo {
                    subresource_range: vk::ImageSubresourceRange {
                        array_layers: 0..1,
                        aspects: vk::ImageAspects::COLOR,
                        mip_levels: 0..1,
                    },
                    view_type: vulkano::image::view::ImageViewType::Dim2d,
                    ..vk::ImageViewCreateInfo::from_image(&document_image_array)
                },
            )?,
            vk::ImageView::new(
                document_image_array.clone(),
                vk::ImageViewCreateInfo {
                    subresource_range: vk::ImageSubresourceRange {
                        array_layers: 1..2,
                        aspects: vk::ImageAspects::COLOR,
                        mip_levels: 0..1,
                    },
                    view_type: vulkano::image::view::ImageViewType::Dim2d,
                    ..vk::ImageViewCreateInfo::from_image(&document_image_array)
                },
            )?,
        ];

        let sampler = vk::Sampler::new(
            render_surface.context().device().clone(),
            vk::SamplerCreateInfo {
                min_filter: vk::Filter::Linear,
                mag_filter: vk::Filter::Nearest,
                ..Default::default()
            },
        )?;

        let vertex_shader = shaders::vertex::load(render_surface.context().device().clone())?;
        let fragment_shader = shaders::fragment::load(render_surface.context().device().clone())?;

        // "main" is the only valid GLSL entry point name, ok to unwrap.
        let vertex_shader = vertex_shader.entry_point("main").unwrap();
        let fragment_shader = fragment_shader.entry_point("main").unwrap();

        let mut no_blend = vk::ColorBlendState::new(1);
        no_blend.attachments[0].blend = None;

        let size = render_surface.extent();

        let pipeline = vk::GraphicsPipeline::start()
            .vertex_shader(vertex_shader.clone(), ())
            .fragment_shader(fragment_shader, ())
            .vertex_input_state(vulkano::pipeline::graphics::vertex_input::VertexInputState::new())
            .rasterization_state(
                vk::RasterizationState::default()
                    .cull_mode(vulkano::pipeline::graphics::rasterization::CullMode::None),
            )
            .input_assembly_state(
                vk::InputAssemblyState::new().topology(vk::PrimitiveTopology::TriangleStrip),
            )
            .color_blend_state(no_blend)
            .render_pass(render_pass.clone().first_subpass())
            .viewport_state(vk::ViewportState::viewport_dynamic_scissor_irrelevant())
            .build(render_surface.context().device().clone())?;

        let document_image_bindings = [
            vk::PersistentDescriptorSet::new(
                render_surface.context().allocators().descriptor_set(),
                pipeline.layout().set_layouts()[0].clone(),
                [vk::WriteDescriptorSet::image_view_sampler(
                    0,
                    document_image_views[0].clone(),
                    sampler.clone(),
                )],
            )?,
            vk::PersistentDescriptorSet::new(
                render_surface.context().allocators().descriptor_set(),
                pipeline.layout().set_layouts()[0].clone(),
                [vk::WriteDescriptorSet::image_view_sampler(
                    0,
                    document_image_views[1].clone(),
                    sampler,
                )],
            )?,
        ];

        let margin = 25.0;

        //Total size, to "fit" image. Use the smallest of both dimensions.
        let image_size_px = size[0].min(size[1]) as f32 - (2.0 * margin);
        let x = (size[0] as f32 - image_size_px) / 2.0;
        let y = (size[1] as f32 - image_size_px) / 2.0;
        let document_to_preview_matrix =
            Matrix4::from_translation(cgmath::Vector3 { x, y, z: 0.0 })
                * Matrix4::from_scale(image_size_px as f32);

        let xform = cgmath::ortho(0.0, size[0] as f32, size[1] as f32, 0.0, -1.0, 1.0)
            * document_to_preview_matrix;

        let mut s = Self {
            render_context: render_surface.context().clone(),
            framebuffers: Vec::new(),
            prerecorded_command_buffers: Vec::new(),
            pipeline,
            render_pass,
            transform_matrix: xform.into(),
            document_to_preview_matrix: document_to_preview_matrix,
            viewport_dimensions: size,

            read_buf: 0.into(),

            document_images: document_image_views,
            document_image_bindings,
        };
        s.surface_changed(render_surface);

        Ok(s)
    }
    fn recalc_matrix(&mut self) {
        let size = self.viewport_dimensions;
        let margin = 25.0;
        //Total size, to "fit" image. Use the smallest of both dimensions.
        let image_size_px = size[0].min(size[1]) as f32 - (2.0 * margin);
        let x = (size[0] as f32 - image_size_px) / 2.0;
        let y = (size[1] as f32 - image_size_px) / 2.0;
        let document_to_preview_matrix =
            Matrix4::from_translation(cgmath::Vector3 { x, y, z: 0.0 })
                * Matrix4::from_scale(image_size_px as f32);

        let xform = cgmath::ortho(0.0, size[0] as f32, size[1] as f32, 0.0, -1.0, 1.0)
            * document_to_preview_matrix;

        self.document_to_preview_matrix = document_to_preview_matrix;
        self.transform_matrix = xform.into();
    }
    fn record_commandbuffers(&mut self) {
        //Drop old buffers-- RESOURCES MIGHT STILL BE IN USE ON ANOTHER THREAD!
        self.prerecorded_command_buffers = Vec::new();

        if self.framebuffers.is_empty() {
            log::error!("Cannot record commandbuffers with no framebuffers");
        }
        let command_buffers: AnyResult<Vec<_>> = self
            .framebuffers
            .iter()
            .map(
                |framebuffer| -> AnyResult<[vk::PrimaryAutoCommandBuffer; 2]> {
                    let command_buffers = [
                        vk::AutoCommandBufferBuilder::primary(
                            self.render_context.allocators().command_buffer(),
                            self.render_context.queues().graphics().idx(),
                            command_buffer::CommandBufferUsage::MultipleSubmit,
                        )?,
                        vk::AutoCommandBufferBuilder::primary(
                            self.render_context.allocators().command_buffer(),
                            self.render_context.queues().graphics().idx(),
                            command_buffer::CommandBufferUsage::MultipleSubmit,
                        )?,
                    ];

                    let mut command_buffers = command_buffers.into_iter().enumerate().map(
                        |(idx, mut buffer)| -> AnyResult<vk::PrimaryAutoCommandBuffer> {
                            buffer
                                .begin_render_pass(
                                    vk::RenderPassBeginInfo {
                                        clear_values: vec![Some([0.05, 0.05, 0.05, 1.0].into())],
                                        ..vk::RenderPassBeginInfo::framebuffer(framebuffer.clone())
                                    },
                                    command_buffer::SubpassContents::Inline,
                                )?
                                .bind_pipeline_graphics(self.pipeline.clone())
                                .bind_descriptor_sets(
                                    vulkano::pipeline::PipelineBindPoint::Graphics,
                                    self.pipeline.layout().clone(),
                                    0,
                                    vec![self.document_image_bindings[idx].clone()],
                                )
                                .set_viewport(
                                    0,
                                    [vk::Viewport {
                                        depth_range: 0.0..1.0,
                                        dimensions: [
                                            self.viewport_dimensions[0] as f32,
                                            self.viewport_dimensions[1] as f32,
                                        ],
                                        origin: [0.0; 2],
                                    }],
                                )
                                .push_constants(
                                    self.pipeline.layout().clone(),
                                    0,
                                    shaders::vertex::Matrix {
                                        mat: self.transform_matrix,
                                    },
                                )
                                .draw(6, 1, 0, 0)?
                                .end_render_pass()?;

                            Ok(buffer.build()?)
                        },
                    );

                    Ok([
                        command_buffers.next().unwrap()?,
                        command_buffers.next().unwrap()?,
                    ])
                },
            )
            .collect();

        match command_buffers {
            Ok(buffers) => {
                self.prerecorded_command_buffers = buffers
                    .into_iter()
                    .map(|[a, b]| [Arc::new(a), Arc::new(b)])
                    .collect();
            }
            Err(e) => {
                log::error!("Failed to record preview command buffers: {e:?}");
            }
        }
    }
    pub async fn get_writeable_buffer(&self) -> Arc<vk::ImageView<vk::StorageImage>> {
        //Todo - wait for rendering on this image to complete.
        self.document_images
            [(self.read_buf.load(std::sync::atomic::Ordering::Acquire) ^ 1) as usize]
            .clone()
    }
    pub fn swap(&self) {
        self.read_buf
            .fetch_xor(0x01, std::sync::atomic::Ordering::Release);
    }
    pub fn get_matrix(&self) -> cgmath::Matrix4<f32> {
        self.document_to_preview_matrix
    }
}
impl crate::PreviewRenderProxy for DocumentViewportPreviewProxy {
    fn render<'a>(&'a mut self, idx: u32) -> AnyResult<Arc<vk::PrimaryAutoCommandBuffer>> {
        let Some(buffer) = self.prerecorded_command_buffers.get(idx as usize) else {
            anyhow::bail!("No buffer found for swapchain image {idx}!")
        };

        Ok(buffer[self.read_buf.load(std::sync::atomic::Ordering::Acquire) as usize].clone())
    }
    fn render_complete(&mut self, _idx: u32) {}
    fn surface_changed(&mut self, render_surface: &render_device::RenderSurface) {
        if render_surface.context().device() != self.pipeline.device() {
            panic!("Wrong device used to recreate preview proxy!")
        }

        self.framebuffers = Vec::new();
        let framebuffers: AnyResult<Vec<_>> = render_surface
            .swapchain_images()
            .iter()
            .map(|image| -> AnyResult<_> {
                // Todo: duplication of view resources.
                let view = vk::ImageView::new_default(image.clone())?;

                let framebuffer = vk::Framebuffer::new(
                    self.render_pass.clone(),
                    vk::FramebufferCreateInfo {
                        attachments: vec![view],
                        ..Default::default()
                    },
                );

                Ok(framebuffer?)
            })
            .collect();

        self.framebuffers = framebuffers.expect("Failed to create proxy framebuffers.");

        self.viewport_dimensions = render_surface.extent();
        self.recalc_matrix();
        //Todo: rebuild pipeline with new format/size, if changed.
        self.record_commandbuffers();
    }
}
