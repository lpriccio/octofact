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

/// A single transport line (one belt segment).
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

/// The belt simulation network — manages all transport lines.
pub struct BeltNetwork {
    lines: SlotMap<TransportLineId, TransportLine>,
    entity_to_line: SecondaryMap<EntityId, TransportLineId>,
}

impl BeltNetwork {
    pub fn new() -> Self {
        Self {
            lines: SlotMap::with_key(),
            entity_to_line: SecondaryMap::new(),
        }
    }

    /// Called after a belt entity is placed in the world.
    /// Creates a transport line and links to same-direction neighbors within the tile.
    pub fn on_belt_placed(
        &mut self,
        entity: EntityId,
        tile: &[u8],
        gx: i32,
        gy: i32,
        direction: Direction,
        world: &WorldState,
    ) {
        let line_id = self.lines.insert(TransportLine::new(FP_SCALE));
        self.entity_to_line.insert(entity, line_id);

        let (dx, dy) = direction.grid_offset_i32();

        // Link to upstream neighbor (behind us — items flow FROM there TO us)
        let behind = (gx - dx, gy - dy);
        if is_within_tile(behind.0, behind.1) {
            if let Some(behind_entity) = find_belt_at(tile, behind, direction, world) {
                if let Some(&behind_line) = self.entity_to_line.get(behind_entity) {
                    if let Some(bl) = self.lines.get_mut(behind_line) {
                        bl.output_end = BeltEnd::Belt(line_id);
                    }
                    if let Some(our) = self.lines.get_mut(line_id) {
                        our.input_end = BeltEnd::Belt(behind_line);
                    }
                }
            }
        }

        // Link to downstream neighbor (ahead of us — items flow FROM us TO there)
        let ahead = (gx + dx, gy + dy);
        if is_within_tile(ahead.0, ahead.1) {
            if let Some(ahead_entity) = find_belt_at(tile, ahead, direction, world) {
                if let Some(&ahead_line) = self.entity_to_line.get(ahead_entity) {
                    if let Some(our) = self.lines.get_mut(line_id) {
                        our.output_end = BeltEnd::Belt(ahead_line);
                    }
                    if let Some(al) = self.lines.get_mut(ahead_line) {
                        al.input_end = BeltEnd::Belt(line_id);
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

    /// Debug: spawn an item at the center of the belt entity's line.
    pub fn spawn_item_on_entity(&mut self, entity: EntityId, item: ItemId) {
        if let Some(&line_id) = self.entity_to_line.get(entity) {
            if let Some(line) = self.lines.get_mut(line_id) {
                let pos = line.length / 2;
                let idx = line.items.partition_point(|i| i.pos < pos);
                line.items.insert(idx, BeltItem { item, pos });
            }
        }
    }

    /// Get the items on the transport line for a belt entity.
    pub fn entity_items(&self, entity: EntityId) -> Option<&[BeltItem]> {
        let &line_id = self.entity_to_line.get(entity)?;
        let line = self.lines.get(line_id)?;
        Some(&line.items)
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

    #[test]
    fn single_item_moves_toward_output() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let e = place_belt(&mut world, &mut net, &[0], 0, 0, Direction::East);

        net.spawn_item_on_entity(e, ItemId::NullSet);
        let start = net.entity_items(e).unwrap()[0].pos;
        assert_eq!(start, FP_SCALE / 2);

        net.tick();

        let after = net.entity_items(e).unwrap()[0].pos;
        assert_eq!(after, start - DEFAULT_BELT_SPEED as u32);
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

        let items = net.entity_items(e).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].pos, 0);
    }

    #[test]
    fn items_compress_behind_blocked_front() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let e = place_belt(&mut world, &mut net, &[0], 0, 0, Direction::East);

        // Manually insert two items
        let line_id = *net.entity_to_line.get(e).unwrap();
        let line = net.lines.get_mut(line_id).unwrap();
        line.items.push(BeltItem { item: ItemId::NullSet, pos: 100 });
        line.items.push(BeltItem { item: ItemId::Point, pos: 200 });

        for _ in 0..1000 {
            net.tick();
        }

        let items = net.entity_items(e).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].pos, 0);
        assert_eq!(items[1].pos, MIN_ITEM_GAP);
    }

    #[test]
    fn chain_transfer_between_belts() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];
        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);

        // Verify linking
        let l1 = *net.entity_to_line.get(e1).unwrap();
        let l2 = *net.entity_to_line.get(e2).unwrap();
        assert_eq!(net.lines.get(l1).unwrap().output_end, BeltEnd::Belt(l2));
        assert_eq!(net.lines.get(l2).unwrap().input_end, BeltEnd::Belt(l1));

        net.spawn_item_on_entity(e1, ItemId::NullSet);

        for _ in 0..500 {
            net.tick();
        }

        // Item should have transferred to second belt
        let items1 = net.entity_items(e1).unwrap();
        let items2 = net.entity_items(e2).unwrap();
        assert_eq!(items1.len(), 0);
        assert_eq!(items2.len(), 1);
        assert_eq!(items2[0].item, ItemId::NullSet);
    }

    #[test]
    fn fast_forward_advances_correctly() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let e = place_belt(&mut world, &mut net, &[0], 0, 0, Direction::East);

        net.spawn_item_on_entity(e, ItemId::NullSet);
        let start = net.entity_items(e).unwrap()[0].pos;

        net.fast_forward(10);

        let after = net.entity_items(e).unwrap()[0].pos;
        assert_eq!(after, start - 10 * DEFAULT_BELT_SPEED as u32);
    }

    #[test]
    fn linking_order_does_not_matter() {
        // Place second belt first, then first — links should still form
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);
        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);

        let l1 = *net.entity_to_line.get(e1).unwrap();
        let l2 = *net.entity_to_line.get(e2).unwrap();
        assert_eq!(net.lines.get(l1).unwrap().output_end, BeltEnd::Belt(l2));
        assert_eq!(net.lines.get(l2).unwrap().input_end, BeltEnd::Belt(l1));
    }

    #[test]
    fn different_directions_do_not_link() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let _e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::North);

        let l1 = *net.entity_to_line.get(e1).unwrap();
        assert_eq!(net.lines.get(l1).unwrap().output_end, BeltEnd::Open);
    }

    #[test]
    fn blocked_output_stops_transfer() {
        let mut world = WorldState::new();
        let mut net = BeltNetwork::new();
        let addr: &[u8] = &[0];

        let e1 = place_belt(&mut world, &mut net, addr, 0, 0, Direction::East);
        let e2 = place_belt(&mut world, &mut net, addr, 1, 0, Direction::East);

        // Fill e2 with items so it can't accept more
        let l2 = *net.entity_to_line.get(e2).unwrap();
        let line2 = net.lines.get_mut(l2).unwrap();
        // Pack items fully: at positions 0, 64, 128, 192, 256
        // Last item at input end (256) leaves gap 0 — can't accept
        for i in 0..5 {
            line2.items.push(BeltItem {
                item: ItemId::Point,
                pos: i * MIN_ITEM_GAP,
            });
        }

        // Spawn item on e1
        net.spawn_item_on_entity(e1, ItemId::NullSet);

        // Tick until e1's item reaches position 0
        for _ in 0..500 {
            net.tick();
        }

        // Item should be stuck at position 0 on e1 (can't transfer to full e2)
        let items1 = net.entity_items(e1).unwrap();
        assert_eq!(items1.len(), 1);
        assert_eq!(items1[0].pos, 0);
    }
}
