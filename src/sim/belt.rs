use slotmap::{new_key_type, SlotMap, SecondaryMap};

use crate::game::items::ItemId;
use crate::game::world::{Direction, EntityId, StructureKind, WorldState};
use crate::sim::machine::MachinePool;

new_key_type! {
    /// Identifies a transport line in the belt network.
    pub struct TransportLineId;
}

/// Fixed-point scale: 1 grid square = 256 units.
pub const FP_SCALE: u32 = 256;

/// Default belt speed in fixed-point units per tick.
/// At 60 UPS: 4/256 × 60 ≈ 0.94 grid squares per second.
pub const DEFAULT_BELT_SPEED: u16 = 4;

/// Minimum gap between adjacent items (fixed-point units).
/// 64 = 1/4 grid square → max 4 items per grid square.
pub const MIN_ITEM_GAP: u32 = 64;

/// What's connected at one end of a transport line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeltEnd {
    /// Nothing connected — items stop here.
    Open,
    /// Connected to another transport line.
    Belt(TransportLineId),
    /// Belt output feeds into a machine's input port.
    /// `entity` is the machine's EntityId, `slot` is the input slot index.
    MachineInput { entity: EntityId, slot: usize },
    /// Machine output port feeds into belt input.
    /// `entity` is the machine's EntityId, `slot` is the output slot index.
    MachineOutput { entity: EntityId, slot: usize },
    /// Belt output side-injects onto a perpendicular belt.
    /// Items insert at the target entity's segment center rather than the line's input end.
    SideInject { entity: EntityId },
    /// Belt endpoint connected to a splitter.
    /// When on output_end: belt feeds items into the splitter (splitter input).
    /// When on input_end: splitter feeds items into the belt (splitter output).
    Splitter { entity: EntityId },
    /// Belt output feeds into a storage building's input port.
    StorageInput { entity: EntityId, slot: usize },
    /// Storage output port feeds into belt input.
    StorageOutput { entity: EntityId, slot: usize },
}

/// An item riding on a transport line.
#[derive(Clone, Debug)]
pub struct BeltItem {
    pub item: ItemId,
    /// Fixed-point distance from the output end.
    /// 0 = at the output, length = at the input.
    pub pos: u32,
}

/// A single transport line — possibly spanning multiple consecutive belt segments.
/// Items flow from input_end (pos = length) toward output_end (pos = 0).
pub struct TransportLine {
    /// Items on the line, ordered front (output) to back (input).
    pub items: Vec<BeltItem>,
    /// Movement speed in fixed-point units per tick.
    pub speed: u16,
    /// Total length in fixed-point units.
    pub length: u32,
    /// What feeds items into this line.
    pub input_end: BeltEnd,
    /// Where items exit this line.
    pub output_end: BeltEnd,
}

impl TransportLine {
    pub fn new(length: u32) -> Self {
        Self {
            items: Vec::new(),
            speed: DEFAULT_BELT_SPEED,
            length,
            input_end: BeltEnd::Open,
            output_end: BeltEnd::Open,
        }
    }

    /// Check whether the input end has room for another item.
    pub fn can_accept_at_input(&self) -> bool {
        match self.items.last() {
            None => true,
            Some(last) => self.length.saturating_sub(last.pos) >= MIN_ITEM_GAP,
        }
    }

    /// Insert an item at the input end of this line.
    pub fn insert_at_input(&mut self, item: ItemId) {
        self.items.push(BeltItem { item, pos: self.length });
    }

    /// Check whether there is room at a specific offset for a side-injected item.
    /// Requires MIN_ITEM_GAP clearance from nearest items on both sides.
    pub fn can_accept_at_offset(&self, offset: u32) -> bool {
        let idx = self.items.partition_point(|i| i.pos < offset);
        // Check gap from item before (closer to output, lower pos)
        if idx > 0 {
            let before = self.items[idx - 1].pos;
            if offset.saturating_sub(before) < MIN_ITEM_GAP {
                return false;
            }
        }
        // Check gap from item after (closer to input, higher pos)
        if idx < self.items.len() {
            let after = self.items[idx].pos;
            if after.saturating_sub(offset) < MIN_ITEM_GAP {
                return false;
            }
        }
        true
    }

    /// Insert an item at a specific offset, maintaining sorted order.
    pub fn insert_at_offset(&mut self, item: ItemId, offset: u32) {
        let idx = self.items.partition_point(|i| i.pos < offset);
        self.items.insert(idx, BeltItem { item, pos: offset });
    }

    /// Advance all items toward the output by `speed` units.
    /// Items compress against each other (min gap enforced).
    fn advance(&mut self) {
        let speed = self.speed as u32;
        for i in 0..self.items.len() {
            let new_pos = self.items[i].pos.saturating_sub(speed);
            let min = if i == 0 {
                0
            } else {
                self.items[i - 1].pos.saturating_add(MIN_ITEM_GAP)
            };
            self.items[i].pos = new_pos.max(min);
        }
    }
}

/// Tracks a belt entity's position within a (possibly merged) transport line.
#[derive(Clone, Copy, Debug)]
pub struct BeltSegment {
    /// Which transport line this entity belongs to.
    pub line: TransportLineId,
    /// Offset of this segment's output end within the line (fixed-point).
    /// Items in range [offset, offset + FP_SCALE) belong to this segment.
    pub offset: u32,
}

/// The belt simulation network — manages all transport lines.
pub struct BeltNetwork {
    lines: SlotMap<TransportLineId, TransportLine>,
    segments: SecondaryMap<EntityId, BeltSegment>,
}

impl BeltNetwork {
    pub fn new() -> Self {
        Self {
            lines: SlotMap::with_key(),
            segments: SecondaryMap::new(),
        }
    }

    /// Called after a belt entity is placed in the world.
    /// Merges consecutive same-direction segments within a tile into one line.
    pub fn on_belt_placed(
        &mut self,
        entity: EntityId,
        tile: &[u8],
        gx: i32,
        gy: i32,
        direction: Direction,
        world: &WorldState,
    ) {
        let (dx, dy) = direction.grid_offset_i32();

        // Find upstream (behind) and downstream (ahead) same-direction neighbors
        let behind = (gx - dx, gy - dy);
        let ahead = (gx + dx, gy + dy);

        let upstream_seg = if is_within_tile(behind.0, behind.1) {
            find_belt_at(tile, behind, direction, world)
                .and_then(|e| self.segments.get(e).copied())
        } else {
            None
        };

        let downstream_seg = if is_within_tile(ahead.0, ahead.1) {
            find_belt_at(tile, ahead, direction, world)
                .and_then(|e| self.segments.get(e).copied())
        } else {
            None
        };

        match (upstream_seg, downstream_seg) {
            (None, None) => {
                // No neighbors — create a new single-segment line.
                let line_id = self.lines.insert(TransportLine::new(FP_SCALE));
                self.segments.insert(entity, BeltSegment { line: line_id, offset: 0 });
            }
            (Some(up), None) => {
                // We're downstream of upstream (closer to output).
                // Insert at output end: shift existing items/segments up.
                let line_id = up.line;
                let line = self.lines.get_mut(line_id).unwrap();
                for item in &mut line.items {
                    item.pos += FP_SCALE;
                }
                line.length += FP_SCALE;
                for (_, seg) in self.segments.iter_mut() {
                    if seg.line == line_id {
                        seg.offset += FP_SCALE;
                    }
                }
                self.segments.insert(entity, BeltSegment { line: line_id, offset: 0 });
            }
            (None, Some(down)) => {
                // We're upstream of downstream (closer to input).
                // Insert at input end: no shifting needed.
                let line = self.lines.get_mut(down.line).unwrap();
                let new_offset = line.length;
                line.length += FP_SCALE;
                self.segments.insert(entity, BeltSegment { line: down.line, offset: new_offset });
            }
            (Some(up), Some(down)) if up.line == down.line => {
                // Both on the same line already (filling a gap) — shouldn't normally happen.
                // Create a standalone line as a fallback.
                let line_id = self.lines.insert(TransportLine::new(FP_SCALE));
                self.segments.insert(entity, BeltSegment { line: line_id, offset: 0 });
            }
            (Some(up), Some(down)) => {
                // Bridge two lines — merge upstream into downstream.
                // Result: [downstream segments] [new segment] [upstream segments]
                let up_line_id = up.line;
                let down_line_id = down.line;

                let down_len = self.lines.get(down_line_id).unwrap().length;
                let new_seg_offset = down_len;
                let up_shift = down_len + FP_SCALE;

                // Take items and metadata from upstream line.
                let (up_items, up_length, up_input_end) = {
                    let up_line = self.lines.get_mut(up_line_id).unwrap();
                    (
                        std::mem::take(&mut up_line.items),
                        up_line.length,
                        up_line.input_end,
                    )
                };

                // Merge into downstream line.
                let down_line = self.lines.get_mut(down_line_id).unwrap();
                for mut item in up_items {
                    item.pos += up_shift;
                    down_line.items.push(item);
                }
                down_line.items.sort_by_key(|i| i.pos);
                down_line.length = down_len + FP_SCALE + up_length;
                down_line.input_end = up_input_end;

                // Update external line that fed into upstream's input.
                if let BeltEnd::Belt(feeder_id) = up_input_end {
                    if let Some(feeder) = self.lines.get_mut(feeder_id) {
                        feeder.output_end = BeltEnd::Belt(down_line_id);
                    }
                }

                // Reassign all upstream segments to the downstream line.
                for (_, seg) in self.segments.iter_mut() {
                    if seg.line == up_line_id {
                        seg.offset += up_shift;
                        seg.line = down_line_id;
                    }
                }

                // Add the new bridging segment.
                self.segments.insert(entity, BeltSegment { line: down_line_id, offset: new_seg_offset });

                // Remove the old upstream line.
                self.lines.remove(up_line_id);
            }
        }

        // --- Perpendicular side-inject detection ---
        let new_seg = *self.segments.get(entity).unwrap();

        // Forward: if this belt is at the output end and faces a perpendicular belt ahead
        if new_seg.offset == 0 {
            let output_open = self.lines.get(new_seg.line)
                .map(|l| l.output_end == BeltEnd::Open)
                .unwrap_or(false);
            if output_open && is_within_tile(ahead.0, ahead.1) {
                if let Some((target_entity, target_dir)) = find_any_belt_at(tile, ahead, world) {
                    if is_perpendicular(direction, target_dir) {
                        self.lines.get_mut(new_seg.line).unwrap().output_end =
                            BeltEnd::SideInject { entity: target_entity };
                    }
                }
            }
        }

        // Reverse: check if existing belts in adjacent cells aim their output at this cell
        for adj_dir in [Direction::North, Direction::East, Direction::South, Direction::West] {
            let (adj_dx, adj_dy) = adj_dir.grid_offset_i32();
            let (nx, ny) = (gx + adj_dx, gy + adj_dy);
            if !is_within_tile(nx, ny) { continue; }

            // A belt at (nx,ny) going adj_dir.opposite() has its output at (gx,gy)
            let required_dir = adj_dir.opposite();
            if !is_perpendicular(required_dir, direction) { continue; }

            if let Some(other_entity) = find_belt_at(tile, (nx, ny), required_dir, world) {
                if let Some(other_seg) = self.segments.get(other_entity).copied() {
                    if other_seg.offset == 0 {
                        let other_output_open = self.lines.get(other_seg.line)
                            .map(|l| l.output_end == BeltEnd::Open)
                            .unwrap_or(false);
                        if other_output_open {
                            self.lines.get_mut(other_seg.line).unwrap().output_end =
                                BeltEnd::SideInject { entity };
                        }
                    }
                }
            }
        }
    }

    /// Run one simulation tick for all transport lines.
    pub fn tick(&mut self) {
        let line_ids: Vec<TransportLineId> = self.lines.keys().collect();

        // Phase 1: Transfer items at output ends to connected inputs.
        let mut transfers: Vec<(TransportLineId, TransportLineId)> = Vec::new();
        let mut side_injects: Vec<(TransportLineId, TransportLineId, u32)> = Vec::new();

        for &line_id in &line_ids {
            let line = match self.lines.get(line_id) {
                Some(l) => l,
                None => continue,
            };
            if line.items.is_empty() {
                continue;
            }
            // Front item sitting at the output end?
            if line.items[0].pos == 0 {
                match line.output_end {
                    BeltEnd::Belt(target_id) => {
                        if let Some(target) = self.lines.get(target_id) {
                            if target.can_accept_at_input() {
                                transfers.push((line_id, target_id));
                            }
                        }
                    }
                    BeltEnd::SideInject { entity: target_entity } => {
                        if let Some(target_seg) = self.segments.get(target_entity).copied() {
                            let inject_offset = target_seg.offset + FP_SCALE / 2;
                            if let Some(target_line) = self.lines.get(target_seg.line) {
                                if target_line.can_accept_at_offset(inject_offset) {
                                    side_injects.push((line_id, target_seg.line, inject_offset));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        for (source_id, target_id) in transfers {
            let item = {
                let source = self.lines.get_mut(source_id).unwrap();
                source.items.remove(0).item
            };
            let target = self.lines.get_mut(target_id).unwrap();
            target.insert_at_input(item);
        }

        for (source_id, target_id, offset) in side_injects {
            let item = {
                let source = self.lines.get_mut(source_id).unwrap();
                source.items.remove(0).item
            };
            let target = self.lines.get_mut(target_id).unwrap();
            target.insert_at_offset(item, offset);
        }

        // Phase 2: Advance all items toward output.
        for &line_id in &line_ids {
            if let Some(line) = self.lines.get_mut(line_id) {
                line.advance();
            }
        }
    }

    /// Debug: spawn an item at the center of the belt entity's segment.
    pub fn spawn_item_on_entity(&mut self, entity: EntityId, item: ItemId) {
        if let Some(seg) = self.segments.get(entity).copied() {
            if let Some(line) = self.lines.get_mut(seg.line) {
                let pos = seg.offset + FP_SCALE / 2;
                let idx = line.items.partition_point(|i| i.pos < pos);
                line.items.insert(idx, BeltItem { item, pos });
            }
        }
    }

    /// Get items on the belt entity's segment, plus the segment offset.
    /// Returns `(items_slice, offset)` where each item's position relative to
    /// this segment is `item.pos - offset` (range 0..FP_SCALE).
    pub fn entity_items(&self, entity: EntityId) -> Option<(&[BeltItem], u32)> {
        let seg = self.segments.get(entity)?;
        let line = self.lines.get(seg.line)?;
        let start = line.items.partition_point(|i| i.pos < seg.offset);
        let end = line.items.partition_point(|i| i.pos < seg.offset + FP_SCALE);
        Some((&line.items[start..end], seg.offset))
    }

    /// Link the output end of `source`'s transport line to the input end of
    /// `target`'s transport line. Used for cross-tile belt connections where
    /// items should transfer across tile boundaries.
    pub fn link_output_to_input(&mut self, source: EntityId, target: EntityId) {
        let source_line_id = match self.segments.get(source) {
            Some(seg) => seg.line,
            None => return,
        };
        let target_line_id = match self.segments.get(target) {
            Some(seg) => seg.line,
            None => return,
        };
        if source_line_id == target_line_id {
            return;
        }
        if let Some(line) = self.lines.get_mut(source_line_id) {
            line.output_end = BeltEnd::Belt(target_line_id);
        }
        if let Some(line) = self.lines.get_mut(target_line_id) {
            line.input_end = BeltEnd::Belt(source_line_id);
        }
    }

    /// Connect a belt's transport line output to a machine input port.
    /// Only succeeds if the belt entity is at the output end of its line.
    pub fn connect_belt_to_machine_input(
        &mut self,
        belt_entity: EntityId,
        machine_entity: EntityId,
        slot: usize,
    ) {
        let seg = match self.segments.get(belt_entity) {
            Some(s) => *s,
            None => return,
        };
        // Only connect if this belt is at the output end of its line (offset 0)
        if seg.offset != 0 {
            return;
        }
        if let Some(line) = self.lines.get_mut(seg.line) {
            line.output_end = BeltEnd::MachineInput {
                entity: machine_entity,
                slot,
            };
        }
    }

    /// Connect a belt's transport line input to a machine output port.
    /// Only succeeds if the belt entity is at the input end of its line.
    pub fn connect_machine_output_to_belt(
        &mut self,
        belt_entity: EntityId,
        machine_entity: EntityId,
        slot: usize,
    ) {
        let seg = match self.segments.get(belt_entity) {
            Some(s) => *s,
            None => return,
        };
        let line_len = match self.lines.get(seg.line) {
            Some(l) => l.length,
            None => return,
        };
        // Only connect if this belt is at the input end of its line
        if seg.offset != line_len - FP_SCALE {
            return;
        }
        if let Some(line) = self.lines.get_mut(seg.line) {
            line.input_end = BeltEnd::MachineOutput {
                entity: machine_entity,
                slot,
            };
        }
    }

    /// Disconnect all belt connections to/from a machine entity.
    /// Sets any BeltEnd referencing this machine back to Open.
    pub fn disconnect_machine_ports(&mut self, machine_entity: EntityId) {
        for (_id, line) in self.lines.iter_mut() {
            match line.output_end {
                BeltEnd::MachineInput { entity, .. } if entity == machine_entity => {
                    line.output_end = BeltEnd::Open;
                }
                _ => {}
            }
            match line.input_end {
                BeltEnd::MachineOutput { entity, .. } if entity == machine_entity => {
                    line.input_end = BeltEnd::Open;
                }
                _ => {}
            }
        }
    }

    /// Connect a belt's output end to a splitter (belt feeds into splitter).
    /// Only succeeds if the belt entity is at the output end of its line
    /// and the output end is currently Open.
    pub fn connect_belt_to_splitter(
        &mut self,
        belt_entity: EntityId,
        splitter_entity: EntityId,
    ) -> bool {
        let seg = match self.segments.get(belt_entity) {
            Some(s) => *s,
            None => return false,
        };
        if seg.offset != 0 {
            return false;
        }
        let line = match self.lines.get_mut(seg.line) {
            Some(l) => l,
            None => return false,
        };
        if line.output_end != BeltEnd::Open {
            return false;
        }
        line.output_end = BeltEnd::Splitter { entity: splitter_entity };
        true
    }

    /// Connect a splitter's output to a belt's input end (splitter feeds belt).
    /// Only succeeds if the belt entity is at the input end of its line
    /// and the input end is currently Open.
    pub fn connect_splitter_to_belt(
        &mut self,
        belt_entity: EntityId,
        splitter_entity: EntityId,
    ) -> bool {
        let seg = match self.segments.get(belt_entity) {
            Some(s) => *s,
            None => return false,
        };
        let line_len = match self.lines.get(seg.line) {
            Some(l) => l.length,
            None => return false,
        };
        if seg.offset != line_len - FP_SCALE {
            return false;
        }
        let line = self.lines.get_mut(seg.line).unwrap();
        if line.input_end != BeltEnd::Open {
            return false;
        }
        line.input_end = BeltEnd::Splitter { entity: splitter_entity };
        true
    }

    /// Disconnect all belt connections to/from a splitter entity.
    /// Sets any BeltEnd::Splitter referencing this splitter back to Open.
    pub fn disconnect_splitter_ports(&mut self, splitter_entity: EntityId) {
        for (_id, line) in self.lines.iter_mut() {
            if let BeltEnd::Splitter { entity } = line.output_end {
                if entity == splitter_entity {
                    line.output_end = BeltEnd::Open;
                }
            }
            if let BeltEnd::Splitter { entity } = line.input_end {
                if entity == splitter_entity {
                    line.input_end = BeltEnd::Open;
                }
            }
        }
    }

    /// Connect a belt's transport line output to a storage input port.
    /// Only succeeds if the belt entity is at the output end of its line.
    pub fn connect_belt_to_storage_input(
        &mut self,
        belt_entity: EntityId,
        storage_entity: EntityId,
        slot: usize,
    ) {
        let seg = match self.segments.get(belt_entity) {
            Some(s) => *s,
            None => return,
        };
        if seg.offset != 0 {
            return;
        }
        if let Some(line) = self.lines.get_mut(seg.line) {
            line.output_end = BeltEnd::StorageInput {
                entity: storage_entity,
                slot,
            };
        }
    }

    /// Connect a storage output port to a belt's transport line input.
    /// Only succeeds if the belt entity is at the input end of its line.
    pub fn connect_storage_output_to_belt(
        &mut self,
        belt_entity: EntityId,
        storage_entity: EntityId,
        slot: usize,
    ) {
        let seg = match self.segments.get(belt_entity) {
            Some(s) => *s,
            None => return,
        };
        let line_len = match self.lines.get(seg.line) {
            Some(l) => l.length,
            None => return,
        };
        if seg.offset != line_len - FP_SCALE {
            return;
        }
        if let Some(line) = self.lines.get_mut(seg.line) {
            line.input_end = BeltEnd::StorageOutput {
                entity: storage_entity,
                slot,
            };
        }
    }

    /// Disconnect all belt connections to/from a storage entity.
    /// Sets any BeltEnd::StorageInput/StorageOutput referencing this storage back to Open.
    pub fn disconnect_storage_ports(&mut self, storage_entity: EntityId) {
        for (_id, line) in self.lines.iter_mut() {
            match line.output_end {
                BeltEnd::StorageInput { entity, .. } if entity == storage_entity => {
                    line.output_end = BeltEnd::Open;
                }
                _ => {}
            }
            match line.input_end {
                BeltEnd::StorageOutput { entity, .. } if entity == storage_entity => {
                    line.input_end = BeltEnd::Open;
                }
                _ => {}
            }
        }
    }

    /// Count how many belt lines are connected to a storage entity as inputs/outputs.
    /// Returns (input_count, output_count).
    pub fn storage_connection_counts(&self, storage_entity: EntityId) -> (usize, usize) {
        let mut inputs = 0;
        let mut outputs = 0;
        for (_id, line) in self.lines.iter() {
            if let BeltEnd::StorageInput { entity, .. } = line.output_end {
                if entity == storage_entity {
                    inputs += 1;
                }
            }
            if let BeltEnd::StorageOutput { entity, .. } = line.input_end {
                if entity == storage_entity {
                    outputs += 1;
                }
            }
        }
        (inputs, outputs)
    }

    /// Check if a belt entity's line has a front item at pos=0 ready to take.
    pub fn peek_front_item(&self, belt_entity: EntityId) -> Option<ItemId> {
        let seg = self.segments.get(belt_entity)?;
        let line = self.lines.get(seg.line)?;
        if !line.items.is_empty() && line.items[0].pos == 0 {
            Some(line.items[0].item)
        } else {
            None
        }
    }

    /// Take the front item from a belt entity's transport line (pos=0).
    pub fn take_front_item(&mut self, belt_entity: EntityId) -> Option<ItemId> {
        let seg = self.segments.get(belt_entity)?;
        let line = self.lines.get_mut(seg.line)?;
        if !line.items.is_empty() && line.items[0].pos == 0 {
            Some(line.items.remove(0).item)
        } else {
            None
        }
    }

    /// Check if a belt entity's line can accept an item at its input end.
    pub fn can_accept_at_entity_input(&self, belt_entity: EntityId) -> bool {
        let seg = match self.segments.get(belt_entity) {
            Some(s) => s,
            None => return false,
        };
        self.lines.get(seg.line)
            .map(|l| l.can_accept_at_input())
            .unwrap_or(false)
    }

    /// Push an item to the input end of a belt entity's transport line.
    pub fn push_to_entity_input(&mut self, belt_entity: EntityId, item: ItemId) -> bool {
        let seg = match self.segments.get(belt_entity) {
            Some(s) => *s,
            None => return false,
        };
        let line = match self.lines.get_mut(seg.line) {
            Some(l) => l,
            None => return false,
        };
        if line.can_accept_at_input() {
            line.insert_at_input(item);
            true
        } else {
            false
        }
    }

    /// Get splitter connections on the line containing this belt entity.
    /// Returns (output_end_splitter, input_end_splitter).
    pub fn line_splitter_connections(&self, entity: EntityId) -> (Option<EntityId>, Option<EntityId>) {
        let seg = match self.segments.get(entity) {
            Some(s) => s,
            None => return (None, None),
        };
        let line = match self.lines.get(seg.line) {
            Some(l) => l,
            None => return (None, None),
        };
        let output_splitter = match line.output_end {
            BeltEnd::Splitter { entity } => Some(entity),
            _ => None,
        };
        let input_splitter = match line.input_end {
            BeltEnd::Splitter { entity } => Some(entity),
            _ => None,
        };
        (output_splitter, input_splitter)
    }

    /// Run port transfers: move items between belt endpoints and machine ports.
    /// Call this each tick after belt advance and machine tick.
    pub fn tick_port_transfers(&mut self, machine_pool: &mut MachinePool) {
        let line_ids: Vec<TransportLineId> = self.lines.keys().collect();

        // Phase 1: Belt → Machine (input ports)
        // Items at a belt's output end (pos=0) transfer into machine input slots.
        let mut belt_to_machine: Vec<(TransportLineId, ItemId, EntityId, usize)> = Vec::new();
        for &line_id in &line_ids {
            let line = match self.lines.get(line_id) {
                Some(l) => l,
                None => continue,
            };
            if let BeltEnd::MachineInput { entity, slot } = line.output_end {
                if !line.items.is_empty() && line.items[0].pos == 0 {
                    belt_to_machine.push((line_id, line.items[0].item, entity, slot));
                }
            }
        }
        for (line_id, item, entity, slot) in belt_to_machine {
            if machine_pool.insert_input_at_slot(entity, slot, item, 1) {
                if let Some(line) = self.lines.get_mut(line_id) {
                    line.items.remove(0);
                }
            }
        }

        // Phase 2: Machine → Belt (output ports)
        // Machine output slots feed into a belt's input end.
        let mut machine_to_belt: Vec<(TransportLineId, EntityId, usize)> = Vec::new();
        for &line_id in &line_ids {
            let line = match self.lines.get(line_id) {
                Some(l) => l,
                None => continue,
            };
            if let BeltEnd::MachineOutput { entity, slot } = line.input_end {
                if line.can_accept_at_input() {
                    machine_to_belt.push((line_id, entity, slot));
                }
            }
        }
        for (line_id, entity, slot) in machine_to_belt {
            if let Some(item) = machine_pool.take_output_from_slot(entity, slot) {
                if let Some(line) = self.lines.get_mut(line_id) {
                    line.insert_at_input(item);
                }
            }
        }
    }

    /// Remove a belt entity from the network. This handles splitting or
    /// shrinking the transport line as needed. Items on the removed segment
    /// are lost (dropped).
    pub fn on_belt_removed(&mut self, entity: EntityId) {
        // Clean up any SideInject links that target this entity.
        for (_, line) in self.lines.iter_mut() {
            if let BeltEnd::SideInject { entity: target } = line.output_end {
                if target == entity {
                    line.output_end = BeltEnd::Open;
                }
            }
        }

        let seg = match self.segments.remove(entity) {
            Some(s) => s,
            None => return,
        };
        let line = match self.lines.get(seg.line) {
            Some(l) => l,
            None => return,
        };

        let line_len = line.length;

        if line_len == FP_SCALE {
            // Only segment on the line — remove the entire line.
            // Clear any external references pointing to this line.
            let input_end = line.input_end;
            let output_end = line.output_end;
            self.lines.remove(seg.line);

            if let BeltEnd::Belt(other_id) = output_end {
                if let Some(other) = self.lines.get_mut(other_id) {
                    if other.input_end == BeltEnd::Belt(seg.line) {
                        other.input_end = BeltEnd::Open;
                    }
                }
            }
            if let BeltEnd::Belt(other_id) = input_end {
                if let Some(other) = self.lines.get_mut(other_id) {
                    if other.output_end == BeltEnd::Belt(seg.line) {
                        other.output_end = BeltEnd::Open;
                    }
                }
            }
            return;
        }

        if seg.offset == 0 {
            // Removing the output-end segment. Shrink the line.
            let line = self.lines.get_mut(seg.line).unwrap();
            // Clear SideInject — it was specific to this segment's position,
            // and the new output-end segment may not be adjacent to the target.
            if matches!(line.output_end, BeltEnd::SideInject { .. } | BeltEnd::Splitter { .. }) {
                line.output_end = BeltEnd::Open;
            }
            // Remove items in the removed segment's range [0, FP_SCALE)
            line.items.retain(|i| i.pos >= FP_SCALE);
            // Shift remaining items and segments toward output
            for item in &mut line.items {
                item.pos -= FP_SCALE;
            }
            line.length -= FP_SCALE;
            for (_, s) in self.segments.iter_mut() {
                if s.line == seg.line {
                    s.offset -= FP_SCALE;
                }
            }
        } else if seg.offset == line_len - FP_SCALE {
            // Removing the input-end segment. Shrink the line.
            let line = self.lines.get_mut(seg.line).unwrap();
            // Clear Splitter — the new input-end segment may not be adjacent to the splitter.
            if matches!(line.input_end, BeltEnd::Splitter { .. }) {
                line.input_end = BeltEnd::Open;
            }
            // Remove items in the removed segment's range
            let seg_start = seg.offset;
            line.items.retain(|i| i.pos < seg_start);
            line.length -= FP_SCALE;
        } else {
            // Middle segment: split into two lines.
            // Keep the output half [0, seg.offset) on the original line.
            // Create a new line for the input half [seg.offset + FP_SCALE, line_len).
            let old_line_id = seg.line;
            let split_point = seg.offset;
            let old_line = self.lines.get(old_line_id).unwrap();
            let old_input_end = old_line.input_end;

            // Collect items for each half
            let mut output_items = Vec::new();
            let mut input_items = Vec::new();
            for item in &old_line.items {
                if item.pos < split_point {
                    output_items.push(item.clone());
                } else if item.pos >= split_point + FP_SCALE {
                    input_items.push(BeltItem {
                        item: item.item,
                        pos: item.pos - split_point - FP_SCALE,
                    });
                }
                // Items in [split_point, split_point + FP_SCALE) are dropped
            }

            let input_half_len = line_len - split_point - FP_SCALE;

            // Update the original line to be the output half
            let line = self.lines.get_mut(old_line_id).unwrap();
            line.items = output_items;
            line.length = split_point;
            line.input_end = BeltEnd::Open;

            // Create the new input-half line
            let new_line_id = self.lines.insert(TransportLine {
                items: input_items,
                speed: DEFAULT_BELT_SPEED,
                length: input_half_len,
                input_end: old_input_end,
                output_end: BeltEnd::Open,
            });

            // Update external line that fed into the old input end
            if let BeltEnd::Belt(feeder_id) = old_input_end {
                if let Some(feeder) = self.lines.get_mut(feeder_id) {
                    if feeder.output_end == BeltEnd::Belt(old_line_id) {
                        feeder.output_end = BeltEnd::Belt(new_line_id);
                    }
                }
            }

            // Reassign segments from the input half to the new line
            for (_, s) in self.segments.iter_mut() {
                if s.line == old_line_id && s.offset > split_point {
                    s.line = new_line_id;
                    s.offset -= split_point + FP_SCALE;
                }
            }
        }
    }

    /// Advance N ticks (for chunk fast-forward).
    #[allow(dead_code)]
    pub fn fast_forward(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.tick();
        }
    }
}

/// Check if a grid position is within a tile's bounds (-32..=32 on each axis).
pub fn is_within_tile(gx: i32, gy: i32) -> bool {
    (-32..=32).contains(&gx) && (-32..=32).contains(&gy)
}

/// Find a belt entity at the given tile+grid position with the given direction.
fn find_belt_at(
    tile: &[u8],
    grid_xy: (i32, i32),
    direction: Direction,
    world: &WorldState,
) -> Option<EntityId> {
    let entities = world.tile_entities(tile)?;
    let &entity = entities.get(&grid_xy)?;
    if world.kind(entity) == Some(StructureKind::Belt)
        && world.direction(entity) == Some(direction)
    {
        Some(entity)
    } else {
        None
    }
}

/// Find any belt entity at the given tile+grid position, regardless of direction.
fn find_any_belt_at(
    tile: &[u8],
    grid_xy: (i32, i32),
    world: &WorldState,
) -> Option<(EntityId, Direction)> {
    let entities = world.tile_entities(tile)?;
    let &entity = entities.get(&grid_xy)?;
    if world.kind(entity) == Some(StructureKind::Belt) {
        let dir = world.direction(entity)?;
        Some((entity, dir))
    } else {
        None
    }
}

/// Two directions are perpendicular if they are neither the same nor opposite.
fn is_perpendicular(a: Direction, b: Direction) -> bool {
    a != b && a != b.opposite()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place_belt(world: &mut WorldState, net: &mut BeltNetwork, addr: &[u8], gx: i32, gy: i32, dir: Direction) -> EntityId {
        let entity = world.place(addr, (gx, gy), ItemId::Belt, dir).unwrap();
        net.on_belt_placed(entity, addr, gx, gy, dir, world);
        entity
    }

    /// Helper: get items for an entity with positions relative to the segment.
    fn local_items(net: &BeltNetwork, entity: EntityId) -> Vec<(ItemId, u32)> {
        match net.entity_items(entity) {
            Some((items, offset)) => items.iter().map(|i| (i.item, i.pos - offset)).collect(),
            None => vec![],
        }
    }

    #[test]
    fn single_item_moves_toward_output() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let e = place_belt(&mut world, &mut net, &[0], 0, 0, Direction::East);

        net.spawn_item_on_entity(e, ItemId::NullSet);
        let items = local_items(&net, e);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1, FP_SCALE / 2);

        net.tick();

        let items = local_items(&net, e);
        assert_eq!(items[0].1, FP_SCALE / 2 - DEFAULT_BELT_SPEED as u32);
    }

    #[test]
    fn item_stops_at_open_end() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let e = place_belt(&mut world, &mut net, &[0], 0, 0, Direction::East);

        net.spawn_item_on_entity(e, ItemId::NullSet);
        for _ in 0..1000 {
            net.tick();
        }

        let items = local_items(&net, e);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1, 0);
    }

    #[test]
    fn items_compress_behind_blocked_front() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let e = place_belt(&mut world, &mut net, &[0], 0, 0, Direction::East);

        // Manually insert two items on the line
        let seg = *net.segments.get(e).unwrap();
        let line = net.lines.get_mut(seg.line).unwrap();
        line.items.push(BeltItem { item: ItemId::NullSet, pos: 100 });
        line.items.push(BeltItem { item: ItemId::Point, pos: 200 });

        for _ in 0..1000 {
            net.tick();
        }

        let items = local_items(&net, e);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].1, 0);
        assert_eq!(items[1].1, MIN_ITEM_GAP);
    }

    #[test]
    fn consecutive_belts_merge_into_one_line() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];
        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);

        // Both entities should be on the same transport line.
        let seg1 = *net.segments.get(e1).unwrap();
        let seg2 = *net.segments.get(e2).unwrap();
        assert_eq!(seg1.line, seg2.line);

        // e2 is downstream (output side), e1 is upstream (input side).
        // e2 offset = 0, e1 offset = FP_SCALE.
        assert_eq!(seg2.offset, 0);
        assert_eq!(seg1.offset, FP_SCALE);

        // Merged line length = 2 * FP_SCALE.
        let line = net.lines.get(seg1.line).unwrap();
        assert_eq!(line.length, 2 * FP_SCALE);
        assert_eq!(line.input_end, BeltEnd::Open);
        assert_eq!(line.output_end, BeltEnd::Open);
    }

    #[test]
    fn item_flows_through_merged_line() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];
        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);

        // Spawn item on e1 (upstream segment)
        net.spawn_item_on_entity(e1, ItemId::NullSet);

        // Item starts at center of e1's segment
        let items = local_items(&net, e1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1, FP_SCALE / 2);

        // Run enough ticks for item to cross into e2
        for _ in 0..500 {
            net.tick();
        }

        // Item should now be in e2's segment (at output end, pos=0)
        let items1 = local_items(&net, e1);
        let items2 = local_items(&net, e2);
        assert_eq!(items1.len(), 0);
        assert_eq!(items2.len(), 1);
        assert_eq!(items2[0].0, ItemId::NullSet);
    }

    #[test]
    fn fast_forward_advances_correctly() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let e = place_belt(&mut world, &mut net, &[0], 0, 0, Direction::East);

        net.spawn_item_on_entity(e, ItemId::NullSet);
        let start = local_items(&net, e)[0].1;

        net.fast_forward(10);

        let after = local_items(&net, e)[0].1;
        assert_eq!(after, start - 10 * DEFAULT_BELT_SPEED as u32);
    }

    #[test]
    fn placement_order_does_not_matter() {
        // Place downstream belt first, then upstream — should still merge.
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);
        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        let seg1 = *net.segments.get(e1).unwrap();
        let seg2 = *net.segments.get(e2).unwrap();
        assert_eq!(seg1.line, seg2.line);

        let line = net.lines.get(seg1.line).unwrap();
        assert_eq!(line.length, 2 * FP_SCALE);
    }

    #[test]
    fn different_directions_do_not_merge() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);

        let seg1 = *net.segments.get(e1).unwrap();
        let seg2 = *net.segments.get(e2).unwrap();
        assert_ne!(seg1.line, seg2.line);
    }

    #[test]
    fn three_belts_merge_into_one_line() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);
        let e3 = place_belt(&mut world, &mut net, addr, 2, 0, Direction::East);

        let seg1 = *net.segments.get(e1).unwrap();
        let seg2 = *net.segments.get(e2).unwrap();
        let seg3 = *net.segments.get(e3).unwrap();
        assert_eq!(seg1.line, seg2.line);
        assert_eq!(seg2.line, seg3.line);

        let line = net.lines.get(seg1.line).unwrap();
        assert_eq!(line.length, 3 * FP_SCALE);

        // e3 is most downstream (output), e1 is most upstream (input).
        assert_eq!(seg3.offset, 0);
        assert_eq!(seg2.offset, FP_SCALE);
        assert_eq!(seg1.offset, 2 * FP_SCALE);
    }

    #[test]
    fn bridge_merges_two_lines() {
        // Place e1 and e3 first (two separate lines), then e2 bridges them.
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e3 = place_belt(&mut world, &mut net, addr, 2, 0, Direction::East);

        // Before bridge: two separate lines.
        let seg1_before = *net.segments.get(e1).unwrap();
        let seg3_before = *net.segments.get(e3).unwrap();
        assert_ne!(seg1_before.line, seg3_before.line);

        // Place the bridge.
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);

        // After bridge: all on one line.
        let seg1 = *net.segments.get(e1).unwrap();
        let seg2 = *net.segments.get(e2).unwrap();
        let seg3 = *net.segments.get(e3).unwrap();
        assert_eq!(seg1.line, seg2.line);
        assert_eq!(seg2.line, seg3.line);

        let line = net.lines.get(seg1.line).unwrap();
        assert_eq!(line.length, 3 * FP_SCALE);
    }

    #[test]
    fn perpendicular_belt_side_injects_items() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // Two belts: e1 East, e2 North — perpendicular, connected via SideInject.
        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);

        net.spawn_item_on_entity(e1, ItemId::NullSet);

        for _ in 0..500 {
            net.tick();
        }

        // Item should have transferred via SideInject to the North belt.
        let items1 = local_items(&net, e1);
        let items2 = local_items(&net, e2);
        assert_eq!(items1.len(), 0, "Item should have left the East belt");
        assert_eq!(items2.len(), 1, "Item should be on the North belt");
        assert_eq!(items2[0].0, ItemId::NullSet);
    }

    #[test]
    fn cross_tile_link_transfers_items() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();

        // Tile A: belt at (32, 0) East — at the eastern edge
        let e1 = place_belt(&mut world, &mut net, &[0], 32, 0, Direction::East);
        // Tile B: belt at (-32, 0) East — at the western edge
        let e2 = place_belt(&mut world, &mut net, &[0, 0], -32, 0, Direction::East);

        // Link: e1's output → e2's input (cross-tile)
        net.link_output_to_input(e1, e2);

        // Spawn item on e1
        net.spawn_item_on_entity(e1, ItemId::NullSet);

        // Run enough ticks for item to transfer
        for _ in 0..1000 {
            net.tick();
        }

        // Item should have moved from e1 to e2
        let items1 = local_items(&net, e1);
        let items2 = local_items(&net, e2);
        assert_eq!(items1.len(), 0);
        assert_eq!(items2.len(), 1);
        assert_eq!(items2[0].0, ItemId::NullSet);
    }

    #[test]
    fn cross_tile_link_with_merged_lines() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();

        // Tile A: belts at (31, 0) and (32, 0) East — merged line
        let e1 = place_belt(&mut world, &mut net, &[0], 31, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, &[0], 32, 0, Direction::East);
        // Tile B: belt at (-32, 0) East
        let e3 = place_belt(&mut world, &mut net, &[0, 0], -32, 0, Direction::East);

        // e1 and e2 are on the same line
        let seg1 = *net.segments.get(e1).unwrap();
        let seg2 = *net.segments.get(e2).unwrap();
        assert_eq!(seg1.line, seg2.line);

        // Link output (e2 is at offset 0) to e3's input
        net.link_output_to_input(e2, e3);

        // Spawn item on e1 (upstream)
        net.spawn_item_on_entity(e1, ItemId::NullSet);

        for _ in 0..2000 {
            net.tick();
        }

        // Item should flow through e1 → e2 → cross-tile → e3
        let items3 = local_items(&net, e3);
        assert_eq!(items3.len(), 1);
        assert_eq!(items3[0].0, ItemId::NullSet);
    }

    #[test]
    fn cross_tile_bidirectional_link() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();

        // Two belts flowing in opposite directions across the same tile boundary
        // Tile A: belt at (32, 0) East, belt at (32, 1) West (separate lines)
        let e1 = place_belt(&mut world, &mut net, &[0], 32, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, &[0], -32, 1, Direction::West);

        // Tile B: corresponding belts
        let e3 = place_belt(&mut world, &mut net, &[0, 0], -32, 0, Direction::East);
        let e4 = place_belt(&mut world, &mut net, &[0, 0], 32, 1, Direction::West);

        // Link e1→e3 (East flow) and e4→e2 (West flow)
        net.link_output_to_input(e1, e3);
        net.link_output_to_input(e4, e2);

        net.spawn_item_on_entity(e1, ItemId::NullSet);
        net.spawn_item_on_entity(e4, ItemId::Point);

        for _ in 0..1000 {
            net.tick();
        }

        let items3 = local_items(&net, e3);
        let items2 = local_items(&net, e2);
        assert_eq!(items3.len(), 1);
        assert_eq!(items3[0].0, ItemId::NullSet);
        assert_eq!(items2.len(), 1);
        assert_eq!(items2[0].0, ItemId::Point);
    }

    // --- Port transfer tests ---

    use crate::game::items::MachineType;
    use crate::game::recipes::RecipeIndex;
    use crate::sim::machine::{MachinePool, DEFAULT_CRAFT_TICKS};

    #[test]
    fn belt_to_machine_input_transfer() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut machines = MachinePool::new();
        let addr: &[u8] = &[0];

        // Belt at (0,0) flowing East, machine at (1,0)
        let belt = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let machine_entity = world.place(addr, (1, 0), ItemId::Composer, Direction::West).unwrap();
        machines.add(machine_entity, MachineType::Composer);

        // Connect belt output to machine input slot 0
        net.connect_belt_to_machine_input(belt, machine_entity, 0);

        // Spawn item on belt
        net.spawn_item_on_entity(belt, ItemId::Point);

        // Run belt ticks until item reaches output end (pos=0)
        for _ in 0..500 {
            net.tick();
        }
        let items = local_items(&net, belt);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1, 0); // stuck at output end

        // Now run port transfers — item should move into machine
        net.tick_port_transfers(&mut machines);
        let items = local_items(&net, belt);
        assert_eq!(items.len(), 0); // item left the belt
        let slots = machines.input_slots(machine_entity).unwrap();
        assert_eq!(slots[0].item, ItemId::Point);
        assert_eq!(slots[0].count, 1);
    }

    #[test]
    fn machine_output_to_belt_transfer() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut machines = MachinePool::new();
        let addr: &[u8] = &[0];

        // Machine at (0,0) (Composer is 2x2, occupies 0..1 x 0..1), belt at (2,0) flowing East
        let machine_entity = world.place(addr, (0, 0), ItemId::Composer, Direction::East).unwrap();
        machines.add(machine_entity, MachineType::Composer);
        let belt = place_belt(&mut world, &mut net, addr, 2, 0, Direction::East);

        // Connect machine output slot 0 to belt input
        net.connect_machine_output_to_belt(belt, machine_entity, 0);

        // Put an item in machine output
        let i = machines.index_of(machine_entity).unwrap();
        machines.cold.output_slots[i][0] = crate::sim::machine::ItemStack {
            item: ItemId::LineSegment,
            count: 1,
        };

        // Run port transfers — item should appear on belt
        net.tick_port_transfers(&mut machines);
        let seg = *net.segments.get(belt).unwrap();
        let line = net.lines.get(seg.line).unwrap();
        assert_eq!(line.items.len(), 1);
        assert_eq!(line.items[0].item, ItemId::LineSegment);
        assert_eq!(line.items[0].pos, line.length); // at input end
    }

    #[test]
    fn full_production_chain_belt_machine_belt() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut machines = MachinePool::new();
        let recipes = RecipeIndex::new();
        let addr: &[u8] = &[0];

        // Input belt → machine → output belt
        // Composer is 2x2 at (0,0): occupies (0,0)-(1,1)
        // Belt at (-1,0) East feeds into machine, belt at (2,0) East receives output
        let input_belt = place_belt(&mut world, &mut net, addr, -1, 0, Direction::East);
        let machine_entity = world.place(addr, (0, 0), ItemId::Composer, Direction::East).unwrap();
        machines.add(machine_entity, MachineType::Composer);
        machines.set_recipe(machine_entity, Some(0)); // 2x Point -> LineSegment
        let output_belt = place_belt(&mut world, &mut net, addr, 2, 0, Direction::East);

        // Connect: input belt output → machine input slot 0
        net.connect_belt_to_machine_input(input_belt, machine_entity, 0);
        // Connect: machine output slot 0 → output belt input
        net.connect_machine_output_to_belt(output_belt, machine_entity, 0);

        // Spawn 2 Points on input belt (enough for one craft)
        net.spawn_item_on_entity(input_belt, ItemId::Point);
        // Need a second item — insert at a different position
        {
            let seg = *net.segments.get(input_belt).unwrap();
            let line = net.lines.get_mut(seg.line).unwrap();
            line.items.push(BeltItem { item: ItemId::Point, pos: FP_SCALE });
            line.items.sort_by_key(|i| i.pos);
        }

        // Run the full cycle: belt tick + port transfer + machine tick
        for _ in 0..(500 + DEFAULT_CRAFT_TICKS as u32 + 100) {
            net.tick();
            net.tick_port_transfers(&mut machines);
            machines.tick(&recipes);
        }

        // The machine should have crafted and output a LineSegment onto the output belt
        let output_items: Vec<_> = {
            let seg = *net.segments.get(output_belt).unwrap();
            let line = net.lines.get(seg.line).unwrap();
            line.items.iter().map(|i| i.item).collect()
        };
        assert!(
            output_items.contains(&ItemId::LineSegment),
            "Expected LineSegment on output belt, got: {:?}",
            output_items
        );
    }

    #[test]
    fn connection_only_at_line_endpoint() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // Three merged belts: (0,0), (1,0), (2,0) East
        let _e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);
        let _e3 = place_belt(&mut world, &mut net, addr, 2, 0, Direction::East);

        let machine_entity = world.place(addr, (1, 1), ItemId::Composer, Direction::North).unwrap();

        // Try connecting e2 (middle of line, offset=FP_SCALE) as output endpoint
        // Should NOT connect because e2 is not at offset=0
        net.connect_belt_to_machine_input(e2, machine_entity, 0);
        let seg = *net.segments.get(e2).unwrap();
        let line = net.lines.get(seg.line).unwrap();
        assert_eq!(line.output_end, BeltEnd::Open, "Middle belt should not connect at output end");
    }

    // --- Side-inject (perpendicular belt) tests ---

    #[test]
    fn t_junction_east_into_north_creates_side_inject() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // North belt at (1, 0)
        let north = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);
        // East belt at (0, 0) — output faces (1, 0) where the North belt is
        let east = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        // East belt should have SideInject output to the North belt
        let east_seg = *net.segments.get(east).unwrap();
        let east_line = net.lines.get(east_seg.line).unwrap();
        assert_eq!(east_line.output_end, BeltEnd::SideInject { entity: north });

        // They should be on separate lines
        let north_seg = *net.segments.get(north).unwrap();
        assert_ne!(east_seg.line, north_seg.line);
    }

    #[test]
    fn t_junction_reverse_order_creates_side_inject() {
        // Place the dead-ending belt first, then the perpendicular belt
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // East belt at (0, 0) first — output faces (1, 0), nothing there yet
        let east = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let east_seg = *net.segments.get(east).unwrap();
        assert_eq!(net.lines.get(east_seg.line).unwrap().output_end, BeltEnd::Open);

        // Now place North belt at (1, 0) — the reverse check should connect
        let north = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);

        // East belt should now have SideInject
        let east_seg = *net.segments.get(east).unwrap();
        let east_line = net.lines.get(east_seg.line).unwrap();
        assert_eq!(east_line.output_end, BeltEnd::SideInject { entity: north });
    }

    #[test]
    fn t_junction_items_transfer_to_perpendicular_belt() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // North belt at (1, 0)
        let north = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);
        // East belt at (0, 0) with SideInject to North belt
        let east = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        // Spawn item on East belt
        net.spawn_item_on_entity(east, ItemId::NullSet);

        // Run ticks until item transfers
        for _ in 0..500 {
            net.tick();
        }

        // Item should have moved to the North belt
        let east_items = local_items(&net, east);
        let north_items = local_items(&net, north);
        assert_eq!(east_items.len(), 0, "Item should have left the East belt");
        assert_eq!(north_items.len(), 1, "Item should be on the North belt");
        assert_eq!(north_items[0].0, ItemId::NullSet);
    }

    #[test]
    fn side_inject_respects_min_gap() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // North belt at (1, 0), East belt at (0, 0)
        let north = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);
        let east = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        // Fill the North belt with 4 items (max: FP_SCALE / MIN_ITEM_GAP = 256/64 = 4)
        // They pack at 0, 64, 128, 192 — injection point 128 is exactly occupied.
        let north_seg = *net.segments.get(north).unwrap();
        let line = net.lines.get_mut(north_seg.line).unwrap();
        for i in 0..4u32 {
            line.items.push(BeltItem {
                item: ItemId::Point,
                pos: north_seg.offset + i * MIN_ITEM_GAP,
            });
        }
        line.items.sort_by_key(|i| i.pos);

        // Spawn item on East belt
        net.spawn_item_on_entity(east, ItemId::NullSet);

        // Run just a few ticks (enough for East item to reach pos=0 but
        // not enough for North items to drain away)
        for _ in 0..500 {
            net.tick();
        }

        // Items on the North belt should remain packed since output is Open
        // (items stop at pos=0). Injection at offset 128 is blocked.
        let east_items = local_items(&net, east);
        assert_eq!(east_items.len(), 1, "Item should stay on East belt (blocked)");
        assert_eq!(east_items[0].1, 0, "Item should be stuck at output end");
    }

    #[test]
    fn side_inject_blocks_when_target_full() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // North belt at (1, 0), East belt at (0, 0)
        let north = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);
        let east = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        // Fill the North belt with 4 items (max per segment = FP_SCALE / MIN_ITEM_GAP = 4)
        let north_seg = *net.segments.get(north).unwrap();
        let line = net.lines.get_mut(north_seg.line).unwrap();
        for i in 0..4u32 {
            line.items.push(BeltItem {
                item: ItemId::Point,
                pos: north_seg.offset + i * MIN_ITEM_GAP,
            });
        }
        line.items.sort_by_key(|i| i.pos);

        // Spawn item on East belt
        net.spawn_item_on_entity(east, ItemId::NullSet);

        // Run ticks — should not transfer
        for _ in 0..500 {
            net.tick();
        }

        let east_items = local_items(&net, east);
        assert_eq!(east_items.len(), 1, "Item should stay on East belt (target full)");
    }

    #[test]
    fn removing_target_belt_clears_side_inject() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        let north = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);
        let east = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        // Verify SideInject exists
        let east_seg = *net.segments.get(east).unwrap();
        assert!(matches!(
            net.lines.get(east_seg.line).unwrap().output_end,
            BeltEnd::SideInject { .. }
        ));

        // Remove the North belt (target)
        net.on_belt_removed(north);
        world.remove(addr, (1, 0));

        // SideInject should be cleared
        let east_seg = *net.segments.get(east).unwrap();
        let east_line = net.lines.get(east_seg.line).unwrap();
        assert_eq!(east_line.output_end, BeltEnd::Open);
    }

    #[test]
    fn removing_source_belt_cleans_up() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        let _north = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);
        let east = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        // Remove the East belt (source of SideInject)
        net.on_belt_removed(east);
        world.remove(addr, (0, 0));

        // No crash, no dangling references. The SideInject was on the removed line.
        // Verify the North belt is still fine
        assert_eq!(net.lines.len(), 1);
    }

    #[test]
    fn opposite_direction_does_not_side_inject() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // West belt at (1, 0) — facing opposite to East
        let _west = place_belt(&mut world, &mut net, addr, 1, 0, Direction::West);
        // East belt at (0, 0) — output faces (1, 0) but West is opposite, not perpendicular
        let east = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        let east_seg = *net.segments.get(east).unwrap();
        let east_line = net.lines.get(east_seg.line).unwrap();
        assert_eq!(east_line.output_end, BeltEnd::Open, "Opposite directions should not side-inject");
    }

    #[test]
    fn same_direction_merges_not_side_injects() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // East belt at (1, 0) — same direction
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);
        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        // Should merge, not side-inject
        let seg1 = *net.segments.get(e1).unwrap();
        let seg2 = *net.segments.get(e2).unwrap();
        assert_eq!(seg1.line, seg2.line, "Same direction should merge");
    }

    #[test]
    fn south_belt_side_injects_onto_east_belt() {
        // Test a different orientation: South belt feeds onto East belt
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // East belt at (0, 1)
        let east = place_belt(&mut world, &mut net, addr, 0, 1, Direction::East);
        // South belt at (0, 0) — output faces (0, 1) where the East belt is
        let south = place_belt(&mut world, &mut net, addr, 0, 0, Direction::South);

        let south_seg = *net.segments.get(south).unwrap();
        let south_line = net.lines.get(south_seg.line).unwrap();
        assert_eq!(south_line.output_end, BeltEnd::SideInject { entity: east });

        // Spawn item and verify transfer
        net.spawn_item_on_entity(south, ItemId::NullSet);
        for _ in 0..500 {
            net.tick();
        }

        let south_items = local_items(&net, south);
        let east_items = local_items(&net, east);
        assert_eq!(south_items.len(), 0);
        assert_eq!(east_items.len(), 1);
    }

    #[test]
    fn output_end_removal_clears_side_inject() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // North belt at (2, 0)
        let north = place_belt(&mut world, &mut net, addr, 2, 0, Direction::North);
        // Two merged East belts: (0,0) and (1,0)
        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);

        // e2 is at offset=0 (output end), should have SideInject to North belt
        let e2_seg = *net.segments.get(e2).unwrap();
        assert_eq!(e2_seg.offset, 0);
        let line = net.lines.get(e2_seg.line).unwrap();
        assert_eq!(line.output_end, BeltEnd::SideInject { entity: north });

        // Remove e2 (the output-end belt with SideInject)
        net.on_belt_removed(e2);
        world.remove(addr, (1, 0));

        // e1 is now at offset=0, but SideInject should be cleared
        let e1_seg = *net.segments.get(e1).unwrap();
        assert_eq!(e1_seg.offset, 0);
        let line = net.lines.get(e1_seg.line).unwrap();
        assert_eq!(line.output_end, BeltEnd::Open, "SideInject should be cleared after output-end removal");
    }

    #[test]
    fn can_accept_at_offset_empty_line() {
        let line = TransportLine::new(FP_SCALE);
        assert!(line.can_accept_at_offset(FP_SCALE / 2));
    }

    #[test]
    fn can_accept_at_offset_with_nearby_items() {
        let mut line = TransportLine::new(FP_SCALE);
        // Item at position 128 (center)
        line.items.push(BeltItem { item: ItemId::NullSet, pos: 128 });

        // Too close (gap < MIN_ITEM_GAP)
        assert!(!line.can_accept_at_offset(128 + MIN_ITEM_GAP - 1));
        assert!(!line.can_accept_at_offset(128 - MIN_ITEM_GAP + 1));

        // Exactly at MIN_ITEM_GAP — should accept
        assert!(line.can_accept_at_offset(128 + MIN_ITEM_GAP));
        assert!(line.can_accept_at_offset(128 - MIN_ITEM_GAP));
    }

    // --- Splitter-belt connection tests ---

    fn place_splitter(world: &mut WorldState, pool: &mut crate::sim::splitter::SplitterPool, addr: &[u8], gx: i32, gy: i32) -> EntityId {
        let entity = world.place(addr, (gx, gy), ItemId::Splitter, Direction::North).unwrap();
        pool.add(entity);
        entity
    }

    #[test]
    fn splitter_belt_output_connects_as_input() {
        // Belt going East at (4,5), splitter at (5,5) → belt feeds into splitter
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 5, 5);
        let belt = place_belt(&mut world, &mut net, &[0], 4, 5, Direction::East);

        // Belt's output end faces the splitter
        assert!(net.connect_belt_to_splitter(belt, splitter));
        pool.add_input(splitter, belt);
        pool.detect_mode(splitter);

        // Verify belt's output_end is Splitter
        let seg = net.segments.get(belt).unwrap();
        let line = net.lines.get(seg.line).unwrap();
        assert_eq!(line.output_end, BeltEnd::Splitter { entity: splitter });

        // Splitter should be Inactive (1 input, 0 outputs)
        assert_eq!(pool.get(splitter).unwrap().mode, crate::sim::splitter::SplitterMode::Inactive);
    }

    #[test]
    fn splitter_belt_input_connects_as_output() {
        // Belt going East at (6,5), splitter at (5,5) → splitter feeds belt
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 5, 5);
        let belt = place_belt(&mut world, &mut net, &[0], 6, 5, Direction::East);

        // Belt's input end faces the splitter
        assert!(net.connect_splitter_to_belt(belt, splitter));
        pool.add_output(splitter, belt);
        pool.detect_mode(splitter);

        // Verify belt's input_end is Splitter
        let seg = net.segments.get(belt).unwrap();
        let line = net.lines.get(seg.line).unwrap();
        assert_eq!(line.input_end, BeltEnd::Splitter { entity: splitter });

        // Splitter should be Inactive (0 inputs, 1 output)
        assert_eq!(pool.get(splitter).unwrap().mode, crate::sim::splitter::SplitterMode::Inactive);
    }

    #[test]
    fn splitter_connects_two_inputs_one_output_merger() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 5, 5);

        // Two belts feeding in from West and South
        let belt_w = place_belt(&mut world, &mut net, &[0], 4, 5, Direction::East);
        let belt_s = place_belt(&mut world, &mut net, &[0], 5, 6, Direction::North);

        // One belt taking out to the East
        let belt_e = place_belt(&mut world, &mut net, &[0], 6, 5, Direction::East);

        assert!(net.connect_belt_to_splitter(belt_w, splitter));
        pool.add_input(splitter, belt_w);
        assert!(net.connect_belt_to_splitter(belt_s, splitter));
        pool.add_input(splitter, belt_s);
        assert!(net.connect_splitter_to_belt(belt_e, splitter));
        pool.add_output(splitter, belt_e);
        pool.detect_mode(splitter);

        assert_eq!(pool.get(splitter).unwrap().inputs.len(), 2);
        assert_eq!(pool.get(splitter).unwrap().outputs.len(), 1);
        assert_eq!(pool.get(splitter).unwrap().mode, crate::sim::splitter::SplitterMode::Merger);
    }

    #[test]
    fn splitter_connects_one_input_two_outputs_splitter_mode() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 5, 5);

        // One belt feeding in from West
        let belt_in = place_belt(&mut world, &mut net, &[0], 4, 5, Direction::East);
        // Two belts taking out to East and South
        let belt_out1 = place_belt(&mut world, &mut net, &[0], 6, 5, Direction::East);
        let belt_out2 = place_belt(&mut world, &mut net, &[0], 5, 6, Direction::South);

        assert!(net.connect_belt_to_splitter(belt_in, splitter));
        pool.add_input(splitter, belt_in);
        assert!(net.connect_splitter_to_belt(belt_out1, splitter));
        pool.add_output(splitter, belt_out1);
        assert!(net.connect_splitter_to_belt(belt_out2, splitter));
        pool.add_output(splitter, belt_out2);
        pool.detect_mode(splitter);

        assert_eq!(pool.get(splitter).unwrap().mode, crate::sim::splitter::SplitterMode::Splitter);
    }

    #[test]
    fn splitter_two_in_two_out_balancer() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 5, 5);

        let belt_in1 = place_belt(&mut world, &mut net, &[0], 4, 5, Direction::East);
        let belt_in2 = place_belt(&mut world, &mut net, &[0], 5, 4, Direction::South);
        let belt_out1 = place_belt(&mut world, &mut net, &[0], 6, 5, Direction::East);
        let belt_out2 = place_belt(&mut world, &mut net, &[0], 5, 6, Direction::South);

        assert!(net.connect_belt_to_splitter(belt_in1, splitter));
        pool.add_input(splitter, belt_in1);
        assert!(net.connect_belt_to_splitter(belt_in2, splitter));
        pool.add_input(splitter, belt_in2);
        assert!(net.connect_splitter_to_belt(belt_out1, splitter));
        pool.add_output(splitter, belt_out1);
        assert!(net.connect_splitter_to_belt(belt_out2, splitter));
        pool.add_output(splitter, belt_out2);
        pool.detect_mode(splitter);

        assert_eq!(pool.get(splitter).unwrap().mode, crate::sim::splitter::SplitterMode::Balancer);
    }

    #[test]
    fn splitter_connect_rejects_non_endpoint_belt() {
        // A merged belt in the middle of a line should NOT connect to a splitter
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 7, 5);

        // Place 3 belts going East: (4,5) (5,5) (6,5) — they merge into one line
        let _belt1 = place_belt(&mut world, &mut net, &[0], 4, 5, Direction::East);
        let belt2 = place_belt(&mut world, &mut net, &[0], 5, 5, Direction::East);
        let belt3 = place_belt(&mut world, &mut net, &[0], 6, 5, Direction::East);

        // belt3 is at offset 0 (output end) — should connect
        assert!(net.connect_belt_to_splitter(belt3, splitter));
        // belt2 is in the middle — should NOT connect
        assert!(!net.connect_belt_to_splitter(belt2, splitter));
    }

    #[test]
    fn splitter_connect_rejects_non_open_endpoint() {
        // A belt whose output_end is already Belt should not override with Splitter
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 7, 5);

        // Place 2 belts going East: (5,5) then (6,5) — output is Belt connection
        let _belt1 = place_belt(&mut world, &mut net, &[0], 6, 5, Direction::East);
        let belt2 = place_belt(&mut world, &mut net, &[0], 5, 5, Direction::East);

        // belt2 is NOT at offset 0 (it was merged as upstream), so connect should fail
        // Actually after merge, belt1 is at offset 0 and belt2 is at the input end
        // Let's check: belt1 output_end is Open, belt2's segment is at offset FP_SCALE
        // connect_belt_to_splitter requires offset 0, so belt2 fails
        assert!(!net.connect_belt_to_splitter(belt2, splitter));
    }

    #[test]
    fn disconnect_splitter_ports_clears_belt_ends() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 5, 5);
        let belt_in = place_belt(&mut world, &mut net, &[0], 4, 5, Direction::East);
        let belt_out = place_belt(&mut world, &mut net, &[0], 6, 5, Direction::East);

        assert!(net.connect_belt_to_splitter(belt_in, splitter));
        assert!(net.connect_splitter_to_belt(belt_out, splitter));

        // Both endpoints should be Splitter
        let seg_in = net.segments.get(belt_in).unwrap();
        assert_eq!(net.lines.get(seg_in.line).unwrap().output_end, BeltEnd::Splitter { entity: splitter });
        let seg_out = net.segments.get(belt_out).unwrap();
        assert_eq!(net.lines.get(seg_out.line).unwrap().input_end, BeltEnd::Splitter { entity: splitter });

        // Disconnect
        net.disconnect_splitter_ports(splitter);

        let seg_in = net.segments.get(belt_in).unwrap();
        assert_eq!(net.lines.get(seg_in.line).unwrap().output_end, BeltEnd::Open);
        let seg_out = net.segments.get(belt_out).unwrap();
        assert_eq!(net.lines.get(seg_out.line).unwrap().input_end, BeltEnd::Open);
    }

    #[test]
    fn removing_output_end_belt_clears_splitter_connection() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 5, 5);

        // Place 2 belts going East: (3,5) then (4,5)
        let belt1 = place_belt(&mut world, &mut net, &[0], 3, 5, Direction::East);
        let belt2 = place_belt(&mut world, &mut net, &[0], 4, 5, Direction::East);
        // They merge: belt2 at offset 0 (output end), belt1 at offset FP_SCALE

        assert!(net.connect_belt_to_splitter(belt2, splitter));
        pool.add_input(splitter, belt2);
        pool.detect_mode(splitter);

        // Remove belt2 (output end) — should clear Splitter connection
        net.on_belt_removed(belt2);

        // belt1 is now the only segment; its line's output_end should be Open
        let seg1 = net.segments.get(belt1).unwrap();
        assert_eq!(net.lines.get(seg1.line).unwrap().output_end, BeltEnd::Open);
    }

    #[test]
    fn removing_input_end_belt_clears_splitter_connection() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter = place_splitter(&mut world, &mut pool, &[0], 2, 5);

        // Place 2 belts going East: (4,5) then (3,5) — they merge
        let belt1 = place_belt(&mut world, &mut net, &[0], 4, 5, Direction::East);
        let belt2 = place_belt(&mut world, &mut net, &[0], 3, 5, Direction::East);
        // belt1 at offset 0, belt2 at offset FP_SCALE (input end)

        assert!(net.connect_splitter_to_belt(belt2, splitter));
        pool.add_output(splitter, belt2);
        pool.detect_mode(splitter);

        // Verify input_end is Splitter
        let seg2 = net.segments.get(belt2).unwrap();
        assert_eq!(net.lines.get(seg2.line).unwrap().input_end, BeltEnd::Splitter { entity: splitter });

        // Remove belt2 (input end) — should clear Splitter connection
        net.on_belt_removed(belt2);

        // belt1 is now the only segment; its line's input_end should be Open
        let seg1 = net.segments.get(belt1).unwrap();
        assert_eq!(net.lines.get(seg1.line).unwrap().input_end, BeltEnd::Open);
    }

    #[test]
    fn line_splitter_connections_returns_both() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let mut pool = crate::sim::splitter::SplitterPool::new();

        let splitter_a = place_splitter(&mut world, &mut pool, &[0], 3, 5);
        let splitter_b = place_splitter(&mut world, &mut pool, &[0], 6, 5);

        // Place belt at (4,5) going East, connect to both splitters
        let belt = place_belt(&mut world, &mut net, &[0], 4, 5, Direction::East);

        // Output end faces East toward (5,5) — but splitter_b is at (6,5), not (5,5)
        // So let me adjust: single belt, splitter behind and splitter ahead
        // Actually for a single-segment belt, output=offset 0, input=offset 0 too (same)
        // For a single belt, offset=0 and length=FP_SCALE, so input end is also offset=0
        // Wait, input end is at offset line.length - FP_SCALE = 0. So both ends are the same segment.

        // Let me use connect directly
        assert!(net.connect_belt_to_splitter(belt, splitter_b));
        // For input end, offset must equal length - FP_SCALE = 0, which it does
        assert!(net.connect_splitter_to_belt(belt, splitter_a));

        let (out, inp) = net.line_splitter_connections(belt);
        assert_eq!(out, Some(splitter_b));
        assert_eq!(inp, Some(splitter_a));
    }
}
