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
use crate::hyperbolic::poincare::{canonical_polygon, polygon_disk_radius, Complex, TilingConfig};
use crate::hyperbolic::tiling::{format_address, TileAddr};
use crate::render::camera::Camera;
use crate::render::engine::{project_to_screen, RenderEngine};
use crate::render::instances::{BeltInstance, ItemInstance, MachineInstance};
use crate::render::mesh::build_polygon_mesh;
use crate::sim::belt::BeltNetwork;
use crate::sim::tick::GameLoop;
use crate::ui::placement::PlacementMode;

struct ClickResult {
    tile_idx: usize,
    grid_xy: (i32, i32),
    local_disk: Complex,
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
    renderer: Option<RenderEngine>,
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
            renderer: None,
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
        let running = self.renderer.as_ref()?;
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
            let tile = &self.renderer.as_ref().unwrap().tiling.tiles[result.tile_idx];
            log::info!(
                "{};{},{}",
                format_address(&tile.address),
                result.grid_xy.0,
                result.grid_xy.1,
            );
        }

        // Flash at snapped grid intersection projected to screen
        let running = self.renderer.as_ref().unwrap();
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
                    crate::sim::power::PowerNodeKind::Relay,
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
        let running = self.renderer.as_ref().unwrap();
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
            let running = self.renderer.as_ref().unwrap();
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
            let running = self.renderer.as_ref().unwrap();
            if let Some(neighbor_addr) = running.tiling.neighbor_tile_addr(tile_idx, edge) {
                if let Some(neighbor_entity) = find_same_dir_belt_at(
                    &self.world, &neighbor_addr, mirror, direction,
                ) {
                    self.belt_network.link_output_to_input(neighbor_entity, entity);
                }
            }
        }
    }

    /// When a machine is placed, check each port's specific adjacent cell for a belt.
    /// Uses `cell_offset` to check only the exact cell where each port lives.
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
            // The port lives at origin + cell_offset; check the adjacent cell on that side
            let port_cell = (grid_xy.0 + port.cell_offset.0, grid_xy.1 + port.cell_offset.1);
            let adj = (port_cell.0 + dx, port_cell.1 + dy);

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
    /// When a belt is placed, check all 4 adjacent cells for machines and connect ports.
    /// Uses `port_at_cell_on_side` to match by the port's exact cell offset.
    fn auto_connect_belt_to_machines(
        &mut self,
        belt_entity: EntityId,
        tile_addr: &[u8],
        grid_xy: (i32, i32),
        belt_dir: Direction,
    ) {
        use crate::sim::inserter::{belt_compatible_with_port, port_at_cell_on_side, PortKind};

        for &check_dir in &[Direction::North, Direction::East, Direction::South, Direction::West] {
            let (dx, dy) = check_dir.grid_offset_i32();
            let adj = (grid_xy.0 + dx, grid_xy.1 + dy);

            if let Some(entities) = self.world.tile_entities(tile_addr) {
                if let Some(&adj_entity) = entities.get(&adj) {
                    if let Some(StructureKind::Machine(mt)) = self.world.kind(adj_entity) {
                        if let Some(facing) = self.world.direction(adj_entity) {
                            // Compute cell offset of `adj` within the machine's footprint
                            if let Some(origin) = self.world.position(adj_entity) {
                                let cell_offset = (
                                    adj.0 - origin.gx as i32,
                                    adj.1 - origin.gy as i32,
                                );
                                // Check if there's a port at this cell on the side facing the belt
                                if let Some(port) = port_at_cell_on_side(
                                    mt,
                                    facing,
                                    cell_offset,
                                    check_dir.opposite(),
                                ) {
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

        let running = self.renderer.as_ref().unwrap();
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
            let running = self.renderer.as_ref().unwrap();
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
                    let running = self.renderer.as_ref().unwrap();
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
            let running = self.renderer.as_ref().unwrap();
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
            let running = self.renderer.as_ref().unwrap();
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

    /// Destroy the structure at the given screen position. Unregisters from
    /// all simulation systems and refunds the building item to inventory.
    fn destroy_at_cursor(&mut self, sx: f64, sy: f64) -> bool {
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return false,
        };
        let address = {
            let running = self.renderer.as_ref().unwrap();
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

        let kind = match self.world.kind(entity) {
            Some(k) => k,
            None => return false,
        };

        // Unregister from simulation systems
        match kind {
            StructureKind::Belt => {
                self.belt_network.on_belt_removed(entity);
            }
            StructureKind::Machine(_) => {
                // Close machine panel if inspecting this entity
                if self.ui.machine_panel_entity == Some(entity) {
                    self.ui.machine_panel_entity = None;
                }
                self.machine_pool.remove(entity);
                self.power_network.remove(entity);
            }
            StructureKind::PowerNode | StructureKind::PowerSource => {
                self.power_network.remove(entity);
            }
        }

        // Remove from world (handles multi-cell footprints)
        if let Some(item) = self.world.remove(&address, result.grid_xy) {
            self.inventory.add(item, 1);

            // Flash feedback
            let running = self.renderer.as_ref().unwrap();
            let width = running.gpu.config.width as f32;
            let height = running.gpu.config.height as f32;
            let scale = running.gpu.window.scale_factor() as f32;
            let aspect = width / height;
            let view_proj = self.camera.build_view_proj(aspect);

            let inv_view = self.camera.local.inverse();
            let tile_xform = running.tiling.tiles[result.tile_idx].transform;
            let combined = inv_view.compose(&tile_xform);

            let khs = self.klein_half_side;
            let divisions = 64.0_f64;
            let snap_kx = (result.grid_xy.0 as f64 / divisions) * 2.0 * khs;
            let snap_ky = (result.grid_xy.1 as f64 / divisions) * 2.0 * khs;
            let kr2 = snap_kx * snap_kx + snap_ky * snap_ky;
            let denom = 1.0 + (1.0 - kr2).max(0.0).sqrt();
            let local_disk = Complex::new(snap_kx / denom, snap_ky / denom);
            let world_disk = combined.apply(local_disk);
            let bowl = crate::hyperbolic::embedding::disk_to_bowl(world_disk);
            let elevation = running.extra_elevation.get(&result.tile_idx).copied().unwrap_or(0.0);
            let world_pos = glam::Vec3::new(bowl[0], bowl[1] + elevation, bowl[2]);

            if let Some((px, py)) = project_to_screen(world_pos, &view_proj, width, height) {
                self.ui.flash_label = format!("-{}", item.display_name());
                self.ui.flash_screen_pos = Some((px / scale, py / scale));
                self.ui.flash_timer = 0.4;
            }
            return true;
        }
        false
    }

    /// Rotate the structure at the given screen position 90° clockwise.
    /// Disconnects old belt connections and auto-reconnects with the new facing.
    fn rotate_at_cursor(&mut self, sx: f64, sy: f64) -> bool {
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return false,
        };
        let address = {
            let running = self.renderer.as_ref().unwrap();
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

        let kind = match self.world.kind(entity) {
            Some(k) => k,
            None => return false,
        };

        // Only rotate machines and power structures (not belts — belt direction is functional)
        let machine_type = match kind {
            StructureKind::Machine(mt) => Some(mt),
            StructureKind::PowerSource => None,
            _ => return false,
        };

        // Disconnect old belt connections for machines
        if machine_type.is_some() {
            self.belt_network.disconnect_machine_ports(entity);
        }

        // Rotate direction
        let new_dir = match self.world.rotate_cw(entity) {
            Some(d) => d,
            None => return false,
        };

        // Auto-reconnect ports for machines
        if let Some(mt) = machine_type {
            let origin = match self.world.position(entity) {
                Some(p) => (p.gx as i32, p.gy as i32),
                None => return false,
            };
            self.auto_connect_machine_ports(entity, &address, origin, new_dir, mt);
        }

        // Flash feedback
        let running = self.renderer.as_ref().unwrap();
        let width = running.gpu.config.width as f32;
        let height = running.gpu.config.height as f32;
        let scale = running.gpu.window.scale_factor() as f32;
        let aspect = width / height;
        let view_proj = self.camera.build_view_proj(aspect);

        let inv_view = self.camera.local.inverse();
        let tile_xform = running.tiling.tiles[result.tile_idx].transform;
        let combined = inv_view.compose(&tile_xform);

        let khs = self.klein_half_side;
        let divisions = 64.0_f64;
        let snap_kx = (result.grid_xy.0 as f64 / divisions) * 2.0 * khs;
        let snap_ky = (result.grid_xy.1 as f64 / divisions) * 2.0 * khs;
        let kr2 = snap_kx * snap_kx + snap_ky * snap_ky;
        let denom = 1.0 + (1.0 - kr2).max(0.0).sqrt();
        let local_disk = Complex::new(snap_kx / denom, snap_ky / denom);
        let world_disk = combined.apply(local_disk);
        let bowl = crate::hyperbolic::embedding::disk_to_bowl(world_disk);
        let elevation = running.extra_elevation.get(&result.tile_idx).copied().unwrap_or(0.0);
        let world_pos = glam::Vec3::new(bowl[0], bowl[1] + elevation, bowl[2]);

        if let Some((px, py)) = project_to_screen(world_pos, &view_proj, width, height) {
            self.ui.flash_label = format!("{}", new_dir.arrow_char());
            self.ui.flash_screen_pos = Some((px / scale, py / scale));
            self.ui.flash_timer = 0.3;
        }
        true
    }

    fn modify_terrain(&mut self, sx: f64, sy: f64, delta: f32) {
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return,
        };

        *self.renderer.as_mut().unwrap().extra_elevation.entry(result.tile_idx).or_insert(0.0) += delta;
    }

    fn render_frame(&mut self) -> Result<(), wgpu::SurfaceError> {
        // Use interpolated camera for smooth rendering between sim ticks
        let render_camera = self.game_loop.interpolated_camera()
            .unwrap_or_else(|| self.camera.snapshot());

        let re = self.renderer.as_mut().unwrap();
        let aspect = re.width() / re.height();
        let view_proj = render_camera.build_view_proj(aspect);
        let inv_view = render_camera.local.inverse();

        // Visibility culling + instanced tile rendering setup
        let visible = re.visible_tiles(&inv_view);
        re.build_tile_instances(&visible, &view_proj, self.grid_enabled, self.klein_half_side as f32);

        // Build belt instances from visible tiles + world state
        re.belt_instances.clear();
        for &(tile_idx, combined) in &visible {
            let tile = &re.tiling.tiles[tile_idx];
            let entities = match self.world.tile_entities(&tile.address) {
                Some(e) => e,
                None => continue,
            };
            for (&(gx, gy), &entity) in entities {
                if !matches!(self.world.kind(entity), Some(StructureKind::Belt)) {
                    continue;
                }
                let dir = match self.world.direction(entity) {
                    Some(d) => d,
                    None => continue,
                };
                let dir_float = match dir {
                    Direction::North => 0.0,
                    Direction::East => 1.0,
                    Direction::South => 2.0,
                    Direction::West => 3.0,
                };
                re.belt_instances.push(BeltInstance {
                    mobius_a: [combined.a.re as f32, combined.a.im as f32],
                    mobius_b: [combined.b.re as f32, combined.b.im as f32],
                    grid_pos: [gx as f32, gy as f32],
                    direction: dir_float,
                });
            }
        }
        re.belt_instances.upload(&re.gpu.device, &re.gpu.queue);

        // Build machine instances from visible tiles + world state
        // Only emit one instance per entity (at its origin cell).
        re.machine_instances.clear();
        for &(tile_idx, combined) in &visible {
            let tile = &re.tiling.tiles[tile_idx];
            let entities = match self.world.tile_entities(&tile.address) {
                Some(e) => e,
                None => continue,
            };
            for (&(gx, gy), &entity) in entities {
                // Skip non-origin cells to avoid duplicate instances
                if !self.world.is_origin(entity, gx, gy) {
                    continue;
                }
                let (machine_type_float, has_pool_entry) = match self.world.kind(entity) {
                    Some(StructureKind::Machine(mt)) => {
                        use crate::game::items::MachineType;
                        let f = match mt {
                            MachineType::Composer => 0.0,
                            MachineType::Inverter => 1.0,
                            MachineType::Embedder => 2.0,
                            MachineType::Quotient => 3.0,
                            MachineType::Transformer => 4.0,
                            MachineType::Source => 5.0,
                        };
                        (f, true)
                    }
                    Some(StructureKind::PowerNode) => (6.0, false),
                    Some(StructureKind::PowerSource) => (7.0, false),
                    _ => continue,
                };

                let progress = if has_pool_entry {
                    self.machine_pool
                        .state(entity)
                        .map(|s| match s {
                            crate::sim::machine::MachineState::Working => {
                                self.machine_pool.progress(entity).unwrap_or(0.0)
                            }
                            crate::sim::machine::MachineState::NoPower => -2.0,
                            _ => -1.0, // Idle, NoInput, OutputFull
                        })
                        .unwrap_or(-1.0)
                } else {
                    -1.0 // Power nodes are always "idle" visually
                };

                let power_sat = self.power_network.satisfaction(entity).unwrap_or(-1.0);
                let facing = self.world.direction(entity).unwrap_or(Direction::North);
                let facing_float = facing.rotations_from_north() as f32;

                re.machine_instances.push(MachineInstance {
                    mobius_a: [combined.a.re as f32, combined.a.im as f32],
                    mobius_b: [combined.b.re as f32, combined.b.im as f32],
                    grid_pos: [gx as f32, gy as f32],
                    machine_type: machine_type_float,
                    progress,
                    power_sat,
                    facing: facing_float,
                });
            }
        }
        re.machine_instances.upload(&re.gpu.device, &re.gpu.queue);

        // Build item instances from items riding on visible belts
        re.item_instances.clear();
        let khs = self.klein_half_side;
        let divisions = 64.0;
        for &(tile_idx, combined) in &visible {
            let tile = &re.tiling.tiles[tile_idx];
            let entities = match self.world.tile_entities(&tile.address) {
                Some(e) => e,
                None => continue,
            };
            let ma = [combined.a.re as f32, combined.a.im as f32];
            let mb = [combined.b.re as f32, combined.b.im as f32];
            for (&(gx, gy), &entity) in entities {
                if !matches!(self.world.kind(entity), Some(StructureKind::Belt)) {
                    continue;
                }
                let dir = match self.world.direction(entity) {
                    Some(d) => d,
                    None => continue,
                };
                if let Some((belt_items, offset)) = self.belt_network.entity_items(entity) {
                    let (dx, dy) = dir.grid_offset();
                    for bi in belt_items {
                        let pos_frac = (bi.pos - offset) as f64 / crate::sim::belt::FP_SCALE as f64;
                        let item_gx = gx as f64 + dx * (0.5 - pos_frac);
                        let item_gy = gy as f64 + dy * (0.5 - pos_frac);
                        let klein_x = (item_gx / divisions * 2.0 * khs) as f32;
                        let klein_y = (item_gy / divisions * 2.0 * khs) as f32;
                        let type_idx = crate::game::items::ItemId::all()
                            .iter()
                            .position(|&x| x == bi.item)
                            .unwrap_or(0) as f32;
                        re.item_instances.push(ItemInstance {
                            mobius_a: ma,
                            mobius_b: mb,
                            klein_pos: [klein_x, klein_y],
                            item_type: type_idx,
                        });
                    }
                }
            }
        }
        re.item_instances.upload(&re.gpu.device, &re.gpu.queue);

        let window = re.gpu.window.clone();
        let width = re.width();
        let height = re.height();
        let scale = re.scale_factor();

        // --- egui frame ---
        re.egui.begin_frame(&window);

        // Label overlay
        if re.render.labels_enabled {
            let egui_ctx = re.egui.ctx.clone();
            let area = egui::Area::new(egui::Id::new("tile_labels"))
                .order(egui::Order::Background)
                .interactable(false);
            area.show(&egui_ctx, |ui| {
                for &(tile_idx, combined) in &visible {
                    let tile = &re.tiling.tiles[tile_idx];
                    let disk_center = combined.apply(Complex::ZERO);
                    if disk_center.abs() > 0.9 {
                        continue;
                    }
                    let elevation = re.extra_elevation.get(&tile_idx).copied().unwrap_or(0.0);
                    let hyp = crate::hyperbolic::embedding::disk_to_bowl(disk_center);
                    let world_pos = glam::Vec3::new(hyp[0], hyp[1] + elevation, hyp[2]);
                    if let Some((sx, sy)) = project_to_screen(world_pos, &view_proj, width, height) {
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
            &re.egui.ctx.clone(),
            &mut self.ui.settings_open,
            &mut self.config,
            &mut self.input_state,
            &mut self.ui.rebinding,
        );

        // Inventory window
        crate::ui::inventory::inventory_window(
            &re.egui.ctx.clone(),
            &mut self.ui.inventory_open,
            &self.inventory,
            &re.icon_atlas,
            &self.recipes,
        );

        // Placement panel
        crate::ui::placement::placement_panel(
            &re.egui.ctx.clone(),
            &mut self.ui.placement_open,
            &self.inventory,
            &re.icon_atlas,
            &mut self.ui.placement_mode,
            self.config.debug.free_placement,
        );

        // Machine inspection panel
        if let Some(entity) = self.ui.machine_panel_entity {
            let egui_ctx = re.egui.ctx.clone();
            if let Some(action) = crate::ui::machine::machine_panel(
                &egui_ctx,
                entity,
                &self.machine_pool,
                &self.recipes,
                &re.icon_atlas,
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
                let egui_ctx = re.egui.ctx.clone();
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
            let egui_ctx = re.egui.ctx.clone();
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

        let full_output = re.egui.end_frame(&window);

        // GPU render passes + submit
        let output = re.draw_and_submit(&full_output)?;
        output.present();
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
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

        let polygon = canonical_polygon(&self.cfg);
        let (verts, indices) = build_polygon_mesh(&polygon);

        self.renderer = Some(RenderEngine::new(window, self.cfg, &verts, &indices));
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
                        if let Some(running) = &mut self.renderer {
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
                        } else if let Some(pos) = self.ui.cursor_pos {
                            self.rotate_at_cursor(pos.x, pos.y);
                        }
                    }
                    if self.input_state.just_pressed(GameAction::DestroyBuilding) {
                        if let Some(pos) = self.ui.cursor_pos {
                            self.destroy_at_cursor(pos.x, pos.y);
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
        if let Some(running) = &mut self.renderer {
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
                if let Some(running) = &mut self.renderer {
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
                if button == winit::event::MouseButton::Right
                    && state == winit::event::ElementState::Pressed
                    && self.ui.placement_mode.is_none()
                {
                    if let Some(pos) = self.ui.cursor_pos {
                        self.destroy_at_cursor(pos.x, pos.y);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if self.renderer.is_some() {
                    match self.render_frame() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            let gpu = &self.renderer.as_ref().unwrap().gpu;
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
                if let Some(running) = &self.renderer {
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
            if let Some(running) = &mut self.renderer {
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

        if let Some(running) = &self.renderer {
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
