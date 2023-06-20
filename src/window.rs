use std::sync::Arc;
use crate::vulkano_prelude::*;
use crate::render_device;
use crate::egui_impl;
use crate::gpu_err::*;

use anyhow::Result as AnyResult;

pub struct WindowSurface {
    event_loop: winit::event_loop::EventLoop<()>,
    win: Arc<winit::window::Window>,
}
impl WindowSurface {
    pub fn new() -> AnyResult<Self> {
        let event_loop = winit::event_loop::EventLoopBuilder::default().build();
        let win = winit::window::WindowBuilder::default()
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
        preview_renderer : Box<dyn crate::PreviewRenderProxy>,
    ) -> GpuResult<WindowRenderer> {
        let egui_ctx = egui_impl::EguiCtx::new(&render_surface)?;
        Ok(WindowRenderer {
            win: self.win,
            render_surface: Some(render_surface),
            swapchain_generation: 0,
            render_context,
            event_loop: Some(self.event_loop),
            last_frame: None,
            egui_ctx,
            preview_renderer,
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

    stylus_events: crate::stylus_events::WinitStylusEventCollector,
    swapchain_generation: u32,

    last_frame: Option<Box<dyn GpuFuture>>,

    preview_renderer: Box<dyn crate::PreviewRenderProxy>,
}
impl WindowRenderer {
    pub fn window(&self) -> Arc<winit::window::Window> {
        self.win.clone()
    }
    pub fn stylus_events(&self) -> tokio::sync::broadcast::Receiver<crate::stylus_events::StylusEventFrame> {
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

        self.preview_renderer.surface_changed(self.render_surface.as_ref().unwrap());

        Ok(())
    }
    fn apply_platform_output(&mut self, out: egui::PlatformOutput) {
        //Todo: Copied text
        if let Some(url) = out.open_url {
            //Todo: x-platform lol
            let out = std::process::Command::new("xdg-open").arg(url.url).spawn();
            if let Err(e) = out {
                log::error!("Failed to open url: {e:?}");
            }
        }

        if let Some(cursor) = egui_impl::egui_to_winit_cursor(out.cursor_icon) {
            self.win.set_cursor_icon(cursor);
            self.win.set_cursor_visible(true);
        } else {
            self.win.set_cursor_visible(false);
        }
    }
    pub fn run(mut self) -> ! {
        //There WILL be an event loop if we got here
        let event_loop = self.event_loop.take().unwrap();
        self.window().request_redraw();

        event_loop.run(move |event, _, control_flow| {
            use winit::event::{Event, WindowEvent};

            //Weird ownership problems here.
            let Some(event) = event.to_static() else {return};
            self.egui_ctx.push_winit_event(&event);

            match event {
                Event::WindowEvent { event, .. } => match event {
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
                        if !self.egui_ctx.wants_pointer_input() {
                            self.stylus_events.push_position(position.into());
                        }
                    }
                    WindowEvent::MouseInput { state, .. } => {
                        let pressed = winit::event::ElementState::Pressed == state;

                        if pressed {
                            // Only take if egui doesn't want it!
                            if !self.egui_ctx.wants_pointer_input() {
                                self.stylus_events.set_mouse_pressed(true)
                            }
                        } else {
                            self.stylus_events.set_mouse_pressed(false)
                        }

                    }
                    _ => (),
                },
                Event::DeviceEvent { event, .. } => {

                    match event {
                        //Pressure out of 65535
                        winit::event::DeviceEvent::Motion { axis: 2, value } => {
                            self.stylus_events.set_pressure(value as f32 / 65535.0)
                        }
                        _ => ()
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
                    if let Some(output) = self.do_ui() {
                        self.apply_platform_output(output);
                    };

                    if self.egui_ctx.needs_redraw() {
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
                    *control_flow = winit::event_loop::ControlFlow::Wait;
                }
                _ => (),
            }
        });
    }
    fn do_ui(&mut self) -> Option<egui::PlatformOutput> {
        static mut color : [f32; 4] = [1.0; 4];
        self.egui_ctx.update(|ctx| {
            egui::Window::new("🐑 Baa").show(&ctx, |ui| {
                ui.label("Testing testing 123!!");
            });

            egui::Window::new("Beep boop").show(&ctx, |ui| {
                ui.label("Testing testing 345!!");
                //Safety - we do not call `do_ui` in a multi-threaded context.
                // Dirty, but this is for testing ;)
                ui.color_edit_button_rgba_premultiplied(unsafe{ &mut color }).clicked();
            });
        })
    }
    fn paint(&mut self) -> AnyResult<()> {
        let (idx, suboptimal, image_future) =
            match vk::acquire_next_image(self.render_surface().swapchain().clone(), None) {
                Err(vulkano::swapchain::AcquireError::OutOfDate) => {
                    log::info!("Swapchain unusable. Recreating");
                    //We cannot draw on this surface as-is. Recreate and request another try next frame.
                    self.recreate_surface()?;
                    self.window().request_redraw();
                    return Ok(())
                }
                Err(e) => {
                    //Todo. Many of these errors are recoverable!
                    anyhow::bail!("Surface image acquire failed! {e:?}");
                }
                Ok(r) => r,
            };


        // Lmao
        // Free up resources from the last time this frame index was rendered
        // Todo: call much much sooner.
        self.preview_renderer.render_complete(idx);

        let preview_commands = self.preview_renderer.render(idx)?;
        let commands = self.egui_ctx.build_commands(idx);

        let render_complete = match commands {
            Some((Some(transfer), draw)) => {
                let transfer_future = match self.last_frame.take() {
                    Some(last_frame) => {
                        last_frame
                            .then_execute(
                                self.render_context.queues().transfer().queue().clone(),
                                transfer
                            )?
                            .boxed()
                            .then_signal_fence_and_flush()?
                    }
                    None => {
                        self.render_context.now()
                            .then_execute(
                                self.render_context.queues().transfer().queue().clone(),
                                transfer
                            )?
                            .boxed()
                            .then_signal_fence_and_flush()?
                    }
                };

                // Todo: no matter what I do, i cannot seem to get semaphores
                // to work. Ideally, the only thing that needs to wait is the
                // egui render commands, however it simply refuses to actually
                // wait for the semaphore. For now, I just stall the thread.
                transfer_future.wait(None)?;

                image_future
                    .then_execute(
                        self.render_context.queues().graphics().queue().clone(),
                        preview_commands
                    )?
                    .then_execute(
                        self.render_context.queues().graphics().queue().clone(),
                        draw
                    )?
                    .boxed()
            }
            Some((None, draw)) => {
                image_future
                    .then_execute(
                        self.render_context.queues().graphics().queue().clone(),
                        preview_commands
                    )?
                    .then_execute_same_queue(draw)?
                    .boxed()
            }
            None => {
                image_future
                    .then_execute(
                        self.render_context.queues().graphics().queue().clone(),
                        preview_commands
                    )?
                    .boxed()
            }
        };

        let next_frame_future = render_complete
            .then_swapchain_present(
                self.render_context.queues().present().unwrap().queue().clone(),
                vk::SwapchainPresentInfo::swapchain_image_index(self.render_surface.as_ref().unwrap().swapchain().clone(), idx)
            )
            .then_signal_semaphore_and_flush()?;

        self.last_frame = Some(next_frame_future.boxed());

        if suboptimal {
            self.recreate_surface()?
        }

        Ok(())
    }
}