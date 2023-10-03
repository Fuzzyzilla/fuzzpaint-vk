use crate::egui_impl;
use crate::render_device;
use crate::vulkano_prelude::*;
use std::sync::Arc;

use anyhow::Result as AnyResult;

pub struct WindowSurface {
    event_loop: winit::event_loop::EventLoop<()>,
    win: Arc<winit::window::Window>,
}
impl WindowSurface {
    pub fn new() -> AnyResult<Self> {
        const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

        let event_loop = winit::event_loop::EventLoopBuilder::default().build();
        let win = winit::window::WindowBuilder::default()
            .with_title(format!("Fuzzpaint v{}", VERSION.unwrap_or("[unknown]")))
            .with_min_inner_size(winit::dpi::LogicalSize::new(500u32, 500u32))
            .with_transparent(false)
            .build(&event_loop)?;

        Ok(Self {
            event_loop: event_loop,
            win: Arc::new(win),
        })
    }
    pub fn window(&self) -> Arc<winit::window::Window> {
        self.win.clone()
    }
    pub fn with_render_surface(
        self,
        render_surface: render_device::RenderSurface,
        render_context: Arc<render_device::RenderContext>,
        preview_renderer: Arc<dyn crate::document_viewport_proxy::PreviewRenderProxy>,
        ui: crate::ui::MainUI,
    ) -> anyhow::Result<WindowRenderer> {
        let egui_ctx = egui_impl::EguiCtx::new(self.win.as_ref(), &render_surface)?;

        let (send, stream) = crate::actions::create_action_stream();

        Ok(WindowRenderer {
            win: self.win,
            render_surface: Some(render_surface),
            swapchain_generation: 0,
            render_context,
            event_loop: Some(self.event_loop),
            last_frame_fence: None,
            egui_ctx,
            ui,
            preview_renderer,
            action_collector:
                crate::actions::winit_action_collector::WinitKeyboardActionCollector::new(send),
            action_stream: stream,
            stylus_events: Default::default(),
        })
    }
}

pub struct WindowRenderer {
    event_loop: Option<winit::event_loop::EventLoop<()>>,
    win: Arc<winit::window::Window>,
    /// Always Some. This is to allow it to be take-able to be remade.
    /// Could None represent a temporary loss of surface that can be recovered from?
    render_surface: Option<render_device::RenderSurface>,
    render_context: Arc<render_device::RenderContext>,
    egui_ctx: egui_impl::EguiCtx,
    ui: crate::ui::MainUI,

    action_collector: crate::actions::winit_action_collector::WinitKeyboardActionCollector,
    action_stream: crate::actions::ActionStream,
    stylus_events: crate::stylus_events::WinitStylusEventCollector,
    swapchain_generation: u32,

    last_frame_fence: Option<vk::sync::future::FenceSignalFuture<Box<dyn GpuFuture>>>,

    preview_renderer: Arc<dyn crate::document_viewport_proxy::PreviewRenderProxy>,
}
impl WindowRenderer {
    pub fn window(&self) -> Arc<winit::window::Window> {
        self.win.clone()
    }
    pub fn action_listener(&self) -> crate::actions::ActionListener {
        self.action_stream.listen()
    }
    pub fn stylus_events(
        &self,
    ) -> tokio::sync::broadcast::Receiver<crate::stylus_events::StylusEventFrame> {
        self.stylus_events.frame_receiver()
    }
    /*
    pub fn gen_framebuffers(&mut self) {
        self.swapchain_framebuffers = Vec::with_capacity(self.render_surface.swapchain_images.len());

        self.swapchain_framebuffers.extend(
            self.render_surface.swapchain_images.iter()
                .map(|image| {
                    vulkano::render_pass::Framebuffer::
                })
        )
    }*/
    pub fn render_surface(&self) -> &render_device::RenderSurface {
        //this will ALWAYS be Some. The option is for taking from a mutable reference for recreation.
        &self.render_surface.as_ref().unwrap()
    }
    /// Recreate surface after loss or out-of-date. Todo: This only handles out-of-date and resize.
    pub fn recreate_surface(&mut self) -> AnyResult<()> {
        let new_surface = self
            .render_surface
            .take()
            .unwrap()
            .recreate(Some(self.window().inner_size().into()))?;

        self.egui_ctx.replace_surface(&new_surface)?;

        self.render_surface = Some(new_surface);
        self.swapchain_generation = self.swapchain_generation.wrapping_add(1);

        self.preview_renderer
            .surface_changed(self.render_surface.as_ref().unwrap());

        Ok(())
    }
    fn apply_document_cursor(&mut self) {
        // If egui did not assert a cursor, allow the document to provide an icon.
        // winit_egui handles egui's requests for cursor otherwise.
        if !self.egui_ctx.wants_pointer_input() {
            let cursor = self.preview_renderer.cursor();
            let cursor = cursor.unwrap_or(crate::gizmos::CursorOrInvisible::Icon(
                winit::window::CursorIcon::Default,
            ));

            if let crate::gizmos::CursorOrInvisible::Icon(i) = cursor {
                self.win.set_cursor_icon(i);
                self.win.set_cursor_visible(true);
            }
            if let crate::gizmos::CursorOrInvisible::Invisible = cursor {
                self.win.set_cursor_visible(false);
            }
        }
    }
    pub fn run(mut self) -> ! {
        //There WILL be an event loop if we got here
        let event_loop = self.event_loop.take().unwrap();
        self.window().request_redraw();

        event_loop.run(move |event, _, control_flow| {
            use winit::event::{Event, WindowEvent};
            match event {
                Event::WindowEvent { event, .. } => {
                    let consumed = self.egui_ctx.push_winit_event(&event).consumed;
                    if !consumed {
                        self.action_collector.push_event(&event);
                    }
                    match event {
                        WindowEvent::CloseRequested => {
                            *control_flow = winit::event_loop::ControlFlow::Exit;
                            return;
                        }
                        WindowEvent::Resized(..) => {
                            self.recreate_surface().expect("Failed to rebuild surface");
                        }
                        WindowEvent::CursorLeft { .. } => {
                            self.stylus_events.set_mouse_pressed(false);
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            // Only take if egui doesn't want it!
                            if !consumed {
                                self.stylus_events.push_position(position.into());
                            }
                        }
                        WindowEvent::MouseInput { state, .. } => {
                            let pressed = winit::event::ElementState::Pressed == state;

                            if pressed {
                                // Only take if egui doesn't want it!
                                if !consumed {
                                    self.stylus_events.set_mouse_pressed(true)
                                }
                            } else {
                                self.stylus_events.set_mouse_pressed(false)
                            }
                        }
                        _ => (),
                    }
                }
                Event::DeviceEvent { event, .. } => {
                    match event {
                        //Pressure out of 65535
                        winit::event::DeviceEvent::Motion { axis: 2, value } => {
                            self.stylus_events.set_pressure(value as f32 / 65535.0)
                        }
                        _ => (),
                    }
                    // 0 -> x in display space
                    // 1 -> y in display space
                    // 2 -> pressure out of 65535, 0 if not pressed
                    // 3 -> Tilt X, degrees from vertical, + to the right
                    // 4 -> Tilt Y, degrees from vertical, + towards user
                    // 5 -> unknown, always zero (rotation?)
                }
                Event::MainEventsCleared => {
                    //Draw!
                    self.do_ui();
                    self.apply_document_cursor();

                    if self.egui_ctx.needs_redraw() || self.preview_renderer.has_update() {
                        self.window().request_redraw()
                    }

                    self.stylus_events.finish();
                }
                Event::RedrawRequested(..) => {
                    if let Err(e) = self.paint() {
                        log::error!("{e:?}")
                    };
                }
                Event::RedrawEventsCleared => {
                    *control_flow = winit::event_loop::ControlFlow::WaitUntil(
                        std::time::Instant::now() + std::time::Duration::from_secs(2),
                    );
                }
                _ => (),
            }
        });
    }
    fn do_ui(&mut self) {
        let mut viewport = Default::default();
        self.egui_ctx
            .update(self.win.as_ref(), |ctx| viewport = self.ui.ui(ctx));

        // Todo: only change if... actually changed :P
        self.preview_renderer
            .viewport_changed(viewport.0, viewport.1);
    }
    fn paint(&mut self) -> AnyResult<()> {
        let (idx, suboptimal, image_future) =
            match vk::acquire_next_image(self.render_surface().swapchain().clone(), None) {
                Err(vulkano::swapchain::AcquireError::OutOfDate) => {
                    log::info!("Swapchain unusable. Recreating");
                    //We cannot draw on this surface as-is. Recreate and request another try next frame.
                    //TODO: Race condition, somehow! Surface is recreated with an out-of-date size.
                    self.recreate_surface()?;
                    self.window().request_redraw();
                    return Ok(());
                }
                Err(e) => {
                    //Todo. Many of these errors are recoverable!
                    anyhow::bail!("Surface image acquire failed! {e:?}");
                }
                Ok(r) => r,
            };

        let commands = self.egui_ctx.build_commands(idx);

        //Wait for previous frame to end. (required for safety of preview render proxy)
        self.last_frame_fence.take().map(|fence| fence.wait(None));

        let preview_commands = unsafe {
            self.preview_renderer.render(
                self.render_surface.as_ref().unwrap().swapchain_images()[idx as usize].clone(),
                idx,
            )
        };
        let preview_commands = match preview_commands {
            Ok(commands) => commands,
            Err(e) => {
                log::warn!("Failed to build preview commands {e:?}");
                Default::default()
            }
        };

        let render_complete = match commands {
            Some((Some(transfer), draw)) => {
                let transfer_future = self
                    .render_context
                    .now()
                    .then_execute(
                        self.render_context.queues().transfer().queue().clone(),
                        transfer,
                    )?
                    .boxed()
                    .then_signal_fence_and_flush()?;

                // Todo: no matter what I do, i cannot seem to get semaphores
                // to work. Ideally, the only thing that needs to wait is the
                // egui render commands, however it simply refuses to actually
                // wait for the semaphore. For now, I just stall the thread.
                transfer_future.wait(None)?;

                let mut future = image_future.boxed();

                for buffer in preview_commands.into_iter() {
                    future = future
                        .then_execute(
                            self.render_context.queues().graphics().queue().clone(),
                            buffer,
                        )?
                        .boxed();
                }

                future
                    .then_execute(
                        self.render_context.queues().graphics().queue().clone(),
                        draw,
                    )?
                    .boxed()
            }
            Some((None, draw)) => {
                let mut future = image_future.boxed();

                for buffer in preview_commands.into_iter() {
                    future = future
                        .then_execute(
                            self.render_context.queues().graphics().queue().clone(),
                            buffer,
                        )?
                        .boxed();
                }
                future.then_execute_same_queue(draw)?.boxed()
            }
            None => image_future.boxed(),
        };

        let next_frame_future = render_complete
            .then_swapchain_present(
                self.render_context
                    .queues()
                    .present()
                    .unwrap()
                    .queue()
                    .clone(),
                vk::SwapchainPresentInfo::swapchain_image_index(
                    self.render_surface.as_ref().unwrap().swapchain().clone(),
                    idx,
                ),
            )
            .boxed()
            .then_signal_fence_and_flush()?;

        self.last_frame_fence = Some(next_frame_future);

        if suboptimal {
            self.recreate_surface()?
        }

        Ok(())
    }
}
