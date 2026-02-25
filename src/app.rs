use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

use crate::game::config::GameConfig;
use crate::game::input::{GameAction, InputState};
use crate::game::inventory::Inventory;
use crate::game::recipes::RecipeIndex;
use crate::game::world::{Direction, EntityId, StructureKind, WorldState};
use crate::hyperbolic::poincare::{canonical_polygon, polygon_disk_radius, Complex, Mobius, TilingConfig};
use crate::hyperbolic::tiling::{format_address, TileAddr, TilingState};
use crate::render::camera::Camera;
use crate::render::mesh::build_polygon_mesh;
use crate::render::pipeline::{RenderState, Uniforms};
use crate::sim::belt::BeltNetwork;
use crate::sim::tick::GameLoop;
use crate::ui::icons::IconAtlas;
use crate::ui::integration::EguiIntegration;
use crate::ui::placement::PlacementMode;
use crate::ui::style::apply_octofact_style;

struct ClickResult {
    tile_idx: usize,
    grid_xy: (i32, i32),
    local_disk: Complex,
}

fn project_to_screen(world_pos: glam::Vec3, view_proj: &glam::Mat4, width: f32, height: f32) -> Option<(f32, f32)> {
    let clip = *view_proj * glam::Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = glam::Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
    if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 {
        return None;
    }
    let screen_x = (ndc.x + 1.0) * 0.5 * width;
    let screen_y = (1.0 - ndc.y) * 0.5 * height;
    Some((screen_x, screen_y))
}

/// Like project_to_screen but allows points outside the viewport (for partially visible geometry).
/// Like project_to_screen but allows points slightly outside the viewport
/// (for partially visible geometry). Coordinates are clamped to 2x viewport
/// to prevent degenerate polygons when clip.w is near zero.
fn project_to_screen_unclamped(world_pos: glam::Vec3, view_proj: &glam::Mat4, width: f32, height: f32) -> Option<(f32, f32)> {
    let clip = *view_proj * glam::Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w <= 0.01 {
        return None;
    }
    let ndc_x = (clip.x / clip.w).clamp(-3.0, 3.0);
    let ndc_y = (clip.y / clip.w).clamp(-3.0, 3.0);
    let screen_x = (ndc_x + 1.0) * 0.5 * width;
    let screen_y = (1.0 - ndc_y) * 0.5 * height;
    Some((screen_x, screen_y))
}

fn render_egui_pass(
    encoder: &mut wgpu::CommandEncoder,
    renderer: &egui_wgpu::Renderer,
    view: &wgpu::TextureView,
    paint_jobs: &[egui::ClippedPrimitive],
    screen: &egui_wgpu::ScreenDescriptor,
) {
    let mut pass = encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        })
        .forget_lifetime();
    renderer.render(&mut pass, paint_jobs, screen);
}

pub struct GpuState {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub window: Arc<Window>,
}

impl GpuState {
    pub fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("request adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("octofact device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
        ))
        .expect("request device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Self {
            surface,
            device,
            queue,
            config,
            window,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}

struct RunningState {
    gpu: GpuState,
    render: RenderState,
    tiling: TilingState,
    extra_elevation: std::collections::HashMap<usize, f32>,
    egui: EguiIntegration,
    icon_atlas: IconAtlas,
}

/// State for dragging belts along a gridline.
struct BeltDrag {
    tile_idx: usize,
    address: TileAddr,
    /// Fixed axis: true = horizontal (fixed gy), false = vertical (fixed gx)
    horizontal: bool,
    /// The fixed coordinate on the constrained axis
    fixed_coord: i32,
    /// The last grid coordinate placed along the free axis
    last_free: i32,
}

/// UI-only state extracted from App: flash notifications, drag state, cursor,
/// panel open flags, key rebinding, and placement mode.
pub struct UiState {
    pub placement_mode: Option<PlacementMode>,
    pub placement_open: bool,
    pub cursor_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    pub settings_open: bool,
    pub inventory_open: bool,
    pub rebinding: Option<GameAction>,
    pub flash_screen_pos: Option<(f32, f32)>,
    pub flash_label: String,
    pub flash_timer: f32,
    /// Active drag-to-place state: tile address, axis constraint, last placed grid coord
    belt_drag: Option<BeltDrag>,
    /// Currently inspected machine entity (opens the machine panel).
    pub machine_panel_entity: Option<EntityId>,
}

impl UiState {
    fn new() -> Self {
        Self {
            placement_mode: None,
            placement_open: false,
            cursor_pos: None,
            settings_open: false,
            inventory_open: false,
            rebinding: None,
            flash_screen_pos: None,
            flash_label: String::new(),
            flash_timer: 0.0,
            belt_drag: None,
            machine_panel_entity: None,
        }
    }

    fn is_panel_open(&self) -> bool {
        self.settings_open || self.inventory_open || self.machine_panel_entity.is_some()
    }
}

pub struct App {
    cfg: TilingConfig,
    running: Option<RunningState>,
    camera: Camera,
    game_loop: GameLoop,
    input_state: InputState,
    config: GameConfig,
    inventory: Inventory,
    recipes: RecipeIndex,
    world: WorldState,
    belt_network: BeltNetwork,
    machine_pool: crate::sim::machine::MachinePool,
    power_network: crate::sim::power::PowerNetwork,
    ui: UiState,
    grid_enabled: bool,
    klein_half_side: f64,
}

impl App {
    pub fn new(cfg: TilingConfig) -> Self {
        let config = GameConfig::load();
        let input_state = InputState::new(config.key_bindings.clone());
        Self {
            cfg,
            running: None,
            camera: Camera::new(),
            game_loop: GameLoop::new(),
            input_state,
            config,
            inventory: Inventory::starting_inventory(),
            recipes: RecipeIndex::new(),
            world: WorldState::new(),
            belt_network: BeltNetwork::new(),
            machine_pool: crate::sim::machine::MachinePool::new(),
            power_network: crate::sim::power::PowerNetwork::new(),
            ui: UiState::new(),
            grid_enabled: false,
            klein_half_side: {
                // For {4,n} squares: Klein half-side = r_klein / sqrt(2)
                // where r_klein = 2*r_poincare / (1 + r_poincare^2)
                let r_p = polygon_disk_radius(&cfg);
                let r_k = 2.0 * r_p / (1.0 + r_p * r_p);
                r_k / std::f64::consts::SQRT_2
            },
        }
    }

    fn ui_is_open(&self) -> bool {
        self.ui.is_panel_open()
    }

    fn find_clicked_tile(&self, sx: f64, sy: f64) -> Option<ClickResult> {
        let running = self.running.as_ref()?;
        let inv_view = self.camera.local.inverse();
        let khs = self.klein_half_side;
        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;
        let click_disk = self.camera.unproject_to_disk(sx, sy, width, height)?;

        // Find the tile whose Klein cell actually contains the click.
        // For each visible tile, compute local Klein coords and pick the
        // tile where the click is closest to cell center (smallest Chebyshev distance).
        let mut best: Option<(usize, f64, f64)> = None; // (tile_idx, norm_x, norm_y)
        let mut best_max_norm = f64::MAX;

        for (i, tile) in running.tiling.tiles.iter().enumerate() {
            let combined = inv_view.compose(&tile.transform);
            let disk_center = combined.apply(Complex::ZERO);
            if disk_center.abs() > 0.98 {
                continue;
            }

            let inv_combined = combined.inverse();
            let local_p = inv_combined.apply(click_disk);
            let r2 = local_p.re * local_p.re + local_p.im * local_p.im;
            let local_kx = 2.0 * local_p.re / (1.0 + r2);
            let local_ky = 2.0 * local_p.im / (1.0 + r2);
            let norm_x = local_kx / (2.0 * khs);
            let norm_y = local_ky / (2.0 * khs);
            let max_norm = norm_x.abs().max(norm_y.abs());

            if max_norm < best_max_norm {
                best_max_norm = max_norm;
                best = Some((i, norm_x, norm_y));
            }
        }

        let (tile_idx, norm_x, norm_y) = best?;

        // Snap to nearest grid intersection (64 divisions per cell edge)
        let divisions = 64.0_f64;
        let gx = (norm_x * divisions).round() as i32;
        let gy = (norm_y * divisions).round() as i32;
        let gx = gx.clamp(-32, 32);
        let gy = gy.clamp(-32, 32);

        // Snap back: grid coords -> Klein -> Poincare
        let snap_kx = (gx as f64 / divisions) * 2.0 * khs;
        let snap_ky = (gy as f64 / divisions) * 2.0 * khs;
        let kr2 = snap_kx * snap_kx + snap_ky * snap_ky;
        let denom = 1.0 + (1.0 - kr2).max(0.0).sqrt();
        let local_disk = Complex::new(snap_kx / denom, snap_ky / denom);

        Some(ClickResult { tile_idx, grid_xy: (gx, gy), local_disk })
    }

    fn handle_debug_click(&mut self, sx: f64, sy: f64) {
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return,
        };

        if self.config.debug.log_clicks {
            let tile = &self.running.as_ref().unwrap().tiling.tiles[result.tile_idx];
            log::info!(
                "{};{},{}",
                format_address(&tile.address),
                result.grid_xy.0,
                result.grid_xy.1,
            );
        }

        // Flash at snapped grid intersection projected to screen
        let running = self.running.as_ref().unwrap();
        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;
        let scale = running.gpu.window.scale_factor() as f32;
        let aspect = width / height;
        let view_proj = self.camera.build_view_proj(aspect);

        let inv_view = self.camera.local.inverse();
        let tile_xform = running.tiling.tiles[result.tile_idx].transform;
        let combined = inv_view.compose(&tile_xform);
        // Transform snapped local Poincare coords back to view-space disk
        let world_disk = combined.apply(result.local_disk);
        let bowl = crate::hyperbolic::embedding::disk_to_bowl(world_disk);
        let elevation = running.extra_elevation.get(&result.tile_idx).copied().unwrap_or(0.0);
        let world_pos = glam::Vec3::new(bowl[0], bowl[1] + elevation, bowl[2]);

        if let Some((px, py)) = project_to_screen(world_pos, &view_proj, width, height) {
            let tile = &running.tiling.tiles[result.tile_idx];
            self.ui.flash_label = format!(
                "{};{},{}",
                format_address(&tile.address),
                result.grid_xy.0,
                result.grid_xy.1,
            );
            self.ui.flash_screen_pos = Some((px / scale, py / scale));
            self.ui.flash_timer = 0.4;
        }
    }

    /// Place a single structure at the given tile address and grid position.
    /// Returns true if placement succeeded.
    fn try_place_at(&mut self, tile_idx: usize, address: &[u8], grid_xy: (i32, i32), mode: &PlacementMode) -> bool {
        if !self.config.debug.free_placement && self.inventory.count(mode.item) == 0 {
            return false;
        }
        let entity = match self.world.place(address, grid_xy, mode.item, mode.direction) {
            Some(e) => e,
            None => return false, // occupied or not placeable
        };
        if !self.config.debug.free_placement {
            self.inventory.remove(mode.item, 1);
        }

        // Register belt with simulation network
        if mode.item == crate::game::items::ItemId::Belt {
            self.belt_network.on_belt_placed(
                entity, address, grid_xy.0, grid_xy.1, mode.direction, &self.world,
            );
            // Establish cross-tile transport line links
            self.check_cross_tile_belt_link(entity, tile_idx, address, grid_xy, mode.direction);
        }

        // Register machine with simulation pool and auto-connect ports
        if let Some(crate::game::world::StructureKind::Machine(mt)) =
            crate::game::world::StructureKind::from_item(mode.item)
        {
            self.machine_pool.add(entity, mt);
            self.auto_connect_machine_ports(entity, address, grid_xy, mode.direction, mt);
            // Register machine as power consumer
            let exempt = mt == crate::game::items::MachineType::Source;
            self.power_network.add(
                entity,
                crate::sim::power::PowerNodeKind::Consumer,
                crate::sim::power::MACHINE_CONSUMPTION,
                address,
                grid_xy.0 as i16,
                grid_xy.1 as i16,
                exempt,
            );
        }

        // Register power structures as producers
        match crate::game::world::StructureKind::from_item(mode.item) {
            Some(crate::game::world::StructureKind::PowerNode) => {
                self.power_network.add(
                    entity,
                    crate::sim::power::PowerNodeKind::Producer,
                    crate::sim::power::QUADRUPOLE_RATE,
                    address,
                    grid_xy.0 as i16,
                    grid_xy.1 as i16,
                    false,
                );
            }
            Some(crate::game::world::StructureKind::PowerSource) => {
                self.power_network.add(
                    entity,
                    crate::sim::power::PowerNodeKind::Producer,
                    crate::sim::power::DYNAMO_RATE,
                    address,
                    grid_xy.0 as i16,
                    grid_xy.1 as i16,
                    false,
                );
            }
            _ => {}
        }

        // Auto-connect belt to adjacent machines
        if mode.item == crate::game::items::ItemId::Belt {
            self.auto_connect_belt_to_machines(entity, address, grid_xy, mode.direction);
        }

        // Flash feedback
        let running = self.running.as_ref().unwrap();
        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;
        let scale = running.gpu.window.scale_factor() as f32;
        let aspect = width / height;
        let view_proj = self.camera.build_view_proj(aspect);
        let khs = self.klein_half_side;
        let divisions = 64.0_f64;

        let inv_view = self.camera.local.inverse();
        let tile_xform = running.tiling.tiles[tile_idx].transform;
        let combined = inv_view.compose(&tile_xform);

        let snap_kx = (grid_xy.0 as f64 / divisions) * 2.0 * khs;
        let snap_ky = (grid_xy.1 as f64 / divisions) * 2.0 * khs;
        let kr2 = snap_kx * snap_kx + snap_ky * snap_ky;
        let denom = 1.0 + (1.0 - kr2).max(0.0).sqrt();
        let local_disk = Complex::new(snap_kx / denom, snap_ky / denom);

        let world_disk = combined.apply(local_disk);
        let bowl = crate::hyperbolic::embedding::disk_to_bowl(world_disk);
        let elevation = running.extra_elevation.get(&tile_idx).copied().unwrap_or(0.0);
        let world_pos = glam::Vec3::new(bowl[0], bowl[1] + elevation, bowl[2]);

        if let Some((px, py)) = project_to_screen(world_pos, &view_proj, width, height) {
            self.ui.flash_label = format!(
                "{} {}",
                mode.item.display_name(),
                mode.direction.arrow_char(),
            );
            self.ui.flash_screen_pos = Some((px / scale, py / scale));
            self.ui.flash_timer = 0.4;
        }
        true
    }

    /// After a belt is placed, check if its ahead/behind positions cross a tile
    /// boundary. If so, find the neighboring tile's belt and link the two
    /// transport lines via `BeltEnd::Belt`.
    ///
    /// Cross-tile check triggers when the neighbor position is:
    /// - Off-tile (outside -32..=32), OR
    /// - At the shared edge (±32) with no same-direction belt on this tile.
    fn check_cross_tile_belt_link(
        &mut self,
        entity: EntityId,
        tile_idx: usize,
        tile_addr: &[u8],
        grid_xy: (i32, i32),
        direction: Direction,
    ) {
        use crate::sim::belt::is_within_tile;

        let (dx, dy) = direction.grid_offset_i32();
        let ahead = (grid_xy.0 + dx, grid_xy.1 + dy);
        let behind = (grid_xy.0 - dx, grid_xy.1 - dy);

        // Determine which neighbor positions need cross-tile checks.
        // Off-tile always needs it; ±32 needs it only if no belt exists there
        // on this tile (the edge is shared between adjacent tiles).
        let check_ahead = !is_within_tile(ahead.0, ahead.1)
            || ((ahead.0.abs() == 32 || ahead.1.abs() == 32)
                && find_same_dir_belt_at(&self.world, tile_addr, ahead, direction).is_none());
        let check_behind = !is_within_tile(behind.0, behind.1)
            || ((behind.0.abs() == 32 || behind.1.abs() == 32)
                && find_same_dir_belt_at(&self.world, tile_addr, behind, direction).is_none());

        // Output connection: this belt's flow exits toward ahead
        if check_ahead {
            let edge = direction.tiling_edge_index();
            let mirror = cross_tile_mirror(ahead);
            let running = self.running.as_ref().unwrap();
            if let Some(neighbor_addr) = running.tiling.neighbor_tile_addr(tile_idx, edge) {
                if let Some(neighbor_entity) = find_same_dir_belt_at(
                    &self.world, &neighbor_addr, mirror, direction,
                ) {
                    self.belt_network.link_output_to_input(entity, neighbor_entity);
                }
            }
        }

        // Input connection: items would enter this belt from behind
        if check_behind {
            let edge = direction.opposite().tiling_edge_index();
            let mirror = cross_tile_mirror(behind);
            let running = self.running.as_ref().unwrap();
            if let Some(neighbor_addr) = running.tiling.neighbor_tile_addr(tile_idx, edge) {
                if let Some(neighbor_entity) = find_same_dir_belt_at(
                    &self.world, &neighbor_addr, mirror, direction,
                ) {
                    self.belt_network.link_output_to_input(neighbor_entity, entity);
                }
            }
        }
    }

    /// When a machine is placed, check all its ports for adjacent belts and connect them.
    fn auto_connect_machine_ports(
        &mut self,
        machine_entity: EntityId,
        tile_addr: &[u8],
        grid_xy: (i32, i32),
        facing: Direction,
        machine_type: crate::game::items::MachineType,
    ) {
        use crate::sim::inserter::{belt_compatible_with_port, rotated_ports, PortKind};

        for port in rotated_ports(machine_type, facing) {
            let (dx, dy) = port.side.grid_offset_i32();
            let adj = (grid_xy.0 + dx, grid_xy.1 + dy);

            if let Some(entities) = self.world.tile_entities(tile_addr) {
                if let Some(&belt_entity) = entities.get(&adj) {
                    if self.world.kind(belt_entity) == Some(StructureKind::Belt) {
                        if let Some(belt_dir) = self.world.direction(belt_entity) {
                            if belt_compatible_with_port(&port, belt_dir) {
                                match port.kind {
                                    PortKind::Input => {
                                        self.belt_network.connect_belt_to_machine_input(
                                            belt_entity,
                                            machine_entity,
                                            port.slot,
                                        );
                                    }
                                    PortKind::Output => {
                                        self.belt_network.connect_machine_output_to_belt(
                                            belt_entity,
                                            machine_entity,
                                            port.slot,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// When a belt is placed, check all 4 adjacent cells for machines and connect ports.
    fn auto_connect_belt_to_machines(
        &mut self,
        belt_entity: EntityId,
        tile_addr: &[u8],
        grid_xy: (i32, i32),
        belt_dir: Direction,
    ) {
        use crate::sim::inserter::{belt_compatible_with_port, port_on_side, PortKind};

        for &check_dir in &[Direction::North, Direction::East, Direction::South, Direction::West] {
            let (dx, dy) = check_dir.grid_offset_i32();
            let adj = (grid_xy.0 + dx, grid_xy.1 + dy);

            if let Some(entities) = self.world.tile_entities(tile_addr) {
                if let Some(&adj_entity) = entities.get(&adj) {
                    if let Some(StructureKind::Machine(mt)) = self.world.kind(adj_entity) {
                        if let Some(facing) = self.world.direction(adj_entity) {
                            // Machine's port on the side facing the belt
                            if let Some(port) =
                                port_on_side(mt, facing, check_dir.opposite())
                            {
                                if belt_compatible_with_port(&port, belt_dir) {
                                    match port.kind {
                                        PortKind::Input => {
                                            self.belt_network.connect_belt_to_machine_input(
                                                belt_entity,
                                                adj_entity,
                                                port.slot,
                                            );
                                        }
                                        PortKind::Output => {
                                            self.belt_network.connect_machine_output_to_belt(
                                                belt_entity,
                                                adj_entity,
                                                port.slot,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn handle_placement_click(&mut self, sx: f64, sy: f64) {
        let mode = match self.ui.placement_mode.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };

        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return,
        };

        let running = self.running.as_ref().unwrap();
        let address = running.tiling.tiles[result.tile_idx].address.clone();

        if self.try_place_at(result.tile_idx, &address, result.grid_xy, &mode) {
            // Lock drag axis parallel to the belt's facing direction
            let horizontal = matches!(mode.direction, Direction::East | Direction::West);
            let (fixed_coord, last_free) = if horizontal {
                (result.grid_xy.1, result.grid_xy.0) // fixed gy, free gx
            } else {
                (result.grid_xy.0, result.grid_xy.1) // fixed gx, free gy
            };
            self.ui.belt_drag = Some(BeltDrag {
                tile_idx: result.tile_idx,
                address,
                horizontal,
                fixed_coord,
                last_free,
            });
        }
    }

    fn handle_placement_drag(&mut self, sx: f64, sy: f64) {
        let mode = match self.ui.placement_mode.as_ref() {
            Some(m) => m.clone(),
            None => { self.ui.belt_drag = None; return; },
        };

        let drag = match self.ui.belt_drag.as_ref() {
            Some(d) => d,
            None => return,
        };

        // Copy out drag state before any mutable operations
        let horizontal = drag.horizontal;
        let fixed_coord = drag.fixed_coord;
        let last_free = drag.last_free;
        let old_address = drag.address.clone();
        let old_tile_idx = drag.tile_idx;
        let khs = self.klein_half_side;

        // Compute cursor's virtual (unclamped) grid position on the drag tile.
        // If the cursor has moved beyond ±32, it's past the tile edge.
        let virtual_free = {
            let running = self.running.as_ref().unwrap();
            let width = running.gpu.config.width as f32;
            let height = running.gpu.config.height as f32;

            let click_disk = match self.camera.unproject_to_disk(sx, sy, width, height) {
                Some(d) => d,
                None => return,
            };

            let inv_view = self.camera.local.inverse();
            let tile_xform = running.tiling.tiles[old_tile_idx].transform;
            let combined = inv_view.compose(&tile_xform);
            let inv_combined = combined.inverse();
            let local_p = inv_combined.apply(click_disk);
            let r2 = local_p.re * local_p.re + local_p.im * local_p.im;
            let local_kx = 2.0 * local_p.re / (1.0 + r2);
            let local_ky = 2.0 * local_p.im / (1.0 + r2);
            let norm_x = local_kx / (2.0 * khs);
            let norm_y = local_ky / (2.0 * khs);
            let divisions = 64.0_f64;
            let vgx = (norm_x * divisions).round() as i32;
            let vgy = (norm_y * divisions).round() as i32;
            if horizontal { vgx } else { vgy }
        }; // running borrow dropped

        const MAX_DRAG_STEP: i32 = 4;

        if virtual_free.abs() <= 32 {
            // --- Same tile: cursor is within grid bounds ---
            let mut target_free = virtual_free;
            target_free = target_free.clamp(last_free - MAX_DRAG_STEP, last_free + MAX_DRAG_STEP);
            if target_free == last_free { return; }

            let step = if target_free > last_free { 1 } else { -1 };
            let mut current = last_free + step;
            loop {
                let grid_xy = if horizontal { (current, fixed_coord) } else { (fixed_coord, current) };
                self.try_place_at(old_tile_idx, &old_address, grid_xy, &mode);
                if current == target_free { break; }
                current += step;
            }

            if let Some(d) = self.ui.belt_drag.as_mut() {
                d.last_free = target_free;
            }
        } else {
            // --- Cross-tile: cursor is beyond ±32 on the free axis ---
            let old_edge = if virtual_free > 32 { 32 } else { -32 };

            // Fill toward edge on old tile (capped by MAX_DRAG_STEP)
            let old_target = old_edge.clamp(last_free - MAX_DRAG_STEP, last_free + MAX_DRAG_STEP);
            if old_target != last_free {
                let step = if old_target > last_free { 1 } else { -1 };
                let mut current = last_free + step;
                loop {
                    let grid_xy = if horizontal { (current, fixed_coord) } else { (fixed_coord, current) };
                    self.try_place_at(old_tile_idx, &old_address, grid_xy, &mode);
                    if current == old_target { break; }
                    current += step;
                }
            }

            if old_target == old_edge {
                // Reached the edge — find the neighboring tile via cursor
                let result = match self.find_clicked_tile(sx, sy) {
                    Some(r) => r,
                    None => {
                        if let Some(d) = self.ui.belt_drag.as_mut() { d.last_free = old_target; }
                        return;
                    }
                };

                if result.tile_idx == old_tile_idx {
                    // Cursor resolved to the same tile; just stay at edge
                    if let Some(d) = self.ui.belt_drag.as_mut() { d.last_free = old_target; }
                    return;
                }

                // Get new tile info (immutable borrow, then drop before mutable ops)
                let (new_tile_idx, new_address) = {
                    let running = self.running.as_ref().unwrap();
                    (result.tile_idx, running.tiling.tiles[result.tile_idx].address.clone())
                };

                // For {4,n} (even p), grids align straight across edges.
                // ±32 on adjacent tiles is the SAME physical edge, so skip it
                // on the new tile to avoid overlapping belts.
                // old gx=+32 → new tile starts at gx=-31 (not -32).
                let inward: i32 = if old_edge > 0 { 1 } else { -1 };
                let new_start = -old_edge + inward;
                let new_free = if horizontal { result.grid_xy.0 } else { result.grid_xy.1 };
                let clamp_lo = new_start.min(new_start + (MAX_DRAG_STEP - 1) * inward);
                let clamp_hi = new_start.max(new_start + (MAX_DRAG_STEP - 1) * inward);
                let new_target = new_free.clamp(clamp_lo, clamp_hi);

                // Fill from new_start toward cursor on new tile (inclusive)
                let mut current = new_start;
                loop {
                    let grid_xy = if horizontal { (current, fixed_coord) } else { (fixed_coord, current) };
                    self.try_place_at(new_tile_idx, &new_address, grid_xy, &mode);
                    if current == new_target { break; }
                    current += inward;
                }

                // Switch drag to the new tile
                self.ui.belt_drag = Some(BeltDrag {
                    tile_idx: new_tile_idx,
                    address: new_address,
                    horizontal,
                    fixed_coord,
                    last_free: new_target,
                });
            } else {
                // Haven't reached edge yet; update position
                if let Some(d) = self.ui.belt_drag.as_mut() {
                    d.last_free = old_target;
                }
            }
        }
    }

    /// Debug: spawn a NullSet item on the belt at the clicked grid position.
    fn debug_spawn_item(&mut self, sx: f64, sy: f64) {
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return,
        };
        let address = {
            let running = self.running.as_ref().unwrap();
            running.tiling.tiles[result.tile_idx].address.clone()
        };
        let entities = match self.world.tile_entities(&address) {
            Some(e) => e,
            None => return,
        };
        let &entity = match entities.get(&result.grid_xy) {
            Some(e) => e,
            None => return,
        };
        if self.world.kind(entity) != Some(StructureKind::Belt) {
            return;
        }
        self.belt_network.spawn_item_on_entity(entity, crate::game::items::ItemId::NullSet);
    }

    /// Try to open the machine panel if the clicked grid cell contains a machine.
    /// Returns true if a machine was found and the panel was opened.
    fn try_open_machine_panel(&mut self, sx: f64, sy: f64) -> bool {
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return false,
        };
        let address = {
            let running = self.running.as_ref().unwrap();
            running.tiling.tiles[result.tile_idx].address.clone()
        };
        let entities = match self.world.tile_entities(&address) {
            Some(e) => e,
            None => return false,
        };
        let &entity = match entities.get(&result.grid_xy) {
            Some(e) => e,
            None => return false,
        };
        if let Some(StructureKind::Machine(_)) = self.world.kind(entity) {
            self.ui.machine_panel_entity = Some(entity);
            true
        } else {
            false
        }
    }

    fn modify_terrain(&mut self, sx: f64, sy: f64, delta: f32) {
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return,
        };

        *self.running.as_mut().unwrap().extra_elevation.entry(result.tile_idx).or_insert(0.0) += delta;
    }

    fn render_frame(&mut self) -> Result<(), wgpu::SurfaceError> {
        // Use interpolated camera for smooth rendering between sim ticks
        let render_camera = self.game_loop.interpolated_camera()
            .unwrap_or_else(|| self.camera.snapshot());

        let aspect = {
            let gpu = &self.running.as_ref().unwrap().gpu;
            gpu.config.width as f32 / gpu.config.height as f32
        };
        let view_proj = render_camera.build_view_proj(aspect);

        let running = self.running.as_mut().unwrap();
        let window = running.gpu.window.clone();

        let output = running.gpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;

        let inv_view = render_camera.local.inverse();

        let running = self.running.as_mut().unwrap();

        // Collect visible tile indices with cached Mobius composition
        let visible: Vec<(usize, Mobius)> = running.tiling
            .tiles
            .iter()
            .enumerate()
            .filter_map(|(i, tile)| {
                let combined = inv_view.compose(&tile.transform);
                let center = combined.apply(Complex::ZERO);
                if center.abs() < 0.98 { Some((i, combined)) } else { None }
            })
            .take(crate::render::pipeline::MAX_TILES)
            .collect();
        let tile_count = visible.len();

        // Upload uniforms only for visible tiles
        for (slot, &(tile_idx, combined)) in visible.iter().enumerate() {
            let tile = &running.tiling.tiles[tile_idx];
            let elevation = running.extra_elevation.get(&tile_idx).copied().unwrap_or(0.0);
            let uniforms = Uniforms {
                view_proj: view_proj.to_cols_array_2d(),
                mobius_a: [combined.a.re as f32, combined.a.im as f32, 0.0, 0.0],
                mobius_b: [combined.b.re as f32, combined.b.im as f32, 0.0, 0.0],
                disk_params: [tile.depth as f32, elevation, slot as f32 * 1e-6, 13.0],
                grid_params: [
                    if self.grid_enabled { 1.0 } else { 0.0 },
                    64.0,  // 64 grid divisions per cell edge
                    0.03,  // line width in grid-cell units
                    self.klein_half_side as f32,
                ],
                ..Default::default()
            };
            running.render.write_tile_uniforms(&running.gpu.queue, slot, &uniforms);
        }

        // --- egui frame ---
        running.egui.begin_frame(&window);

        // Label overlay (replaces glyphon)
        if running.render.labels_enabled {
            let egui_ctx = running.egui.ctx.clone();
            let scale = window.scale_factor() as f32;
            let area = egui::Area::new(egui::Id::new("tile_labels"))
                .order(egui::Order::Background)
                .interactable(false);
            area.show(&egui_ctx, |ui| {
                for &(tile_idx, combined) in &visible {
                    let tile = &running.tiling.tiles[tile_idx];
                    let disk_center = combined.apply(Complex::ZERO);
                    if disk_center.abs() > 0.9 {
                        continue;
                    }
                    let elevation = running.extra_elevation.get(&tile_idx).copied().unwrap_or(0.0);
                    let hyp = crate::hyperbolic::embedding::disk_to_bowl(disk_center);
                    let world_pos = glam::Vec3::new(hyp[0], hyp[1] + elevation, hyp[2]);
                    if let Some((sx, sy)) = project_to_screen(world_pos, &view_proj, width, height) {
                        // Convert physical pixels to logical points for egui
                        let lx = sx / scale;
                        let ly = sy / scale;
                        let text = format_address(&tile.address);
                        ui.put(
                            egui::Rect::from_min_size(
                                egui::pos2(lx - 20.0, ly - 8.0),
                                egui::vec2(80.0, 20.0),
                            ),
                            egui::Label::new(
                                egui::RichText::new(text).color(egui::Color32::WHITE).size(14.0)
                            ),
                        );
                    }
                }
            });
        }

        // Settings menu
        crate::ui::settings::settings_menu(
            &running.egui.ctx.clone(),
            &mut self.ui.settings_open,
            &mut self.config,
            &mut self.input_state,
            &mut self.ui.rebinding,
        );

        // Inventory window
        crate::ui::inventory::inventory_window(
            &running.egui.ctx.clone(),
            &mut self.ui.inventory_open,
            &self.inventory,
            &running.icon_atlas,
            &self.recipes,
        );

        // Placement panel
        crate::ui::placement::placement_panel(
            &running.egui.ctx.clone(),
            &mut self.ui.placement_open,
            &self.inventory,
            &running.icon_atlas,
            &mut self.ui.placement_mode,
            self.config.debug.free_placement,
        );

        // Machine inspection panel
        if let Some(entity) = self.ui.machine_panel_entity {
            let egui_ctx = running.egui.ctx.clone();
            if let Some(action) = crate::ui::machine::machine_panel(
                &egui_ctx,
                entity,
                &self.machine_pool,
                &self.recipes,
                &running.icon_atlas,
            ) {
                match action {
                    crate::ui::machine::MachineAction::SetRecipe(e, recipe_idx) => {
                        self.machine_pool.set_recipe(e, recipe_idx);
                    }
                    crate::ui::machine::MachineAction::Close => {
                        self.ui.machine_panel_entity = None;
                    }
                }
            }
        }

        // Debug click flash
        if self.ui.flash_timer > 0.0 {
            if let Some((fx, fy)) = self.ui.flash_screen_pos {
                let alpha = (self.ui.flash_timer / 0.4).clamp(0.0, 1.0);
                let a = (alpha * 255.0) as u8;
                let egui_ctx = running.egui.ctx.clone();
                egui::Area::new(egui::Id::new("debug_flash"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(fx, fy))
                    .interactable(false)
                    .show(&egui_ctx, |ui| {
                        let painter = ui.painter();
                        painter.circle_filled(
                            egui::pos2(fx, fy),
                            6.0,
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, a),
                        );
                        if !self.ui.flash_label.is_empty() {
                            painter.text(
                                egui::pos2(fx, fy - 12.0),
                                egui::Align2::CENTER_BOTTOM,
                                &self.ui.flash_label,
                                egui::FontId::monospace(13.0),
                                egui::Color32::from_rgba_unmultiplied(255, 255, 255, a),
                            );
                        }
                    });
            }
        }

        // FPS / UPS debug overlay
        {
            let egui_ctx = running.egui.ctx.clone();
            egui::Area::new(egui::Id::new("fps_ups_overlay"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(8.0, 8.0))
                .interactable(false)
                .show(&egui_ctx, |ui| {
                    let fps = self.game_loop.fps;
                    let ups = self.game_loop.ups;
                    ui.label(
                        egui::RichText::new(format!("FPS {fps:.0}  UPS {ups:.0}"))
                            .color(egui::Color32::from_rgb(180, 220, 180))
                            .size(13.0)
                            .font(egui::FontId::monospace(13.0)),
                    );
                });
        }

        // Belt overlay — projected flat on tile surface
        {
            let egui_ctx = running.egui.ctx.clone();
            let scale = window.scale_factor() as f32;
            let khs = self.klein_half_side;
            let divisions = 64.0_f64;
            egui::Area::new(egui::Id::new("belt_overlay"))
                .order(egui::Order::Background)
                .interactable(false)
                .show(&egui_ctx, |ui| {
                    let painter = ui.painter();
                    for &(tile_idx, combined) in &visible {
                        let tile = &running.tiling.tiles[tile_idx];
                        let disk_center = combined.apply(Complex::ZERO);
                        if disk_center.abs() > 0.9 {
                            continue;
                        }
                        let entities = match self.world.tile_entities(&tile.address) {
                            Some(e) => e,
                            None => continue,
                        };
                        let elevation = running.extra_elevation.get(&tile_idx).copied().unwrap_or(0.0);
                        let dist = disk_center.abs();
                        let alpha = ((1.0 - dist / 0.9) * 1.5).clamp(0.0, 1.0);
                        if alpha < 0.01 {
                            continue;
                        }

                        for (&(gx, gy), &entity) in entities {
                            let kind = match self.world.kind(entity) {
                                Some(k) => k,
                                None => continue,
                            };

                            // Per-kind colors
                            let (face_rgb, hilite_rgb, shadow_rgb, edge_rgb) = match kind {
                                StructureKind::Belt => (
                                    (140, 140, 145),
                                    (180, 180, 185),
                                    (60, 60, 60),
                                    (20, 20, 20),
                                ),
                                StructureKind::PowerNode => (
                                    (200, 180, 50),
                                    (230, 210, 100),
                                    (120, 100, 20),
                                    (60, 50, 10),
                                ),
                                StructureKind::PowerSource => (
                                    (220, 200, 60),
                                    (250, 230, 120),
                                    (140, 120, 30),
                                    (70, 60, 15),
                                ),
                                StructureKind::Machine(_) => (
                                    (100, 130, 180),
                                    (140, 170, 210),
                                    (40, 60, 90),
                                    (15, 25, 45),
                                ),
                            };

                            // Project fractional grid coords through the full 3D pipeline
                            // Uses unclamped projection so partially offscreen structures still render
                            let grid_to_screen = |fx: f64, fy: f64| -> Option<egui::Pos2> {
                                let skx = (fx / divisions) * 2.0 * khs;
                                let sky = (fy / divisions) * 2.0 * khs;
                                let kr2 = skx * skx + sky * sky;
                                let d = 1.0 + (1.0 - kr2).max(0.0).sqrt();
                                let ld = Complex::new(skx / d, sky / d);
                                let wd = combined.apply(ld);
                                let b = crate::hyperbolic::embedding::disk_to_bowl(wd);
                                let wp = glam::Vec3::new(b[0], b[1] + elevation, b[2]);
                                project_to_screen_unclamped(wp, &view_proj, width, height)
                                    .map(|(px, py)| egui::pos2(px / scale, py / scale))
                            };

                            let cx = gx as f64;
                            let cy = gy as f64;
                            let h = 0.48; // slightly inset from full grid cell

                            // Four corners, projected flat on the surface
                            let c0 = grid_to_screen(cx - h, cy - h);
                            let c1 = grid_to_screen(cx + h, cy - h);
                            let c2 = grid_to_screen(cx + h, cy + h);
                            let c3 = grid_to_screen(cx - h, cy + h);

                            let corners = match (c0, c1, c2, c3) {
                                (Some(a), Some(b), Some(c), Some(d)) => [a, b, c, d],
                                _ => continue,
                            };

                            let a = (alpha * 255.0) as u8;

                            // Dark outer edge (slightly larger quad)
                            let centroid = egui::pos2(
                                corners.iter().map(|p| p.x).sum::<f32>() / 4.0,
                                corners.iter().map(|p| p.y).sum::<f32>() / 4.0,
                            );
                            let edge_corners: Vec<egui::Pos2> = corners.iter().map(|p| {
                                let dx = p.x - centroid.x;
                                let dy = p.y - centroid.y;
                                egui::pos2(centroid.x + dx * 1.12, centroid.y + dy * 1.12)
                            }).collect();
                            painter.add(egui::Shape::convex_polygon(
                                edge_corners,
                                egui::Color32::from_rgba_unmultiplied(edge_rgb.0, edge_rgb.1, edge_rgb.2, a),
                                egui::Stroke::NONE,
                            ));

                            // Light bevel highlight (top-left bias)
                            let hilite_corners: Vec<egui::Pos2> = corners.iter().enumerate().map(|(i, p)| {
                                let dx = p.x - centroid.x;
                                let dy = p.y - centroid.y;
                                let shrink = if i == 0 || i == 1 { 1.04 } else { 0.96 };
                                egui::pos2(centroid.x + dx * shrink, centroid.y + dy * shrink)
                            }).collect();
                            painter.add(egui::Shape::convex_polygon(
                                hilite_corners,
                                egui::Color32::from_rgba_unmultiplied(hilite_rgb.0, hilite_rgb.1, hilite_rgb.2, a),
                                egui::Stroke::NONE,
                            ));

                            // Main face
                            painter.add(egui::Shape::convex_polygon(
                                corners.to_vec(),
                                egui::Color32::from_rgba_unmultiplied(face_rgb.0, face_rgb.1, face_rgb.2, a),
                                egui::Stroke::NONE,
                            ));

                            // Dark bevel shadow (bottom-right bias)
                            let shadow_corners: Vec<egui::Pos2> = corners.iter().enumerate().map(|(i, p)| {
                                let dx = p.x - centroid.x;
                                let dy = p.y - centroid.y;
                                let shrink = if i == 2 || i == 3 { 1.0 } else { 0.88 };
                                egui::pos2(centroid.x + dx * shrink, centroid.y + dy * shrink)
                            }).collect();
                            painter.add(egui::Shape::convex_polygon(
                                shadow_corners,
                                egui::Color32::from_rgba_unmultiplied(shadow_rgb.0, shadow_rgb.1, shadow_rgb.2, a),
                                egui::Stroke::NONE,
                            ));

                            // Kind-specific symbol
                            match kind {
                                StructureKind::Belt => {
                                    // Direction arrow for belts
                                    let dir = match self.world.direction(entity) {
                                        Some(d) => d,
                                        None => continue,
                                    };
                                    let (dx, dy) = dir.grid_offset();
                                    let tip = grid_to_screen(cx + dx * 0.35, cy + dy * 0.35);
                                    let bl = grid_to_screen(cx - dx * 0.25 - dy * 0.2, cy - dy * 0.25 + dx * 0.2);
                                    let br = grid_to_screen(cx - dx * 0.25 + dy * 0.2, cy - dy * 0.25 - dx * 0.2);
                                    if let (Some(t), Some(l), Some(r)) = (tip, bl, br) {
                                        painter.add(egui::Shape::convex_polygon(
                                            vec![t, l, r],
                                            egui::Color32::from_rgba_unmultiplied(30, 30, 30, a),
                                            egui::Stroke::NONE,
                                        ));
                                    }

                                    // Draw items riding on this belt
                                    if let Some((belt_items, offset)) = self.belt_network.entity_items(entity) {
                                        for bi in belt_items {
                                            let pos_frac = (bi.pos - offset) as f64 / crate::sim::belt::FP_SCALE as f64;
                                            let ix = cx + dx * (0.5 - pos_frac);
                                            let iy = cy + dy * (0.5 - pos_frac);
                                            if let Some(center) = grid_to_screen(ix, iy) {
                                                let colors = bi.item.icon_params().primary_color;
                                                let r = (colors[0] * 255.0) as u8;
                                                let g = (colors[1] * 255.0) as u8;
                                                let b = (colors[2] * 255.0) as u8;
                                                painter.circle_filled(
                                                    center,
                                                    3.0,
                                                    egui::Color32::from_rgba_unmultiplied(r, g, b, a),
                                                );
                                            }
                                        }
                                    }
                                }
                                StructureKind::PowerNode | StructureKind::PowerSource => {
                                    // "+" cross symbol for power structures
                                    let arm = 0.28;
                                    let w = 0.08;
                                    let cross_color = egui::Color32::from_rgba_unmultiplied(60, 50, 10, a);
                                    // Horizontal bar
                                    if let (Some(a0), Some(a1), Some(a2), Some(a3)) = (
                                        grid_to_screen(cx - arm, cy - w),
                                        grid_to_screen(cx + arm, cy - w),
                                        grid_to_screen(cx + arm, cy + w),
                                        grid_to_screen(cx - arm, cy + w),
                                    ) {
                                        painter.add(egui::Shape::convex_polygon(
                                            vec![a0, a1, a2, a3],
                                            cross_color,
                                            egui::Stroke::NONE,
                                        ));
                                    }
                                    // Vertical bar
                                    if let (Some(a0), Some(a1), Some(a2), Some(a3)) = (
                                        grid_to_screen(cx - w, cy - arm),
                                        grid_to_screen(cx + w, cy - arm),
                                        grid_to_screen(cx + w, cy + arm),
                                        grid_to_screen(cx - w, cy + arm),
                                    ) {
                                        painter.add(egui::Shape::convex_polygon(
                                            vec![a0, a1, a2, a3],
                                            cross_color,
                                            egui::Stroke::NONE,
                                        ));
                                    }
                                }
                                StructureKind::Machine(_) => {
                                    // Gear-like circle for machines
                                    let dir = match self.world.direction(entity) {
                                        Some(d) => d,
                                        None => continue,
                                    };
                                    let (dx, dy) = dir.grid_offset();
                                    let tip = grid_to_screen(cx + dx * 0.35, cy + dy * 0.35);
                                    let bl = grid_to_screen(cx - dx * 0.25 - dy * 0.2, cy - dy * 0.25 + dx * 0.2);
                                    let br = grid_to_screen(cx - dx * 0.25 + dy * 0.2, cy - dy * 0.25 - dx * 0.2);
                                    if let (Some(t), Some(l), Some(r)) = (tip, bl, br) {
                                        painter.add(egui::Shape::convex_polygon(
                                            vec![t, l, r],
                                            egui::Color32::from_rgba_unmultiplied(15, 25, 45, a),
                                            egui::Stroke::NONE,
                                        ));
                                    }
                                }
                            }

                            // Power satisfaction indicator pip (machines and power structures)
                            if matches!(kind, StructureKind::Machine(_) | StructureKind::PowerNode | StructureKind::PowerSource) {
                                if let Some(sat) = self.power_network.satisfaction(entity) {
                                    let pip_color = if sat >= 1.0 {
                                        egui::Color32::from_rgba_unmultiplied(50, 200, 50, a) // green
                                    } else if sat >= 0.5 {
                                        let t = (sat - 0.5) * 2.0; // 0..1
                                        let r = (230.0 - t * 180.0) as u8;
                                        let g = (180.0 + t * 20.0) as u8;
                                        egui::Color32::from_rgba_unmultiplied(r, g, 50, a) // yellow->green
                                    } else {
                                        let t = sat * 2.0; // 0..1
                                        let r = 200;
                                        let g = (50.0 + t * 130.0) as u8;
                                        egui::Color32::from_rgba_unmultiplied(r, g, 50, a) // red->yellow
                                    };

                                    if let Some(pip_pos) = grid_to_screen(cx + 0.35, cy + 0.35) {
                                        painter.circle_filled(pip_pos, 2.5, pip_color);
                                    }
                                }
                            }
                        }
                    }
                });
        }

        let full_output = running.egui.end_frame(&window);

        // --- Render ---
        let mut encoder = running.gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            });

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [running.gpu.config.width, running.gpu.config.height],
            pixels_per_point: window.scale_factor() as f32,
        };

        // Prepare egui (tessellate, update textures/buffers)
        let paint_jobs = running.egui.prepare(
            &running.gpu.device,
            &running.gpu.queue,
            &mut encoder,
            &screen,
            &full_output,
        );

        // Main render pass (tiles + egui overlay)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.02,
                            b: 0.05,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &running.render.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Draw tiles
            pass.set_pipeline(&running.render.pipeline);
            pass.set_vertex_buffer(0, running.render.vertex_buffer.slice(..));
            pass.set_index_buffer(running.render.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            for i in 0..tile_count {
                let offset = RenderState::dynamic_offset(i);
                pass.set_bind_group(0, &running.render.bind_group, &[offset]);
                pass.draw_indexed(0..running.render.num_indices, 0, 0..1);
            }
        }

        // Egui render pass (separate helper to unify encoder/renderer lifetimes)
        render_egui_pass(&mut encoder, &running.egui.renderer, &view, &paint_jobs, &screen);

        running.gpu.queue.submit(std::iter::once(encoder.finish()));
        running.egui.cleanup(&full_output);
        output.present();
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        let window_attrs = Window::default_attributes()
            .with_title(format!("Octofact — {{{},{}}} Hyperbolic Plane", self.cfg.p, self.cfg.q))
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800));
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("create window"),
        );
        let gpu = GpuState::new(window.clone());

        let polygon = canonical_polygon(&self.cfg);
        let (verts, indices) = build_polygon_mesh(&polygon);

        let render = RenderState::new(
            &gpu.device,
            &gpu.queue,
            gpu.config.format,
            gpu.config.width,
            gpu.config.height,
            &verts,
            &indices,
        );

        let mut tiling = TilingState::new(self.cfg);
        tiling.ensure_coverage(Complex::ZERO, 3);

        let egui = EguiIntegration::new(&gpu.device, gpu.config.format, window);
        apply_octofact_style(&egui.ctx);
        let icon_atlas = IconAtlas::generate(&egui.ctx);

        self.running = Some(RunningState {
            gpu,
            render,
            tiling,
            extra_elevation: std::collections::HashMap::new(),
            egui,
            icon_atlas,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        use winit::keyboard::PhysicalKey;

        // Handle game toggle keys BEFORE egui so Tab/Esc aren't consumed
        if let WindowEvent::KeyboardInput { ref event, .. } = event {
            if let PhysicalKey::Code(code) = event.physical_key {
                // Track shift state before rebinding check
                if code == winit::keyboard::KeyCode::ShiftLeft || code == winit::keyboard::KeyCode::ShiftRight {
                    self.input_state.shift_held = event.state.is_pressed();
                }

                // Handle rebinding mode
                if let Some(action) = self.ui.rebinding {
                    if event.state.is_pressed() {
                        // Don't bind bare modifier keys
                        if code == winit::keyboard::KeyCode::ShiftLeft || code == winit::keyboard::KeyCode::ShiftRight {
                            return;
                        }
                        let bind = if self.input_state.shift_held {
                            crate::game::input::KeyBind::with_shift(code)
                        } else {
                            crate::game::input::KeyBind::new(code)
                        };
                        self.input_state.rebind(action, bind);
                        self.config.key_bindings.insert(action, bind);
                        self.config.save();
                        self.ui.rebinding = None;
                    }
                    return;
                }

                self.input_state.on_key_event(code, event.state.is_pressed());

                // Handle toggle actions on press (before egui eats them)
                if event.state.is_pressed() {
                    if self.input_state.just_pressed(GameAction::OpenSettings) {
                        self.ui.settings_open = !self.ui.settings_open;
                    }
                    if self.input_state.just_pressed(GameAction::OpenInventory) {
                        self.ui.inventory_open = !self.ui.inventory_open;
                    }
                    if self.input_state.just_pressed(GameAction::ToggleLabels) {
                        if let Some(running) = &mut self.running {
                            running.render.labels_enabled = !running.render.labels_enabled;
                            log::info!("labels: {}", if running.render.labels_enabled { "ON" } else { "OFF" });
                        }
                    }
                    if self.input_state.just_pressed(GameAction::ToggleViewMode) {
                        self.camera.toggle_mode();
                        log::info!("view: {}", if self.camera.is_first_person() { "first-person" } else { "top-down" });
                    }
                    if self.input_state.just_pressed(GameAction::ToggleGrid) {
                        self.grid_enabled = !self.grid_enabled;
                        log::info!("grid: {}", if self.grid_enabled { "ON" } else { "OFF" });
                    }
                    if self.input_state.just_pressed(GameAction::OpenPlacement) {
                        self.ui.placement_open = !self.ui.placement_open;
                        if !self.ui.placement_open {
                            self.ui.placement_mode = None;
                        }
                    }
                    if self.input_state.just_pressed(GameAction::RotateStructure) {
                        if let Some(mode) = &mut self.ui.placement_mode {
                            mode.direction = mode.direction.rotate_cw();
                        }
                    }
                    if self.input_state.just_pressed(GameAction::RaiseTerrain) {
                        if let Some(pos) = self.ui.cursor_pos {
                            self.modify_terrain(pos.x, pos.y, 0.04);
                        }
                    }
                    if self.input_state.just_pressed(GameAction::LowerTerrain) {
                        if let Some(pos) = self.ui.cursor_pos {
                            self.modify_terrain(pos.x, pos.y, -0.04);
                        }
                    }
                }
            }
        }

        // Let egui handle events (for pointer, text input, etc.)
        if let Some(running) = &mut self.running {
            let consumed = running.egui.on_window_event(&running.gpu.window, &event);
            if consumed {
                return;
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(running) = &mut self.running {
                    running.gpu.resize(new_size.width, new_size.height);
                    running.render.resize_depth(&running.gpu.device, new_size.width, new_size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.cursor_pos = Some(position);
                // Continue drag-to-place if active
                if self.ui.belt_drag.is_some() {
                    self.handle_placement_drag(position.x, position.y);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left {
                    if state == winit::event::ElementState::Pressed {
                        if let Some(pos) = self.ui.cursor_pos {
                            if self.input_state.shift_held {
                                // Shift+click: debug spawn item on belt
                                self.debug_spawn_item(pos.x, pos.y);
                            } else if self.ui.placement_mode.is_some() {
                                self.handle_placement_click(pos.x, pos.y);
                            } else if !self.ui_is_open() {
                                if !self.try_open_machine_panel(pos.x, pos.y) {
                                    self.handle_debug_click(pos.x, pos.y);
                                }
                            } else if self.ui.machine_panel_entity.is_some() {
                                // Clicking outside while machine panel is open:
                                // try to click another machine, else close panel
                                if !self.try_open_machine_panel(pos.x, pos.y) {
                                    self.ui.machine_panel_entity = None;
                                }
                            }
                        }
                    } else {
                        // Mouse released — end drag
                        self.ui.belt_drag = None;
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if self.running.is_some() {
                    match self.render_frame() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            let gpu = &self.running.as_ref().unwrap().gpu;
                            gpu.surface.configure(&gpu.device, &gpu.config);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                        Err(e) => log::error!("render error: {e:?}"),
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let frame_dt = match self.game_loop.begin_frame() {
            Some(dt) => dt,
            None => {
                // First frame — just initialize timing, don't sim yet
                self.input_state.end_frame();
                if let Some(running) = &self.running {
                    running.gpu.window.request_redraw();
                }
                return;
            }
        };

        // Fixed timestep simulation
        let ticks = self.game_loop.accumulate(frame_dt);

        let ui_open = self.ui_is_open();
        for _ in 0..ticks {
            // Save per-tick so prev/curr are always one SIM_DT apart
            // and in adjacent coordinate frames (at most one tile crossing)
            self.game_loop.save_prev_camera(self.camera.snapshot());
            // Solve power network and propagate satisfaction to machines
            self.power_network.solve();
            for i in 0..self.machine_pool.count {
                let entity = self.machine_pool.cold.entity_id[i];
                if let Some(sat) = self.power_network.satisfaction(entity) {
                    self.machine_pool.hot.power_draw[i] = sat;
                }
            }
            self.machine_pool.tick(&self.recipes);
            self.belt_network.tick();
            self.belt_network.tick_port_transfers(&mut self.machine_pool);
            if let Some(running) = &mut self.running {
                self.camera.process_movement(
                    &self.input_state,
                    &mut running.tiling,
                    ui_open,
                    crate::sim::tick::SIM_DT,
                );
            }
            self.game_loop.save_curr_camera(self.camera.snapshot());
        }

        // Flash timer uses real frame dt for smooth fadeout
        if self.ui.flash_timer > 0.0 {
            self.ui.flash_timer = (self.ui.flash_timer - frame_dt as f32).max(0.0);
        }

        self.input_state.end_frame();

        if let Some(running) = &self.running {
            running.gpu.window.request_redraw();
        }
    }
}

/// Map a grid position at or past the tile edge to the neighbor tile's coordinate.
/// Tiles share edge positions (±32 ↔ ∓32) and the grid has 64 cells per tile,
/// so the mapping is a ±64 offset: 32→-32, 33→-31, -32→32, -33→31.
fn cross_tile_mirror(pos: (i32, i32)) -> (i32, i32) {
    let mx = if pos.0 > 31 { pos.0 - 64 } else if pos.0 < -31 { pos.0 + 64 } else { pos.0 };
    let my = if pos.1 > 31 { pos.1 - 64 } else if pos.1 < -31 { pos.1 + 64 } else { pos.1 };
    (mx, my)
}

/// Find a belt entity at the given tile + grid position with a specific direction.
fn find_same_dir_belt_at(
    world: &WorldState,
    tile_addr: &[u8],
    grid_xy: (i32, i32),
    direction: Direction,
) -> Option<EntityId> {
    let entities = world.tile_entities(tile_addr)?;
    let &entity = entities.get(&grid_xy)?;
    if world.kind(entity) == Some(StructureKind::Belt)
        && world.direction(entity) == Some(direction)
    {
        Some(entity)
    } else {
        None
    }
}
