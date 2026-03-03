use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

use crate::game::blueprint::{self, BlueprintFile, Clipboard};
use crate::game::config::GameConfig;
use crate::game::input::{GameAction, InputState};
use crate::game::inventory::Inventory;
use crate::game::recipes::RecipeIndex;
use crate::game::world::{Direction, EntityId, StructureKind, WorldState};
use crate::hyperbolic::poincare::{canonical_polygon, polygon_disk_radius, Complex, TilingConfig};
use crate::hyperbolic::cell_id::CellId;
use crate::hyperbolic::tiling::format_cell_id;
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



/// A tile in a multi-tile selection strip.
pub(crate) struct StripTile {
    /// Index into the tiling's tile array.
    tile_idx: usize,
    /// Tile address.
    id: CellId,
    /// Offset from the anchor tile (0 = anchor, positive = forward along strip).
    delta: i32,
}

/// State for an active box selection (blueprint mode).
pub(crate) struct SelectionState {
    /// Tiles in the strip, ordered by delta. Always has at least one (the anchor at delta=0).
    tiles: Vec<StripTile>,
    /// True if the strip extends along the x-axis; false for y-axis.
    /// Only meaningful when tiles.len() > 1.
    strip_axis_x: bool,
    /// Grid coordinate where the mouse went down (virtual coords for multi-tile).
    start: (i32, i32),
    /// Current grid coordinate (virtual coords for multi-tile).
    current: (i32, i32),
    /// True after mouse-up finalizes the selection.
    finalized: bool,
}

impl SelectionState {
    /// The anchor tile's index into the tiling array.
    fn tile_idx(&self) -> usize {
        self.tiles[0].tile_idx
    }

    /// The anchor tile's CellId.
    fn id(&self) -> &CellId {
        &self.tiles[0].id
    }
}

/// State for dragging belts along a gridline.
struct BeltDrag {
    tile_idx: usize,
    id: CellId,
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
    /// Currently inspected splitter entity (opens the splitter panel).
    pub splitter_panel_entity: Option<EntityId>,
    /// Currently inspected storage entity (opens the storage panel).
    pub storage_panel_entity: Option<EntityId>,
    /// Deferred save/load action from UI (processed after UI rendering).
    pub save_action: Option<crate::ui::settings::SettingsAction>,
    /// Whether blueprint box-select mode is active (B key toggle).
    pub blueprint_select: bool,
    /// Active box selection state.
    pub selection: Option<SelectionState>,
    /// Whether paste mode is active (Ctrl-V with clipboard).
    pub paste_mode: bool,
    /// Whether the blueprint manager window is open (N key toggle).
    pub blueprint_manager_open: bool,
    /// Persistent state for the blueprint manager UI.
    pub blueprint_manager_state: crate::ui::blueprint::BlueprintManagerState,
    /// True when egui has keyboard focus (e.g. text field), suppresses game hotkeys.
    pub egui_wants_keyboard: bool,
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
            splitter_panel_entity: None,
            storage_panel_entity: None,
            save_action: None,
            blueprint_select: false,
            selection: None,
            paste_mode: false,
            blueprint_manager_open: false,
            blueprint_manager_state: crate::ui::blueprint::BlueprintManagerState::new(),
            egui_wants_keyboard: false,
        }
    }

    fn is_panel_open(&self) -> bool {
        self.settings_open || self.inventory_open || self.blueprint_manager_open || self.machine_panel_entity.is_some() || self.splitter_panel_entity.is_some() || self.storage_panel_entity.is_some()
    }
}

/// Map a `StructureKind` to the `machine_type` float used in shaders.
fn kind_to_machine_type_float(kind: StructureKind) -> f32 {
    use crate::game::items::MachineType;
    match kind {
        StructureKind::Belt => 10.0,
        StructureKind::Machine(mt) => match mt {
            MachineType::Composer => 0.0,
            MachineType::Inverter => 1.0,
            MachineType::Embedder => 2.0,
            MachineType::Quotient => 3.0,
            MachineType::Transformer => 4.0,
            MachineType::Source => 5.0,
        },
        StructureKind::PowerNode => 6.0,
        StructureKind::PowerSource => 7.0,
        StructureKind::Splitter => 8.0,
        StructureKind::Storage => 9.0,
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
    splitter_pool: crate::sim::splitter::SplitterPool,
    storage_pool: crate::sim::storage::StoragePool,
    power_network: crate::sim::power::PowerNetwork,
    ui: UiState,
    grid_enabled: bool,
    klein_half_side: f64,
    clipboard: Option<Clipboard>,
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
            splitter_pool: crate::sim::splitter::SplitterPool::new(),
            storage_pool: crate::sim::storage::StoragePool::new(),
            power_network: crate::sim::power::PowerNetwork::new(),
            ui: UiState::new(),
            clipboard: None,
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
                format_cell_id(&tile.id),
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
                format_cell_id(&tile.id),
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

        // Register splitter with simulation pool and connect to adjacent belts
        if mode.item == crate::game::items::ItemId::Splitter {
            self.splitter_pool.add(entity);
            self.auto_connect_splitter_to_belts(entity, address, grid_xy);
        }

        // Register storage building with simulation pool and auto-connect ports
        if mode.item == crate::game::items::ItemId::Storage {
            self.storage_pool.add(entity);
            self.auto_connect_storage_to_belts(entity, address, grid_xy, mode.direction);
        }

        // Auto-connect belt to adjacent machines, splitters, and storage
        if mode.item == crate::game::items::ItemId::Belt {
            self.auto_connect_belt_to_machines(entity, address, grid_xy, mode.direction);
            self.auto_connect_belt_to_splitters(entity, address, grid_xy, mode.direction);
            self.auto_connect_belt_to_storage(entity, address, grid_xy, mode.direction);
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
            if let Some(neighbor_id) = running.tiling.neighbor_tile_id(tile_idx, edge) {
                if let Some(neighbor_entity) = find_same_dir_belt_at(
                    &self.world, neighbor_id.word(), mirror, direction,
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
            if let Some(neighbor_id) = running.tiling.neighbor_tile_id(tile_idx, edge) {
                if let Some(neighbor_entity) = find_same_dir_belt_at(
                    &self.world, neighbor_id.word(), mirror, direction,
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

    /// When a belt is placed, check the cell ahead and behind for splitters.
    /// A belt pointing toward a splitter → belt feeds into it (input).
    /// A belt pointing away from a splitter → splitter feeds into belt (output).
    fn auto_connect_belt_to_splitters(
        &mut self,
        belt_entity: EntityId,
        tile_addr: &[u8],
        grid_xy: (i32, i32),
        belt_dir: Direction,
    ) {
        let (dx, dy) = belt_dir.grid_offset_i32();
        let ahead = (grid_xy.0 + dx, grid_xy.1 + dy);
        let behind = (grid_xy.0 - dx, grid_xy.1 - dy);

        // Ahead: belt output faces a splitter → belt is input to splitter
        if let Some(entities) = self.world.tile_entities(tile_addr) {
            if let Some(&adj_entity) = entities.get(&ahead) {
                if self.world.kind(adj_entity) == Some(StructureKind::Splitter)
                    && self.belt_network.connect_belt_to_splitter(belt_entity, adj_entity)
                {
                    self.splitter_pool.add_input(adj_entity, belt_entity);
                    self.splitter_pool.detect_mode(adj_entity);
                }
            }
        }

        // Behind: splitter feeds into belt input → belt is output from splitter
        if let Some(entities) = self.world.tile_entities(tile_addr) {
            if let Some(&adj_entity) = entities.get(&behind) {
                if self.world.kind(adj_entity) == Some(StructureKind::Splitter)
                    && self.belt_network.connect_splitter_to_belt(belt_entity, adj_entity)
                {
                    self.splitter_pool.add_output(adj_entity, belt_entity);
                    self.splitter_pool.detect_mode(adj_entity);
                }
            }
        }
    }

    /// When a splitter is placed, scan all 4 adjacent cells for existing belts
    /// and connect them based on their direction relative to the splitter.
    fn auto_connect_splitter_to_belts(
        &mut self,
        splitter_entity: EntityId,
        tile_addr: &[u8],
        grid_xy: (i32, i32),
    ) {
        for &check_dir in &[Direction::North, Direction::East, Direction::South, Direction::West] {
            let (dx, dy) = check_dir.grid_offset_i32();
            let adj = (grid_xy.0 + dx, grid_xy.1 + dy);

            let adj_entity = match self.world.tile_entities(tile_addr)
                .and_then(|e| e.get(&adj).copied())
            {
                Some(e) => e,
                None => continue,
            };

            if self.world.kind(adj_entity) != Some(StructureKind::Belt) {
                continue;
            }
            let belt_dir = match self.world.direction(adj_entity) {
                Some(d) => d,
                None => continue,
            };

            // Belt at adj going toward splitter: belt_dir == check_dir.opposite()
            // → belt output feeds into splitter (belt is an input)
            if belt_dir == check_dir.opposite() {
                if self.belt_network.connect_belt_to_splitter(adj_entity, splitter_entity) {
                    self.splitter_pool.add_input(splitter_entity, adj_entity);
                }
            }
            // Belt at adj going away from splitter: belt_dir == check_dir
            // → splitter feeds into belt input (belt is an output)
            else if belt_dir == check_dir
                && self.belt_network.connect_splitter_to_belt(adj_entity, splitter_entity)
            {
                self.splitter_pool.add_output(splitter_entity, adj_entity);
            }
        }

        self.splitter_pool.detect_mode(splitter_entity);
    }

    /// When a belt is placed, check all 4 adjacent cells for storage buildings and connect ports.
    /// Uses `structure_port_at_cell_on_side` to match by the port's exact cell offset.
    fn auto_connect_belt_to_storage(
        &mut self,
        belt_entity: EntityId,
        tile_addr: &[u8],
        grid_xy: (i32, i32),
        belt_dir: Direction,
    ) {
        use crate::sim::inserter::{belt_compatible_with_port, structure_port_at_cell_on_side, PortKind};

        for &check_dir in &[Direction::North, Direction::East, Direction::South, Direction::West] {
            let (dx, dy) = check_dir.grid_offset_i32();
            let adj = (grid_xy.0 + dx, grid_xy.1 + dy);

            if let Some(entities) = self.world.tile_entities(tile_addr) {
                if let Some(&adj_entity) = entities.get(&adj) {
                    if self.world.kind(adj_entity) == Some(StructureKind::Storage) {
                        if let Some(facing) = self.world.direction(adj_entity) {
                            if let Some(origin) = self.world.position(adj_entity) {
                                let cell_offset = (
                                    adj.0 - origin.gx as i32,
                                    adj.1 - origin.gy as i32,
                                );
                                if let Some(port) = structure_port_at_cell_on_side(
                                    StructureKind::Storage,
                                    facing,
                                    cell_offset,
                                    check_dir.opposite(),
                                ) {
                                    if belt_compatible_with_port(&port, belt_dir) {
                                        match port.kind {
                                            PortKind::Input => {
                                                self.belt_network.connect_belt_to_storage_input(
                                                    belt_entity,
                                                    adj_entity,
                                                    port.slot,
                                                );
                                            }
                                            PortKind::Output => {
                                                self.belt_network.connect_storage_output_to_belt(
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

    /// When a storage building is placed, scan adjacent cells for existing belts
    /// and connect them to the storage's ports.
    fn auto_connect_storage_to_belts(
        &mut self,
        storage_entity: EntityId,
        tile_addr: &[u8],
        grid_xy: (i32, i32),
        facing: Direction,
    ) {
        use crate::sim::inserter::{belt_compatible_with_port, rotated_structure_ports, PortKind};

        for port in rotated_structure_ports(StructureKind::Storage, facing) {
            let (dx, dy) = port.side.grid_offset_i32();
            let port_cell = (grid_xy.0 + port.cell_offset.0, grid_xy.1 + port.cell_offset.1);
            let adj = (port_cell.0 + dx, port_cell.1 + dy);

            if let Some(entities) = self.world.tile_entities(tile_addr) {
                if let Some(&belt_entity) = entities.get(&adj) {
                    if self.world.kind(belt_entity) == Some(StructureKind::Belt) {
                        if let Some(belt_dir) = self.world.direction(belt_entity) {
                            if belt_compatible_with_port(&port, belt_dir) {
                                match port.kind {
                                    PortKind::Input => {
                                        self.belt_network.connect_belt_to_storage_input(
                                            belt_entity,
                                            storage_entity,
                                            port.slot,
                                        );
                                    }
                                    PortKind::Output => {
                                        self.belt_network.connect_storage_output_to_belt(
                                            belt_entity,
                                            storage_entity,
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
        let cell_id = running.tiling.tiles[result.tile_idx].id.clone();

        if self.try_place_at(result.tile_idx, cell_id.word(), result.grid_xy, &mode) {
            // Lock drag axis parallel to the belt's facing direction
            let horizontal = matches!(mode.direction, Direction::East | Direction::West);
            let (fixed_coord, last_free) = if horizontal {
                (result.grid_xy.1, result.grid_xy.0) // fixed gy, free gx
            } else {
                (result.grid_xy.0, result.grid_xy.1) // fixed gx, free gy
            };
            self.ui.belt_drag = Some(BeltDrag {
                tile_idx: result.tile_idx,
                id: cell_id,
                horizontal,
                fixed_coord,
                last_free,
            });
        }
    }

    /// Walk `|delta|` steps from a tile to find the neighbor at that offset along a strip axis.
    ///
    /// Returns `(tile_idx, CellId)` of the target tile, or `None` if not loaded.
    fn walk_tile_strip(&self, anchor_idx: usize, delta: i32, axis_x: bool) -> Option<(usize, CellId)> {
        if delta == 0 {
            let running = self.renderer.as_ref().unwrap();
            return Some((anchor_idx, running.tiling.tiles[anchor_idx].id.clone()));
        }

        let running = self.renderer.as_ref().unwrap();
        let edge = if axis_x {
            if delta > 0 { 0u8 } else { 2u8 } // East / West
        } else if delta > 0 {
            1u8 // South
        } else {
            3u8 // North
        };

        let mut current_idx = anchor_idx;
        for _ in 0..delta.unsigned_abs() {
            let neighbor_id = running.tiling.neighbor_tile_id(current_idx, edge)?;
            let next_idx = running.tiling.find_tile(&neighbor_id)?;
            current_idx = next_idx;
        }

        let id = running.tiling.tiles[current_idx].id.clone();
        Some((current_idx, id))
    }

    /// Handle a click while in paste mode: batch-place clipboard entries.
    ///
    /// Placement order: non-belt structures first, then belts, so that belts
    /// can auto-connect to freshly placed machines/splitters/storage.
    fn handle_paste_click(&mut self, sx: f64, sy: f64) {
        let clipboard = match &self.clipboard {
            Some(c) => c.clone(),
            None => {
                self.ui.paste_mode = false;
                return;
            }
        };

        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return,
        };

        let anchor = result.grid_xy;
        let anchor_tile_idx = result.tile_idx;
        let running = self.renderer.as_ref().unwrap();
        let cell_id = running.tiling.tiles[anchor_tile_idx].id.clone();
        let address = cell_id.word();

        // Detect multi-tile clipboard or paste crossing a tile boundary
        let max_paste_x = anchor.0 + clipboard.width - 1;
        let max_paste_y = anchor.1 + clipboard.height - 1;
        let crosses_x = max_paste_x > 32 || anchor.0 < -32;
        let crosses_y = max_paste_y > 32 || anchor.1 < -32;
        let multi_tile = clipboard.width > 64 || clipboard.height > 64 || crosses_x || crosses_y;

        if multi_tile {
            if crosses_x && crosses_y {
                self.ui.flash_label = "Can't paste across tile corner".to_string();
                self.ui.flash_timer = 1.5;
                self.ui.flash_screen_pos = None;
                return;
            }
            let strip_axis_x = if clipboard.width > 64 { true }
                else if clipboard.height > 64 { false }
                else { crosses_x };

            // Check inventory
            if !self.config.debug.free_placement {
                let costs = blueprint::required_items(&clipboard);
                for &(item, count) in &costs {
                    if self.inventory.count(item) < count as u32 {
                        self.ui.flash_label = format!(
                            "Need {} more {}",
                            count as u32 - self.inventory.count(item),
                            item.display_name(),
                        );
                        self.ui.flash_timer = 1.5;
                        self.ui.flash_screen_pos = None;
                        return;
                    }
                }
            }

            // Build tile cache: delta → (tile_idx, CellId, tile_address)
            let mut tile_cache: std::collections::HashMap<i32, (usize, CellId)> = std::collections::HashMap::new();
            tile_cache.insert(0, (anchor_tile_idx, cell_id.clone()));

            // Pre-resolve all needed tile deltas
            for entry in &clipboard.entries {
                let (offset_x, offset_y) = (anchor.0 + entry.offset.0, anchor.1 + entry.offset.1);
                let (td, _local) = if strip_axis_x {
                    blueprint::virtual_to_tile_local(offset_x)
                } else {
                    blueprint::virtual_to_tile_local(offset_y)
                };
                if let std::collections::hash_map::Entry::Vacant(e) = tile_cache.entry(td) {
                    match self.walk_tile_strip(anchor_tile_idx, td, strip_axis_x) {
                        Some(r) => { e.insert(r); }
                        None => {
                            self.ui.flash_label = "Move closer — target tile not loaded".to_string();
                            self.ui.flash_timer = 1.5;
                            self.ui.flash_screen_pos = None;
                            return;
                        }
                    }
                }
            }

            // Check collisions on all target tiles
            for entry in &clipboard.entries {
                let (offset_x, offset_y) = (anchor.0 + entry.offset.0, anchor.1 + entry.offset.1);
                let (td, local_strip) = if strip_axis_x {
                    blueprint::virtual_to_tile_local(offset_x)
                } else {
                    blueprint::virtual_to_tile_local(offset_y)
                };
                let local_xy = if strip_axis_x { (local_strip, offset_y) } else { (offset_x, local_strip) };
                let (_, ref target_id) = tile_cache[&td];

                let (fw, fh) = entry.kind.footprint();
                let footprint = entry.direction.rotate_footprint(fw, fh);
                let cells = super::game::world::occupied_cells(local_xy, footprint);
                for &cell in &cells {
                    if self.world.tile_entities(target_id.word())
                        .and_then(|m| m.get(&cell))
                        .is_some()
                    {
                        self.ui.flash_label = "Can't paste: collision".to_string();
                        self.ui.flash_timer = 1.5;
                        self.ui.flash_screen_pos = None;
                        return;
                    }
                }
            }

            // Partition entries: non-belts first, belts second
            let mut non_belts: Vec<&blueprint::BlueprintEntry> = Vec::new();
            let mut belts: Vec<&blueprint::BlueprintEntry> = Vec::new();
            for entry in &clipboard.entries {
                if entry.kind == StructureKind::Belt {
                    belts.push(entry);
                } else {
                    non_belts.push(entry);
                }
            }

            let mut placed = 0u32;

            for entry in non_belts.iter().chain(belts.iter()) {
                let (offset_x, offset_y) = (anchor.0 + entry.offset.0, anchor.1 + entry.offset.1);
                let (td, local_strip) = if strip_axis_x {
                    blueprint::virtual_to_tile_local(offset_x)
                } else {
                    blueprint::virtual_to_tile_local(offset_y)
                };
                let local_xy = if strip_axis_x { (local_strip, offset_y) } else { (offset_x, local_strip) };
                let (target_tile_idx, ref target_id) = tile_cache[&td];

                let mode = PlacementMode {
                    item: entry.kind.to_item(),
                    direction: entry.direction,
                };
                if self.try_place_at(target_tile_idx, target_id.word(), local_xy, &mode) {
                    placed += 1;
                }
            }

            // Restore recipes
            for entry in &clipboard.entries {
                if let Some(recipe_idx) = entry.recipe {
                    let (offset_x, offset_y) = (anchor.0 + entry.offset.0, anchor.1 + entry.offset.1);
                    let (td, local_strip) = if strip_axis_x {
                        blueprint::virtual_to_tile_local(offset_x)
                    } else {
                        blueprint::virtual_to_tile_local(offset_y)
                    };
                    let local_xy = if strip_axis_x { (local_strip, offset_y) } else { (offset_x, local_strip) };
                    let (_, ref target_id) = tile_cache[&td];
                    if let Some(entities) = self.world.tile_entities(target_id.word()) {
                        if let Some(&entity) = entities.get(&local_xy) {
                            self.machine_pool.set_recipe(entity, Some(recipe_idx));
                        }
                    }
                }
            }

            self.ui.flash_label = format!("Pasted {placed} structures");
            self.ui.flash_timer = 1.5;
            self.ui.flash_screen_pos = None;
        } else {
            // Single-tile paste (original path)

            // Check all entries can be placed
            let checks = blueprint::can_paste(&self.world, address, anchor, &clipboard);
            if checks.iter().any(|(_i, ok)| !*ok) {
                self.ui.flash_label = "Can't paste: collision".to_string();
                self.ui.flash_timer = 1.5;
                self.ui.flash_screen_pos = None;
                return;
            }

            // Check inventory has enough items
            if !self.config.debug.free_placement {
                let costs = blueprint::required_items(&clipboard);
                for &(item, count) in &costs {
                    if self.inventory.count(item) < count as u32 {
                        self.ui.flash_label = format!(
                            "Need {} more {}",
                            count as u32 - self.inventory.count(item),
                            item.display_name(),
                        );
                        self.ui.flash_timer = 1.5;
                        self.ui.flash_screen_pos = None;
                        return;
                    }
                }
            }

            // Partition entries: non-belts first, belts second
            let mut non_belts: Vec<&blueprint::BlueprintEntry> = Vec::new();
            let mut belts: Vec<&blueprint::BlueprintEntry> = Vec::new();
            for entry in &clipboard.entries {
                if entry.kind == StructureKind::Belt {
                    belts.push(entry);
                } else {
                    non_belts.push(entry);
                }
            }

            let mut placed = 0u32;

            // Place non-belt structures first
            for entry in non_belts.iter().chain(belts.iter()) {
                let grid_xy = (anchor.0 + entry.offset.0, anchor.1 + entry.offset.1);
                let mode = PlacementMode {
                    item: entry.kind.to_item(),
                    direction: entry.direction,
                };
                if self.try_place_at(anchor_tile_idx, address, grid_xy, &mode) {
                    placed += 1;
                }
            }

            // Restore recipes for pasted machines
            for entry in &clipboard.entries {
                if let Some(recipe_idx) = entry.recipe {
                    let grid_xy = (anchor.0 + entry.offset.0, anchor.1 + entry.offset.1);
                    if let Some(entities) = self.world.tile_entities(address) {
                        if let Some(&entity) = entities.get(&grid_xy) {
                            self.machine_pool.set_recipe(entity, Some(recipe_idx));
                        }
                    }
                }
            }

            self.ui.flash_label = format!("Pasted {placed} structures");
            self.ui.flash_timer = 1.5;
            self.ui.flash_screen_pos = None;
        }

        // Stay in paste mode for repeated pasting
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
        let old_id = drag.id.clone();
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
                self.try_place_at(old_tile_idx, old_id.word(), grid_xy, &mode);
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
                    self.try_place_at(old_tile_idx, old_id.word(), grid_xy, &mode);
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
                let (new_tile_idx, new_cell_id) = {
                    let running = self.renderer.as_ref().unwrap();
                    (result.tile_idx, running.tiling.tiles[result.tile_idx].id.clone())
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
                    self.try_place_at(new_tile_idx, new_cell_id.word(), grid_xy, &mode);
                    if current == new_target { break; }
                    current += inward;
                }

                // Switch drag to the new tile
                self.ui.belt_drag = Some(BeltDrag {
                    tile_idx: new_tile_idx,
                    id: new_cell_id,
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
            running.tiling.tiles[result.tile_idx].id.clone()
        };
        let entities = match self.world.tile_entities(address.word()) {
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
            running.tiling.tiles[result.tile_idx].id.clone()
        };
        let entities = match self.world.tile_entities(address.word()) {
            Some(e) => e,
            None => return false,
        };
        let &entity = match entities.get(&result.grid_xy) {
            Some(e) => e,
            None => return false,
        };
        match self.world.kind(entity) {
            Some(StructureKind::Machine(_)) => {
                self.ui.splitter_panel_entity = None;
                self.ui.storage_panel_entity = None;
                self.ui.machine_panel_entity = Some(entity);
                true
            }
            Some(StructureKind::Splitter) => {
                self.ui.machine_panel_entity = None;
                self.ui.storage_panel_entity = None;
                self.ui.splitter_panel_entity = Some(entity);
                true
            }
            Some(StructureKind::Storage) => {
                self.ui.machine_panel_entity = None;
                self.ui.splitter_panel_entity = None;
                self.ui.storage_panel_entity = Some(entity);
                true
            }
            _ => false,
        }
    }

    /// Unregister an entity from all simulation pools and remove it from the
    /// world. Returns the refunded item if successful. Does NOT add to inventory
    /// — callers decide what to do with the returned item.
    fn destroy_entity_at(&mut self, tile_address: &[u8], grid_xy: (i32, i32)) -> Option<crate::game::items::ItemId> {
        let entities = self.world.tile_entities(tile_address)?;
        let &entity = entities.get(&grid_xy)?;
        let kind = self.world.kind(entity)?;

        // Unregister from simulation systems
        match kind {
            StructureKind::Belt => {
                let (output_splitter, input_splitter) =
                    self.belt_network.line_splitter_connections(entity);
                if let Some(se) = output_splitter {
                    self.splitter_pool.disconnect_belt(se, entity);
                    self.splitter_pool.detect_mode(se);
                }
                if let Some(se) = input_splitter {
                    self.splitter_pool.disconnect_belt(se, entity);
                    self.splitter_pool.detect_mode(se);
                }
                self.belt_network.on_belt_removed(entity);
            }
            StructureKind::Machine(_) => {
                if self.ui.machine_panel_entity == Some(entity) {
                    self.ui.machine_panel_entity = None;
                }
                self.machine_pool.remove(entity);
                self.power_network.remove(entity);
            }
            StructureKind::Splitter => {
                if self.ui.splitter_panel_entity == Some(entity) {
                    self.ui.splitter_panel_entity = None;
                }
                self.belt_network.disconnect_splitter_ports(entity);
                self.splitter_pool.remove(entity);
            }
            StructureKind::Storage => {
                if self.ui.storage_panel_entity == Some(entity) {
                    self.ui.storage_panel_entity = None;
                }
                // Return stored items to inventory
                if let Some(state) = self.storage_pool.get(entity) {
                    for slot in &state.slots {
                        if slot.count > 0 {
                            self.inventory.add(slot.item, slot.count as u32);
                        }
                    }
                }
                self.belt_network.disconnect_storage_ports(entity);
                self.storage_pool.remove(entity);
            }
            StructureKind::PowerNode | StructureKind::PowerSource => {
                self.power_network.remove(entity);
            }
        }

        self.world.remove(tile_address, grid_xy)
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
            running.tiling.tiles[result.tile_idx].id.clone()
        };

        if let Some(item) = self.destroy_entity_at(address.word(), result.grid_xy) {
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

    /// Copy selected structures to the clipboard.
    fn blueprint_copy(&mut self) {
        // Accept any selection (finalized or in-progress drag)
        let sel = match &self.ui.selection {
            Some(s) => s,
            None => return,
        };
        let min_x = sel.start.0.min(sel.current.0);
        let max_x = sel.start.0.max(sel.current.0);
        let min_y = sel.start.1.min(sel.current.1);
        let max_y = sel.start.1.max(sel.current.1);

        let clip = if sel.tiles.len() > 1 {
            let tile_refs: Vec<(&[u8], i32)> = sel.tiles.iter()
                .map(|t| (t.id.word(), t.delta))
                .collect();
            blueprint::capture_strip(
                &self.world,
                &self.machine_pool,
                &self.storage_pool,
                &tile_refs,
                (min_x, min_y),
                (max_x, max_y),
                sel.strip_axis_x,
                self.cfg.q,
            )
        } else {
            let tile_addr = sel.id().word();
            blueprint::capture_region(
                &self.world,
                &self.machine_pool,
                &self.storage_pool,
                tile_addr,
                (min_x, min_y),
                (max_x, max_y),
                self.cfg.q,
            )
        };
        let count = clip.entries.len();
        self.clipboard = Some(clip);
        self.ui.flash_label = format!("Copied {count} structure{}", if count == 1 { "" } else { "s" });
        self.ui.flash_timer = 1.5;
        self.ui.flash_screen_pos = None;
        log::info!("blueprint copy: {count} structures");
    }

    /// Cut selected structures: copy to clipboard, then destroy originals.
    fn blueprint_cut(&mut self) {
        // Accept any selection (finalized or in-progress drag)
        let sel = match &self.ui.selection {
            Some(s) => s,
            None => return,
        };
        let min_x = sel.start.0.min(sel.current.0);
        let max_x = sel.start.0.max(sel.current.0);
        let min_y = sel.start.1.min(sel.current.1);
        let max_y = sel.start.1.max(sel.current.1);

        // Collect tile info before mutable borrows
        let strip_tiles: Vec<(Vec<u8>, i32)> = sel.tiles.iter()
            .map(|t| (t.id.word().to_vec(), t.delta))
            .collect();
        let strip_axis_x = sel.strip_axis_x;
        let multi_tile = sel.tiles.len() > 1;

        // Capture first
        let clip = if multi_tile {
            let tile_refs: Vec<(&[u8], i32)> = strip_tiles.iter()
                .map(|(addr, delta)| (addr.as_slice(), *delta))
                .collect();
            blueprint::capture_strip(
                &self.world,
                &self.machine_pool,
                &self.storage_pool,
                &tile_refs,
                (min_x, min_y),
                (max_x, max_y),
                strip_axis_x,
                self.cfg.q,
            )
        } else {
            blueprint::capture_region(
                &self.world,
                &self.machine_pool,
                &self.storage_pool,
                &strip_tiles[0].0,
                (min_x, min_y),
                (max_x, max_y),
                self.cfg.q,
            )
        };
        let count = clip.entries.len();

        // Destroy all entities in the selection rectangle across all tiles.
        for (tile_addr, delta) in &strip_tiles {
            let (local_min_x, local_max_x, local_min_y, local_max_y) = if multi_tile {
                if strip_axis_x {
                    let tile_vmin = blueprint::tile_local_to_virtual(*delta, -32);
                    let tile_vmax = blueprint::tile_local_to_virtual(*delta, 32);
                    let clipped_vmin = min_x.max(tile_vmin);
                    let clipped_vmax = max_x.min(tile_vmax);
                    if clipped_vmin > clipped_vmax { continue; }
                    let lmin = clipped_vmin - delta * 64;
                    let lmax = clipped_vmax - delta * 64;
                    (lmin, lmax, min_y, max_y)
                } else {
                    let tile_vmin = blueprint::tile_local_to_virtual(*delta, -32);
                    let tile_vmax = blueprint::tile_local_to_virtual(*delta, 32);
                    let clipped_vmin = min_y.max(tile_vmin);
                    let clipped_vmax = max_y.min(tile_vmax);
                    if clipped_vmin > clipped_vmax { continue; }
                    let lmin = clipped_vmin - delta * 64;
                    let lmax = clipped_vmax - delta * 64;
                    (min_x, max_x, lmin, lmax)
                }
            } else {
                (min_x, max_x, min_y, max_y)
            };

            let mut to_destroy = Vec::new();
            if let Some(entities) = self.world.tile_entities(tile_addr) {
                let mut seen = std::collections::HashSet::new();
                for gy in local_min_y..=local_max_y {
                    for gx in local_min_x..=local_max_x {
                        if let Some(&entity) = entities.get(&(gx, gy)) {
                            if seen.insert(entity) {
                                if let Some(pos) = self.world.position(entity) {
                                    to_destroy.push((pos.gx as i32, pos.gy as i32));
                                }
                            }
                        }
                    }
                }
            }
            for grid_xy in to_destroy {
                if let Some(item) = self.destroy_entity_at(tile_addr, grid_xy) {
                    self.inventory.add(item, 1);
                }
            }
        }

        self.clipboard = Some(clip);
        self.ui.selection = None;
        self.ui.flash_label = format!("Cut {count} structure{}", if count == 1 { "" } else { "s" });
        self.ui.flash_timer = 1.5;
        self.ui.flash_screen_pos = None;
        log::info!("blueprint cut: {count} structures");
    }

    /// Handle box selection mouse-down: begin a new selection at the clicked grid cell.
    fn begin_box_selection(&mut self, sx: f64, sy: f64) {
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return,
        };
        let address = {
            let running = self.renderer.as_ref().unwrap();
            running.tiling.tiles[result.tile_idx].id.clone()
        };
        self.ui.selection = Some(SelectionState {
            tiles: vec![StripTile {
                tile_idx: result.tile_idx,
                id: address,
                delta: 0,
            }],
            strip_axis_x: true,
            start: result.grid_xy,
            current: result.grid_xy,
            finalized: false,
        });
    }

    /// Handle box selection mouse-move: update the current corner of the selection.
    fn update_box_selection(&mut self, sx: f64, sy: f64) {
        let sel = match &self.ui.selection {
            Some(s) if !s.finalized => s,
            _ => return,
        };
        let result = match self.find_clicked_tile(sx, sy) {
            Some(r) => r,
            None => return,
        };

        // Check if cursor is on a tile already in the strip
        let cursor_tile_idx = result.tile_idx;
        let running = self.renderer.as_ref().unwrap();
        let cursor_cell_id = running.tiling.tiles[cursor_tile_idx].id.clone();

        if let Some(strip_pos) = sel.tiles.iter().position(|t| t.id == cursor_cell_id) {
            // Cursor on an existing strip tile: update current in virtual coords
            let delta = sel.tiles[strip_pos].delta;
            let strip_axis_x = sel.strip_axis_x;

            // Trim strip: remove tiles with delta further than this one
            if let Some(sel) = &mut self.ui.selection {
                let current_delta = sel.tiles[strip_pos].delta;
                sel.tiles.retain(|t| {
                    if current_delta >= 0 {
                        t.delta <= current_delta
                    } else {
                        t.delta >= current_delta
                    }
                });
            }

            if let Some(sel) = &mut self.ui.selection {
                if strip_axis_x {
                    sel.current = (
                        blueprint::tile_local_to_virtual(delta, result.grid_xy.0),
                        result.grid_xy.1,
                    );
                } else {
                    sel.current = (
                        result.grid_xy.0,
                        blueprint::tile_local_to_virtual(delta, result.grid_xy.1),
                    );
                }
            }
        } else {
            // Cursor on a new tile — check if it's a neighbor of the last strip tile
            let last_tile = sel.tiles.last().unwrap();
            let last_delta = last_tile.delta;
            let last_tile_idx = last_tile.tile_idx;
            let num_tiles = sel.tiles.len();

            // Check all 4 edges for adjacency
            let mut edge_found = None;
            for edge in 0..4u8 {
                if let Some(neighbor_id) = running.tiling.neighbor_tile_id(last_tile_idx, edge) {
                    if neighbor_id == cursor_cell_id {
                        edge_found = Some(edge);
                        break;
                    }
                }
            }

            if let Some(edge) = edge_found {
                // Determine strip axis from the crossing direction
                let crossing_axis_x = edge == 0 || edge == 2; // East/West = x-axis
                let new_delta = match edge {
                    0 | 1 => last_delta + 1, // East or South = positive direction
                    2 | 3 => last_delta - 1, // West or North = negative direction
                    _ => unreachable!(),
                };

                if num_tiles == 1 || crossing_axis_x == sel.strip_axis_x {
                    if let Some(sel) = &mut self.ui.selection {
                        if num_tiles == 1 {
                            sel.strip_axis_x = crossing_axis_x;
                        }
                        sel.tiles.push(StripTile {
                            tile_idx: cursor_tile_idx,
                            id: cursor_cell_id,
                            delta: new_delta,
                        });
                        if crossing_axis_x {
                            sel.current = (
                                blueprint::tile_local_to_virtual(new_delta, result.grid_xy.0),
                                result.grid_xy.1,
                            );
                        } else {
                            sel.current = (
                                result.grid_xy.0,
                                blueprint::tile_local_to_virtual(new_delta, result.grid_xy.1),
                            );
                        }
                    }
                }
            }
            // If not adjacent or wrong axis, ignore (don't update selection)
        }
    }

    /// Handle box selection mouse-up: finalize the selection.
    fn finalize_box_selection(&mut self) {
        if let Some(sel) = &mut self.ui.selection {
            if sel.finalized {
                return;
            }

            // Vertex exclusion check for multi-tile selections
            if sel.tiles.len() > 1 {
                // Check if bounding box in any tile includes corner cells
                for strip_tile in &sel.tiles {
                    let (local_min_x, local_max_x, local_min_y, local_max_y) =
                        if sel.strip_axis_x {
                            let (_, lsx) = blueprint::virtual_to_tile_local(sel.start.0);
                            let (_, lcx) = blueprint::virtual_to_tile_local(sel.current.0);
                            (lsx.min(lcx), lsx.max(lcx), sel.start.1, sel.current.1)
                        } else {
                            let (_, lsy) = blueprint::virtual_to_tile_local(sel.start.1);
                            let (_, lcy) = blueprint::virtual_to_tile_local(sel.current.1);
                            (sel.start.0, sel.current.0, lsy.min(lcy), lsy.max(lcy))
                        };

                    let min_gx = local_min_x.min(local_max_x);
                    let max_gx = local_min_x.max(local_max_x);
                    let min_gy = local_min_y.min(local_max_y);
                    let max_gy = local_min_y.max(local_max_y);

                    // Check corners of this sub-rectangle
                    let _ = strip_tile; // use the tile for the range check
                    for &gx in &[min_gx, max_gx] {
                        for &gy in &[min_gy, max_gy] {
                            if blueprint::is_vertex_cell(gx, gy) {
                                self.ui.flash_label = "Can't select: includes tile vertex".to_string();
                                self.ui.flash_timer = 1.5;
                                self.ui.flash_screen_pos = None;
                                self.ui.selection = None;
                                return;
                            }
                        }
                    }
                }
            }

            sel.finalized = true;
        }
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
            running.tiling.tiles[result.tile_idx].id.clone()
        };
        let entities = match self.world.tile_entities(address.word()) {
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

        // Only rotate machines, storage, and power structures (not belts — belt direction is functional)
        let machine_type = match kind {
            StructureKind::Machine(mt) => Some(mt),
            StructureKind::Storage | StructureKind::PowerSource => None,
            _ => return false,
        };

        // Disconnect old belt connections for machines and storage
        if machine_type.is_some() {
            self.belt_network.disconnect_machine_ports(entity);
        }
        if kind == StructureKind::Storage {
            self.belt_network.disconnect_storage_ports(entity);
        }

        // Rotate direction
        let new_dir = match self.world.rotate_cw(entity) {
            Some(d) => d,
            None => return false,
        };

        // Auto-reconnect ports for machines and storage
        if let Some(mt) = machine_type {
            let origin = match self.world.position(entity) {
                Some(p) => (p.gx as i32, p.gy as i32),
                None => return false,
            };
            self.auto_connect_machine_ports(entity, address.word(), origin, new_dir, mt);
        }
        if kind == StructureKind::Storage {
            let origin = match self.world.position(entity) {
                Some(p) => (p.gx as i32, p.gy as i32),
                None => return false,
            };
            self.auto_connect_storage_to_belts(entity, address.word(), origin, new_dir);
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
        let time = self.game_loop.elapsed_secs();
        re.build_tile_instances(&visible, &view_proj, self.grid_enabled, self.klein_half_side as f32, time, render_camera.height);

        // Build belt instances from visible tiles + world state
        re.belt_instances.clear();
        for &(tile_idx, combined) in &visible {
            let tile = &re.tiling.tiles[tile_idx];
            let entities = match self.world.tile_entities(tile.id.word()) {
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
        re.topper_instances.clear();
        for &(tile_idx, combined) in &visible {
            let tile = &re.tiling.tiles[tile_idx];
            let entities = match self.world.tile_entities(tile.id.word()) {
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
                    Some(StructureKind::Splitter) => (8.0, false),
                    Some(StructureKind::Storage) => (9.0, false),
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
                } else if machine_type_float == 8.0 {
                    // Splitter: encode connection bitmask in progress field
                    self.splitter_pool.connection_bitmask(entity, &self.world) as f32
                } else if machine_type_float == 9.0 {
                    // Storage: encode fill fraction (0.0-1.0) in progress field
                    self.storage_pool.fill_fraction(entity)
                } else {
                    -1.0 // Power nodes are always "idle" visually
                };

                let power_sat = self.power_network.satisfaction(entity).unwrap_or(-1.0);
                let facing = self.world.direction(entity).unwrap_or(Direction::North);
                let facing_float = facing.rotations_from_north() as f32;

                let inst = MachineInstance {
                    mobius_a: [combined.a.re as f32, combined.a.im as f32],
                    mobius_b: [combined.b.re as f32, combined.b.im as f32],
                    grid_pos: [gx as f32, gy as f32],
                    machine_type: machine_type_float,
                    progress,
                    power_sat,
                    facing: facing_float,
                };
                re.machine_instances.push(inst);
                re.topper_instances.push(inst);
            }
        }
        re.machine_instances.upload(&re.gpu.device, &re.gpu.queue);
        re.topper_instances.upload(&re.gpu.device, &re.gpu.queue);

        // Build item instances from items riding on visible belts
        re.item_instances.clear();
        let khs = self.klein_half_side;
        let divisions = 64.0;
        for &(tile_idx, combined) in &visible {
            let tile = &re.tiling.tiles[tile_idx];
            let entities = match self.world.tile_entities(tile.id.word()) {
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

        // Build ghost preview instances for placement mode or paste mode
        re.ghost_instances.clear();
        if let Some(cursor) = self.ui.cursor_pos {
            let width = re.gpu.config.width as f32;
            let height = re.gpu.config.height as f32;
            let khs = self.klein_half_side;
            if let Some(click_disk) = render_camera.unproject_to_disk(cursor.x, cursor.y, width, height) {
                // Find containing tile (inline find_clicked_tile logic using render_camera)
                let mut best_tile: Option<(usize, f64, f64)> = None;
                let mut best_max_norm = f64::MAX;
                for &(i, combined) in &visible {
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
                        best_tile = Some((i, norm_x, norm_y));
                    }
                }
                if let Some((tile_vis_idx, norm_x, norm_y)) = best_tile {
                    let divisions = 64.0_f64;
                    let gx = (norm_x * divisions).round() as i32;
                    let gy = (norm_y * divisions).round() as i32;
                    let gx = gx.clamp(-32, 32);
                    let gy = gy.clamp(-32, 32);

                    let combined = visible.iter()
                        .find(|&&(idx, _)| idx == tile_vis_idx)
                        .map(|&(_, m)| m)
                        .unwrap();

                    let ma = [combined.a.re as f32, combined.a.im as f32];
                    let mb = [combined.b.re as f32, combined.b.im as f32];

                    if self.ui.paste_mode {
                        // Paste mode: ghost preview for all clipboard entries
                        if let Some(clip) = &self.clipboard {
                            // Detect cross-tile paste even for single-tile clipboards
                            let max_px = gx + clip.width - 1;
                            let max_py = gy + clip.height - 1;
                            let crosses_x = max_px > 32 || gx < -32;
                            let crosses_y = max_py > 32 || gy < -32;
                            let multi_tile_clip = clip.width > 64 || clip.height > 64 || crosses_x || crosses_y;

                            if multi_tile_clip {
                                // Multi-tile ghost: build Mobius map per tile_delta
                                let strip_axis_x = if clip.width > 64 { true }
                                    else if clip.height > 64 { false }
                                    else { crosses_x };
                                let mut tile_mobius: std::collections::HashMap<i32, ([f32; 2], [f32; 2])> =
                                    std::collections::HashMap::new();
                                tile_mobius.insert(0, (ma, mb));

                                // Pre-walk strip to find Mobius for each needed delta
                                for entry in &clip.entries {
                                    let (offset_x, offset_y) = (gx + entry.offset.0, gy + entry.offset.1);
                                    let (td, _) = if strip_axis_x {
                                        blueprint::virtual_to_tile_local(offset_x)
                                    } else {
                                        blueprint::virtual_to_tile_local(offset_y)
                                    };
                                    if tile_mobius.contains_key(&td) {
                                        continue;
                                    }
                                    // Walk from anchor tile
                                    let edge = if strip_axis_x {
                                        if td > 0 { 0u8 } else { 2u8 }
                                    } else if td > 0 {
                                        1u8
                                    } else {
                                        3u8
                                    };
                                    let mut cur = tile_vis_idx;
                                    let mut found = true;
                                    for _ in 0..td.unsigned_abs() {
                                        if let Some(nid) = re.tiling.neighbor_tile_id(cur, edge) {
                                            if let Some(nidx) = re.tiling.find_tile(&nid) {
                                                cur = nidx;
                                            } else {
                                                found = false;
                                                break;
                                            }
                                        } else {
                                            found = false;
                                            break;
                                        }
                                    }
                                    if found {
                                        if let Some(&(_, m)) = visible.iter().find(|&&(idx, _)| idx == cur) {
                                            tile_mobius.insert(td, (
                                                [m.a.re as f32, m.a.im as f32],
                                                [m.b.re as f32, m.b.im as f32],
                                            ));
                                        }
                                    }
                                }

                                // Emit ghost instances per entry
                                for entry in &clip.entries {
                                    let (offset_x, offset_y) = (gx + entry.offset.0, gy + entry.offset.1);
                                    let (td, local_strip) = if strip_axis_x {
                                        blueprint::virtual_to_tile_local(offset_x)
                                    } else {
                                        blueprint::virtual_to_tile_local(offset_y)
                                    };
                                    let local_xy = if strip_axis_x {
                                        (local_strip, offset_y)
                                    } else {
                                        (offset_x, local_strip)
                                    };
                                    if let Some(&(entry_ma, entry_mb)) = tile_mobius.get(&td) {
                                        re.ghost_instances.push(MachineInstance {
                                            mobius_a: entry_ma,
                                            mobius_b: entry_mb,
                                            grid_pos: [local_xy.0 as f32, local_xy.1 as f32],
                                            machine_type: kind_to_machine_type_float(entry.kind),
                                            progress: -1.0,
                                            power_sat: -1.0,
                                            facing: entry.direction.rotations_from_north() as f32,
                                        });
                                    }
                                    // Skip entries whose tile isn't visible
                                }
                            } else {
                                // Single-tile paste ghost preview
                                let tile = &re.tiling.tiles[tile_vis_idx];
                                let checks = blueprint::can_paste(
                                    &self.world, tile.id.word(), (gx, gy), clip,
                                );
                                for (i, entry) in clip.entries.iter().enumerate() {
                                    let egx = gx + entry.offset.0;
                                    let egy = gy + entry.offset.1;
                                    let blocked = !checks.get(i).is_none_or(|&(_, ok)| ok);
                                    re.ghost_instances.push(MachineInstance {
                                        mobius_a: ma,
                                        mobius_b: mb,
                                        grid_pos: [egx as f32, egy as f32],
                                        machine_type: kind_to_machine_type_float(entry.kind),
                                        progress: if blocked { -3.0 } else { -1.0 },
                                        power_sat: -1.0,
                                        facing: entry.direction.rotations_from_north() as f32,
                                    });
                                }
                            }
                        }
                    } else if let Some(mode) = &self.ui.placement_mode {
                        // Single-structure ghost preview
                        let sk = StructureKind::from_item(mode.item);
                        re.ghost_instances.push(MachineInstance {
                            mobius_a: ma,
                            mobius_b: mb,
                            grid_pos: [gx as f32, gy as f32],
                            machine_type: sk.map_or(10.0, kind_to_machine_type_float),
                            progress: -1.0,
                            power_sat: -1.0,
                            facing: mode.direction.rotations_from_north() as f32,
                        });
                    }
                }
            }
        }
        re.ghost_instances.upload(&re.gpu.device, &re.gpu.queue);

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
                        let text = format_cell_id(&tile.id);
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
        if let Some(action) = crate::ui::settings::settings_menu(
            &re.egui.ctx.clone(),
            &mut self.ui.settings_open,
            &mut self.config,
            &mut self.input_state,
            &mut self.ui.rebinding,
        ) {
            self.ui.save_action = Some(action);
        }

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

        // Splitter inspection panel
        if let Some(entity) = self.ui.splitter_panel_entity {
            let egui_ctx = re.egui.ctx.clone();
            if let Some(action) = crate::ui::splitter::splitter_panel(
                &egui_ctx,
                entity,
                &self.splitter_pool,
                &self.world,
            ) {
                match action {
                    crate::ui::splitter::SplitterAction::Close => {
                        self.ui.splitter_panel_entity = None;
                    }
                }
            }
        }

        // Storage inspection panel
        if let Some(entity) = self.ui.storage_panel_entity {
            let egui_ctx = re.egui.ctx.clone();
            if let Some(action) = crate::ui::storage::storage_panel(
                &egui_ctx,
                entity,
                &self.storage_pool,
                &self.belt_network,
            ) {
                match action {
                    crate::ui::storage::StorageAction::Close => {
                        self.ui.storage_panel_entity = None;
                    }
                }
            }
        }

        // Blueprint manager window
        {
            let egui_ctx = re.egui.ctx.clone();
            if let Some(action) = crate::ui::blueprint::blueprint_manager(
                &egui_ctx,
                &mut self.ui.blueprint_manager_open,
                &mut self.ui.blueprint_manager_state,
                self.clipboard.as_ref(),
                self.cfg.q,
                &re.icon_atlas,
            ) {
                use crate::ui::blueprint::BlueprintAction;
                match action {
                    BlueprintAction::LoadToClipboard(path) => {
                        match blueprint::load_blueprint(&path, self.cfg.q) {
                            Ok(file) => {
                                self.clipboard = Some(file.to_clipboard());
                                self.ui.flash_label = format!("Loaded blueprint '{}'", file.name);
                                self.ui.flash_timer = 2.0;
                                self.ui.flash_screen_pos = None;
                                self.ui.blueprint_manager_state.invalidate();
                            }
                            Err(e) => {
                                self.ui.flash_label = format!("Load failed: {e}");
                                self.ui.flash_timer = 3.0;
                                self.ui.flash_screen_pos = None;
                            }
                        }
                    }
                    BlueprintAction::SaveClipboard(name) => {
                        if let Some(clip) = &self.clipboard {
                            let file = BlueprintFile::from_clipboard(clip, name.clone());
                            match blueprint::save_blueprint(&file) {
                                Ok(_path) => {
                                    self.ui.flash_label = format!("Saved blueprint '{name}'");
                                    self.ui.flash_timer = 2.0;
                                    self.ui.flash_screen_pos = None;
                                    self.ui.blueprint_manager_state.invalidate();
                                }
                                Err(e) => {
                                    self.ui.flash_label = format!("Save failed: {e}");
                                    self.ui.flash_timer = 3.0;
                                    self.ui.flash_screen_pos = None;
                                }
                            }
                        }
                    }
                    BlueprintAction::Rename(path, new_name) => {
                        match blueprint::rename_blueprint(&path, &new_name) {
                            Ok(_new_path) => {
                                self.ui.flash_label = format!("Renamed to '{new_name}'");
                                self.ui.flash_timer = 2.0;
                                self.ui.flash_screen_pos = None;
                                self.ui.blueprint_manager_state.invalidate();
                            }
                            Err(e) => {
                                self.ui.flash_label = format!("Rename failed: {e}");
                                self.ui.flash_timer = 3.0;
                                self.ui.flash_screen_pos = None;
                            }
                        }
                    }
                    BlueprintAction::Delete(path) => {
                        match blueprint::delete_blueprint(&path) {
                            Ok(()) => {
                                self.ui.flash_label = "Blueprint deleted".to_string();
                                self.ui.flash_timer = 2.0;
                                self.ui.flash_screen_pos = None;
                                self.ui.blueprint_manager_state.invalidate();
                            }
                            Err(e) => {
                                self.ui.flash_label = format!("Delete failed: {e}");
                                self.ui.flash_timer = 3.0;
                                self.ui.flash_screen_pos = None;
                            }
                        }
                    }
                    BlueprintAction::Close => {
                        self.ui.blueprint_manager_open = false;
                    }
                }
            }
        }

        // Blueprint selection rectangle overlay
        if let Some(sel) = &self.ui.selection {
            let khs = self.klein_half_side;
            let divisions = 64.0_f64;
            let color = if sel.finalized {
                egui::Color32::from_rgba_unmultiplied(60, 180, 255, 60)
            } else {
                egui::Color32::from_rgba_unmultiplied(60, 180, 255, 40)
            };
            let border_color = egui::Color32::from_rgba_unmultiplied(60, 180, 255, 140);

            // For each tile in the strip, compute the local sub-rectangle and draw
            for strip_tile in &sel.tiles {
                let combined = match visible.iter().find(|&&(idx, _)| idx == strip_tile.tile_idx) {
                    Some(&(_, m)) => m,
                    None => continue,
                };

                // Compute local grid bounds for this tile
                let (local_min_gx, local_max_gx, local_min_gy, local_max_gy) = if sel.tiles.len() == 1 {
                    // Single-tile: use raw coords
                    let min_gx = sel.start.0.min(sel.current.0);
                    let max_gx = sel.start.0.max(sel.current.0);
                    let min_gy = sel.start.1.min(sel.current.1);
                    let max_gy = sel.start.1.max(sel.current.1);
                    (min_gx, max_gx, min_gy, max_gy)
                } else {
                    // Multi-tile: decompose virtual coords for this tile's delta
                    let delta = strip_tile.delta;
                    if sel.strip_axis_x {
                        let vmin_x = sel.start.0.min(sel.current.0);
                        let vmax_x = sel.start.0.max(sel.current.0);
                        // Clip to this tile's virtual range
                        let tile_vmin = blueprint::tile_local_to_virtual(delta, -32);
                        let tile_vmax = blueprint::tile_local_to_virtual(delta, 32);
                        let clipped_vmin = vmin_x.max(tile_vmin);
                        let clipped_vmax = vmax_x.min(tile_vmax);
                        if clipped_vmin > clipped_vmax { continue; }
                        let lmin = clipped_vmin - delta * 64;
                        let lmax = clipped_vmax - delta * 64;
                        let min_gy = sel.start.1.min(sel.current.1);
                        let max_gy = sel.start.1.max(sel.current.1);
                        (lmin, lmax, min_gy, max_gy)
                    } else {
                        let vmin_y = sel.start.1.min(sel.current.1);
                        let vmax_y = sel.start.1.max(sel.current.1);
                        let tile_vmin = blueprint::tile_local_to_virtual(delta, -32);
                        let tile_vmax = blueprint::tile_local_to_virtual(delta, 32);
                        let clipped_vmin = vmin_y.max(tile_vmin);
                        let clipped_vmax = vmax_y.min(tile_vmax);
                        if clipped_vmin > clipped_vmax { continue; }
                        let lmin = clipped_vmin - delta * 64;
                        let lmax = clipped_vmax - delta * 64;
                        let min_gx = sel.start.0.min(sel.current.0);
                        let max_gx = sel.start.0.max(sel.current.0);
                        (min_gx, max_gx, lmin, lmax)
                    }
                };

                let corners = [
                    (local_min_gx as f64 - 0.5, local_min_gy as f64 - 0.5),
                    (local_max_gx as f64 + 0.5, local_min_gy as f64 - 0.5),
                    (local_max_gx as f64 + 0.5, local_max_gy as f64 + 0.5),
                    (local_min_gx as f64 - 0.5, local_max_gy as f64 + 0.5),
                ];
                let mut screen_pts = Vec::new();
                for &(cx, cy) in &corners {
                    let snap_kx = (cx / divisions) * 2.0 * khs;
                    let snap_ky = (cy / divisions) * 2.0 * khs;
                    let kr2 = snap_kx * snap_kx + snap_ky * snap_ky;
                    let denom = 1.0 + (1.0 - kr2).max(0.0).sqrt();
                    let local_disk = Complex::new(snap_kx / denom, snap_ky / denom);
                    let world_disk = combined.apply(local_disk);
                    let bowl = crate::hyperbolic::embedding::disk_to_bowl(world_disk);
                    let elevation = re.extra_elevation.get(&strip_tile.tile_idx).copied().unwrap_or(0.0);
                    let world_pos = glam::Vec3::new(bowl[0], bowl[1] + elevation, bowl[2]);
                    if let Some((sx, sy)) = project_to_screen(world_pos, &view_proj, width, height) {
                        screen_pts.push(egui::pos2(sx / scale, sy / scale));
                    }
                }
                if screen_pts.len() == 4 {
                    let egui_ctx = re.egui.ctx.clone();
                    let layer_id = egui::LayerId::new(egui::Order::Background, egui::Id::new("blueprint_selection"));
                    let painter = egui_ctx.layer_painter(layer_id);
                    // Draw filled quad using a mesh (proper hyperbolic shape)
                    let mut mesh = egui::Mesh::default();
                    for &pt in &screen_pts {
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: pt,
                            uv: egui::epaint::WHITE_UV,
                            color,
                        });
                    }
                    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                    painter.add(egui::Shape::mesh(mesh));
                    // Draw border as a closed polyline
                    let mut border_pts = screen_pts.clone();
                    border_pts.push(screen_pts[0]);
                    painter.add(egui::Shape::line(border_pts, egui::Stroke::new(1.5, border_color)));
                }
            }
        }

        // Blueprint select mode indicator
        if self.ui.blueprint_select {
            let egui_ctx = re.egui.ctx.clone();
            egui::Area::new(egui::Id::new("blueprint_mode_indicator"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(8.0, 26.0))
                .interactable(false)
                .show(&egui_ctx, |ui| {
                    ui.label(
                        egui::RichText::new("SELECT MODE (B to exit, drag to select, Ctrl+C copy, Ctrl+X cut)")
                            .color(egui::Color32::from_rgb(60, 180, 255))
                            .size(13.0)
                            .font(egui::FontId::monospace(13.0)),
                    );
                });
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
                    let tile_count = re.tiling.tiles.len();
                    ui.label(
                        egui::RichText::new(format!(
                            "FPS {fps:.0}  UPS {ups:.0}  Tiles {tile_count}"
                        ))
                        .color(egui::Color32::from_rgb(180, 220, 180))
                        .size(13.0)
                        .font(egui::FontId::monospace(13.0)),
                    );
                });
        }

        let full_output = re.egui.end_frame(&window);
        self.ui.egui_wants_keyboard = re.egui.ctx.wants_keyboard_input();

        // GPU render passes + submit
        let output = re.draw_and_submit(&full_output)?;
        output.present();
        Ok(())
    }

    /// Serialize and save the current game state to the autosave slot.
    fn save_game(&mut self) {
        let Some(re) = &self.renderer else { return };
        let tile_id = re.tiling.tiles[self.camera.tile].id.clone();
        match crate::game::save::serialize_save(
            self.cfg,
            &self.camera,
            &tile_id,
            self.game_loop.sim_tick,
            self.grid_enabled,
            &self.inventory,
            &self.world,
            &self.belt_network,
            &self.machine_pool,
            &self.splitter_pool,
            &self.storage_pool,
            &self.power_network,
        ) {
            Ok(bytes) => {
                if let Err(e) = crate::game::save::autosave(&bytes) {
                    log::error!("Autosave failed: {e}");
                }
            }
            Err(e) => log::error!("Save serialize failed: {e}"),
        }
    }

    /// Serialize and save to a named slot.
    fn save_game_named(&mut self, name: &str) {
        let Some(re) = &self.renderer else { return };
        let tile_id = re.tiling.tiles[self.camera.tile].id.clone();
        match crate::game::save::serialize_save(
            self.cfg,
            &self.camera,
            &tile_id,
            self.game_loop.sim_tick,
            self.grid_enabled,
            &self.inventory,
            &self.world,
            &self.belt_network,
            &self.machine_pool,
            &self.splitter_pool,
            &self.storage_pool,
            &self.power_network,
        ) {
            Ok(bytes) => {
                if let Err(e) = crate::game::save::save_named(&bytes, name) {
                    log::error!("Named save failed: {e}");
                }
            }
            Err(e) => log::error!("Save serialize failed: {e}"),
        }
    }

    /// Restore game state from a RestoredState. Called after loading.
    /// Returns an error message if the save is incompatible.
    fn apply_restored_state(&mut self, state: crate::game::save::RestoredState) -> Result<(), String> {
        // Check tiling config compatibility
        if state.cfg.p != self.cfg.p || state.cfg.q != self.cfg.q {
            return Err(format!(
                "Save is for {{{},{}}}, but current game is {{{},{}}}",
                state.cfg.p, state.cfg.q, self.cfg.p, self.cfg.q
            ));
        }

        self.inventory = state.inventory;
        self.world = state.world;
        self.belt_network = state.belt_network;
        self.machine_pool = state.machine_pool;
        self.splitter_pool = state.splitter_pool;
        self.storage_pool = state.storage_pool;
        self.power_network = state.power_network;
        self.power_network.mark_dirty();
        self.grid_enabled = state.grid_enabled;
        self.game_loop.sim_tick = state.sim_tick;

        // Reconstruct tiling centered on the saved camera tile
        if let Some(re) = &mut self.renderer {
            use crate::hyperbolic::tiling::TilingState;

            // Create tiling directly centered on the saved tile —
            // avoids the impossible task of expanding from origin to a distant cell.
            let mut tiling = TilingState::new_centered_on(self.cfg, &state.camera_tile_id);

            // The center tile is at index 0
            self.camera.tile = 0;
            self.camera.local = state.camera_local;
            self.camera.heading = state.camera_heading;
            self.camera.height = state.camera_height;
            self.camera.mode = state.camera_mode;

            // Expand tiles around the camera position
            let cam_pos = self.camera.local.apply(Complex::ZERO);
            tiling.ensure_coverage(cam_pos, 3);

            re.tiling = tiling;
        }
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
                // Track modifier state before rebinding check
                if code == winit::keyboard::KeyCode::ShiftLeft || code == winit::keyboard::KeyCode::ShiftRight {
                    self.input_state.shift_held = event.state.is_pressed();
                }
                if code == winit::keyboard::KeyCode::ControlLeft || code == winit::keyboard::KeyCode::ControlRight
                    || code == winit::keyboard::KeyCode::SuperLeft || code == winit::keyboard::KeyCode::SuperRight
                {
                    self.input_state.ctrl_held = event.state.is_pressed();
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

                // When egui has keyboard focus (text field etc.), only allow
                // Escape to close dialogs — skip all other game hotkeys.
                if self.ui.egui_wants_keyboard {
                    // Clear held game actions so keys don't get "stuck"
                    self.input_state.clear_active();
                    if event.state.is_pressed() && code == winit::keyboard::KeyCode::Escape {
                        self.ui.settings_open = false;
                        self.ui.inventory_open = false;
                        self.ui.blueprint_manager_open = false;
                        self.ui.machine_panel_entity = None;
                        self.ui.splitter_panel_entity = None;
                        self.ui.storage_panel_entity = None;
                    }
                    // Still forward to egui below, but don't process game actions
                    // (fall through to egui event handling at the end of this function)
                } else {
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
                    if self.input_state.just_pressed(GameAction::DestroyBuilding)
                        && !self.input_state.ctrl_held
                    {
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
                    if self.input_state.just_pressed(GameAction::QuickSave) {
                        self.save_game();
                        self.ui.flash_label = "Game saved".to_string();
                        self.ui.flash_timer = 2.0;
                        self.ui.flash_screen_pos = None;
                    }
                    // B key: toggle blueprint box-select mode
                    if code == winit::keyboard::KeyCode::KeyB
                        && !self.input_state.ctrl_held
                        && !self.input_state.shift_held
                    {
                        self.ui.blueprint_select = !self.ui.blueprint_select;
                        if !self.ui.blueprint_select {
                            self.ui.selection = None;
                        } else {
                            // Entering selection mode — clear placement mode
                            self.ui.placement_mode = None;
                        }
                        log::info!(
                            "blueprint select: {}",
                            if self.ui.blueprint_select { "ON" } else { "OFF" }
                        );
                    }

                    // N key: toggle blueprint manager window
                    if code == winit::keyboard::KeyCode::KeyN
                        && !self.input_state.ctrl_held
                        && !self.input_state.shift_held
                    {
                        self.ui.blueprint_manager_open = !self.ui.blueprint_manager_open;
                        if self.ui.blueprint_manager_open {
                            self.ui.blueprint_manager_state.invalidate();
                        }
                    }

                    // Ctrl+C: copy selected structures to clipboard
                    if self.input_state.ctrl_held && code == winit::keyboard::KeyCode::KeyC {
                        self.blueprint_copy();
                    }

                    // Ctrl+X: cut selected structures to clipboard
                    if self.input_state.ctrl_held && code == winit::keyboard::KeyCode::KeyX {
                        self.blueprint_cut();
                    }

                    // Ctrl+V: enter paste mode with clipboard contents
                    if self.input_state.ctrl_held
                        && code == winit::keyboard::KeyCode::KeyV
                        && self.clipboard.is_some()
                    {
                        self.ui.paste_mode = true;
                        self.ui.placement_mode = None;
                        self.ui.blueprint_select = false;
                        self.ui.selection = None;
                        log::info!("paste mode: ON");
                    }

                    // R key in paste mode: rotate clipboard 90° CW
                    if self.ui.paste_mode && code == winit::keyboard::KeyCode::KeyR {
                        if let Some(clip) = &mut self.clipboard {
                            clip.rotate_cw();
                            log::info!("clipboard rotated CW");
                        }
                    }

                    // Escape exits paste mode
                    if self.ui.paste_mode && code == winit::keyboard::KeyCode::Escape {
                        self.ui.paste_mode = false;
                        log::info!("paste mode: OFF");
                    }

                    if self.input_state.just_pressed(GameAction::QuickLoad) {
                        match crate::game::save::load_autosave() {
                            Ok(state) => {
                                match self.apply_restored_state(state) {
                                    Ok(()) => {
                                        self.ui.flash_label = "Game loaded".to_string();
                                        self.ui.flash_timer = 2.0;
                                        self.ui.flash_screen_pos = None;
                                    }
                                    Err(e) => {
                                        log::error!("Load failed: {e}");
                                        self.ui.flash_label = format!("Load failed: {e}");
                                        self.ui.flash_timer = 3.0;
                                        self.ui.flash_screen_pos = None;
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("No save to load: {e}");
                                self.ui.flash_label = "No save found".to_string();
                                self.ui.flash_timer = 2.0;
                                self.ui.flash_screen_pos = None;
                            }
                        }
                    }
                }
                } // else: egui does NOT want keyboard
            }
        }

        // Handle blueprint selection mouse events BEFORE egui so they aren't consumed
        if self.ui.blueprint_select {
            match &event {
                WindowEvent::CursorMoved { position, .. } => {
                    self.ui.cursor_pos = Some(*position);
                    if self.ui.selection.as_ref().is_some_and(|s| !s.finalized) {
                        self.update_box_selection(position.x, position.y);
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    if *button == winit::event::MouseButton::Left {
                        if *state == winit::event::ElementState::Pressed {
                            if let Some(pos) = self.ui.cursor_pos {
                                self.begin_box_selection(pos.x, pos.y);
                            }
                        } else {
                            self.finalize_box_selection();
                        }
                    }
                }
                _ => {}
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
                self.save_game();
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
                            } else if self.ui.paste_mode {
                                self.handle_paste_click(pos.x, pos.y);
                            } else if self.ui.placement_mode.is_some() {
                                self.handle_placement_click(pos.x, pos.y);
                            } else if !self.ui_is_open() {
                                if !self.try_open_machine_panel(pos.x, pos.y) {
                                    self.handle_debug_click(pos.x, pos.y);
                                }
                            } else if self.ui.machine_panel_entity.is_some() || self.ui.splitter_panel_entity.is_some() || self.ui.storage_panel_entity.is_some() {
                                // Clicking outside while inspection panel is open:
                                // try to click another building, else close panel
                                if !self.try_open_machine_panel(pos.x, pos.y) {
                                    self.ui.machine_panel_entity = None;
                                    self.ui.splitter_panel_entity = None;
                                    self.ui.storage_panel_entity = None;
                                }
                            }
                        }
                    } else {
                        // Mouse released — end belt drag
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

                // Process deferred save/load actions from UI
                if let Some(action) = self.ui.save_action.take() {
                    use crate::ui::settings::SettingsAction;
                    match action {
                        SettingsAction::Save => {
                            self.save_game();
                            self.ui.flash_label = "Game saved".to_string();
                            self.ui.flash_timer = 2.0;
                            self.ui.flash_screen_pos = None;
                        }
                        SettingsAction::SaveNamed(name) => {
                            self.save_game_named(&name);
                            self.ui.flash_label = format!("Saved as '{name}'");
                            self.ui.flash_timer = 2.0;
                            self.ui.flash_screen_pos = None;
                        }
                        SettingsAction::Load(path) => {
                            match crate::game::save::load_from_path(&path) {
                                Ok(state) => match self.apply_restored_state(state) {
                                    Ok(()) => {
                                        self.ui.flash_label = "Game loaded".to_string();
                                        self.ui.flash_timer = 2.0;
                                        self.ui.flash_screen_pos = None;
                                    }
                                    Err(e) => {
                                        self.ui.flash_label = format!("Load failed: {e}");
                                        self.ui.flash_timer = 3.0;
                                        self.ui.flash_screen_pos = None;
                                    }
                                },
                                Err(e) => {
                                    self.ui.flash_label = format!("Load failed: {e}");
                                    self.ui.flash_timer = 3.0;
                                    self.ui.flash_screen_pos = None;
                                }
                            }
                        }
                        SettingsAction::DeleteSave(name) => {
                            match crate::game::save::delete_save(&name) {
                                Ok(()) => {
                                    self.ui.flash_label = format!("Deleted '{name}'");
                                    self.ui.flash_timer = 2.0;
                                    self.ui.flash_screen_pos = None;
                                }
                                Err(e) => {
                                    self.ui.flash_label = format!("Delete failed: {e}");
                                    self.ui.flash_timer = 3.0;
                                    self.ui.flash_screen_pos = None;
                                }
                            }
                        }
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
            self.splitter_pool.tick(&mut self.belt_network);
            self.belt_network.tick_port_transfers(&mut self.machine_pool, &mut self.storage_pool);
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
