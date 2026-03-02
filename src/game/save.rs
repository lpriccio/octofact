use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::game::inventory::Inventory;
use crate::game::world::WorldState;
use crate::hyperbolic::cell_id::CellId;
use crate::hyperbolic::poincare::{Mobius, TilingConfig};
use crate::render::camera::{Camera, CameraMode};
use crate::sim::belt::BeltNetwork;
use crate::sim::machine::MachinePool;
use crate::sim::power::PowerNetwork;
use crate::sim::splitter::SplitterPool;
use crate::sim::storage::StoragePool;

/// Current save format version. Bump when the format changes.
const SAVE_VERSION: u32 = 1;

/// Camera state in a serializable form (tile stored as CellId, not index).
#[derive(Serialize, Deserialize)]
struct CameraSave {
    tile_id: CellId,
    local: Mobius,
    heading: f64,
    height: f32,
    mode: CameraMode,
}

/// Borrowed view of game state for serialization.
#[derive(Serialize)]
struct SaveView<'a> {
    version: u32,
    timestamp: u64,
    tiling_config: TilingConfig,
    camera: CameraSave,
    sim_tick: u64,
    grid_enabled: bool,
    inventory: &'a Inventory,
    world: &'a WorldState,
    belt_network: &'a BeltNetwork,
    machine_pool: &'a MachinePool,
    splitter_pool: &'a SplitterPool,
    storage_pool: &'a StoragePool,
    power_network: &'a PowerNetwork,
}

/// Owned save data for deserialization.
#[derive(Deserialize)]
struct SaveData {
    version: u32,
    #[allow(dead_code)]
    timestamp: u64,
    tiling_config: TilingConfig,
    camera: CameraSave,
    sim_tick: u64,
    grid_enabled: bool,
    inventory: Inventory,
    world: WorldState,
    belt_network: BeltNetwork,
    machine_pool: MachinePool,
    splitter_pool: SplitterPool,
    storage_pool: StoragePool,
    power_network: PowerNetwork,
}

/// Returns the saves directory.
fn saves_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "octofact")
        .map(|dirs| dirs.data_dir().join("saves"))
}

/// Ensure the saves directory exists and return it.
fn ensure_saves_dir() -> Option<PathBuf> {
    let dir = saves_dir()?;
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Path for the auto-save slot.
fn autosave_path() -> Option<PathBuf> {
    ensure_saves_dir().map(|d| d.join("autosave.bin"))
}

/// Path for a named save.
fn named_save_path(name: &str) -> Option<PathBuf> {
    ensure_saves_dir().map(|d| d.join(format!("{name}.bin")))
}

/// List all available saves, returning (display_name, path) pairs.
pub fn list_saves() -> Vec<(String, PathBuf)> {
    let Some(dir) = saves_dir() else {
        return vec![];
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return vec![];
    };
    let mut saves: Vec<(String, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "bin")
        })
        .map(|e| {
            let path = e.path();
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (name, path)
        })
        .collect();
    saves.sort_by(|a, b| a.0.cmp(&b.0));
    saves
}

/// Serialize game state to bytes.
#[allow(clippy::too_many_arguments)]
pub fn serialize_save(
    cfg: TilingConfig,
    camera: &Camera,
    camera_tile_id: &CellId,
    sim_tick: u64,
    grid_enabled: bool,
    inventory: &Inventory,
    world: &WorldState,
    belt_network: &BeltNetwork,
    machine_pool: &MachinePool,
    splitter_pool: &SplitterPool,
    storage_pool: &StoragePool,
    power_network: &PowerNetwork,
) -> Result<Vec<u8>, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let view = SaveView {
        version: SAVE_VERSION,
        timestamp,
        tiling_config: cfg,
        camera: CameraSave {
            tile_id: camera_tile_id.clone(),
            local: camera.local,
            heading: camera.heading,
            height: camera.height,
            mode: camera.mode,
        },
        sim_tick,
        grid_enabled,
        inventory,
        world,
        belt_network,
        machine_pool,
        splitter_pool,
        storage_pool,
        power_network,
    };

    bincode::serialize(&view).map_err(|e| format!("serialize: {e}"))
}

/// Write bytes to a save file atomically.
fn write_save(bytes: &[u8], path: &PathBuf) -> Result<(), String> {
    let tmp = path.with_extension("bin.tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("write: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename: {e}"))?;
    log::info!("Saved {} bytes to {}", bytes.len(), path.display());
    Ok(())
}

/// Auto-save the game state.
pub fn autosave(bytes: &[u8]) -> Result<(), String> {
    let path = autosave_path().ok_or("Could not determine save directory")?;
    write_save(bytes, &path)
}

/// Save to a named slot.
pub fn save_named(bytes: &[u8], name: &str) -> Result<(), String> {
    let path = named_save_path(name).ok_or("Could not determine save directory")?;
    write_save(bytes, &path)
}

/// Delete a named save file.
pub fn delete_save(name: &str) -> Result<(), String> {
    let path = named_save_path(name).ok_or("Could not determine save directory")?;
    fs::remove_file(&path).map_err(|e| format!("delete: {e}"))
}

/// Restored game state from a save file.
pub struct RestoredState {
    pub cfg: TilingConfig,
    pub camera_tile_id: CellId,
    pub camera_local: Mobius,
    pub camera_heading: f64,
    pub camera_height: f32,
    pub camera_mode: CameraMode,
    pub sim_tick: u64,
    pub grid_enabled: bool,
    pub inventory: Inventory,
    pub world: WorldState,
    pub belt_network: BeltNetwork,
    pub machine_pool: MachinePool,
    pub splitter_pool: SplitterPool,
    pub storage_pool: StoragePool,
    pub power_network: PowerNetwork,
}

/// Load and deserialize a save from bytes.
fn deserialize_save(bytes: &[u8]) -> Result<RestoredState, String> {
    let data: SaveData =
        bincode::deserialize(bytes).map_err(|e| format!("deserialize: {e}"))?;
    if data.version > SAVE_VERSION {
        return Err(format!(
            "Save version {} is newer than supported version {SAVE_VERSION}",
            data.version
        ));
    }
    Ok(RestoredState {
        cfg: data.tiling_config,
        camera_tile_id: data.camera.tile_id,
        camera_local: data.camera.local,
        camera_heading: data.camera.heading,
        camera_height: data.camera.height,
        camera_mode: data.camera.mode,
        sim_tick: data.sim_tick,
        grid_enabled: data.grid_enabled,
        inventory: data.inventory,
        world: data.world,
        belt_network: data.belt_network,
        machine_pool: data.machine_pool,
        splitter_pool: data.splitter_pool,
        storage_pool: data.storage_pool,
        power_network: data.power_network,
    })
}

/// Load from the auto-save slot.
pub fn load_autosave() -> Result<RestoredState, String> {
    let path = autosave_path().ok_or("Could not determine save directory")?;
    if !path.exists() {
        return Err("No autosave found".into());
    }
    let bytes = fs::read(&path).map_err(|e| format!("read: {e}"))?;
    log::info!("Loaded {} bytes from {}", bytes.len(), path.display());
    deserialize_save(&bytes)
}

/// Load from a named save.
#[allow(dead_code)]
pub fn load_named(name: &str) -> Result<RestoredState, String> {
    let path = named_save_path(name).ok_or("Could not determine save directory")?;
    if !path.exists() {
        return Err(format!("Save '{name}' not found"));
    }
    let bytes = fs::read(&path).map_err(|e| format!("read: {e}"))?;
    log::info!("Loaded {} bytes from {}", bytes.len(), path.display());
    deserialize_save(&bytes)
}

/// Load from a specific file path.
pub fn load_from_path(path: &PathBuf) -> Result<RestoredState, String> {
    let bytes = fs::read(path).map_err(|e| format!("read: {e}"))?;
    log::info!("Loaded {} bytes from {}", bytes.len(), path.display());
    deserialize_save(&bytes)
}

/// Check if an autosave exists.
#[allow(dead_code)]
pub fn has_autosave() -> bool {
    autosave_path().is_some_and(|p| p.exists())
}
