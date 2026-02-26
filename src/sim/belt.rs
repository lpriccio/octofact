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
    }

    /// Run one simulation tick for all transport lines.
    pub fn tick(&mut self) {
        let line_ids: Vec<TransportLineId> = self.lines.keys().collect();

        // Phase 1: Transfer items at output ends to connected inputs.
        let mut transfers: Vec<(TransportLineId, TransportLineId)> = Vec::new();

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
                if let BeltEnd::Belt(target_id) = line.output_end {
                    if let Some(target) = self.lines.get(target_id) {
                        if target.can_accept_at_input() {
                            transfers.push((line_id, target_id));
                        }
                    }
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
    fn blocked_output_stops_items() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        // Two belts: e1 East, e2 North — different directions, separate lines.
        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let _e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);

        net.spawn_item_on_entity(e1, ItemId::NullSet);

        for _ in 0..500 {
            net.tick();
        }

        // Item stuck at output end (open, since e2 is different direction).
        let items = local_items(&net, e1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1, 0);
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
}
