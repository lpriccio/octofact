use slotmap::{new_key_type, SlotMap, SecondaryMap};

use crate::game::items::ItemId;
use crate::game::world::{Direction, EntityId, StructureKind, WorldState};

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

    /// Advance N ticks (for chunk fast-forward).
    #[allow(dead_code)]
    pub fn fast_forward(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.tick();
        }
    }
}

/// Check if a grid position is within a tile's bounds (-32..=32 on each axis).
fn is_within_tile(gx: i32, gy: i32) -> bool {
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
}
