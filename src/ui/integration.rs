use std::sync::Arc;
use winit::window::Window;

pub struct EguiIntegration {
    pub ctx: egui::Context,
    state: egui_winit::State,
    pub renderer: egui_wgpu::Renderer,
}

impl EguiIntegration {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        window: Arc<Window>,
    ) -> Self {
        let ctx = egui::Context::default();
        let viewport_id = ctx.viewport_id();
        let state = egui_winit::State::new(ctx.clone(), viewport_id, &window, None, None, None);
        let renderer = egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::default());

        Self { ctx, state, renderer }
    }

    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        let response = self.state.on_window_event(window, event);
        response.consumed
    }

    pub fn wants_keyboard_input(&self) -> bool {
        self.ctx.wants_keyboard_input()
    }

    pub fn wants_pointer_input(&self) -> bool {
        self.ctx.wants_pointer_input()
    }

    pub fn begin_frame(&mut self, window: &Window) {
        let raw_input = self.state.take_egui_input(window);
        self.ctx.begin_pass(raw_input);
    }

    pub fn end_frame(&mut self, window: &Window) -> egui::FullOutput {
        let full_output = self.ctx.end_pass();
        self.state.handle_platform_output(window, full_output.platform_output.clone());
        full_output
    }

    /// Tessellate, update textures and buffers. Returns paint jobs for use in render pass.
    /// After calling this, create a render pass and call `self.renderer.render(&mut pass, &jobs, &screen)`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        screen: &egui_wgpu::ScreenDescriptor,
        full_output: &egui::FullOutput,
    ) -> Vec<egui::ClippedPrimitive> {
        let paint_jobs = self.ctx.tessellate(full_output.shapes.clone(), full_output.pixels_per_point);

        for (id, image_delta) in &full_output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, image_delta);
        }

        self.renderer.update_buffers(device, queue, encoder, &paint_jobs, screen);

        paint_jobs
    }

    /// Free textures that egui no longer needs.
    pub fn cleanup(&mut self, full_output: &egui::FullOutput) {
        for id in &full_output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}
