# Octofact — Belt System Improvements

## Phase B1: Perpendicular Belt Side-Loading

> Belts that dead-end into the side of a perpendicular belt should automatically deposit items onto it.

### Context

Currently, `on_belt_placed` only merges consecutive same-direction belts. A belt
whose output end faces the side of a perpendicular belt gets `BeltEnd::Open` and
items pile up at the dead end. Players expect items to flow onto the crossing belt
the way a T-junction works in Factorio.

### Tasks

- [x] Add `BeltEnd::SideInject { entity: EntityId }` variant — items side-inject at the target entity's segment center (offset resolved at tick time for robustness)
- [x] In `on_belt_placed`, after checking same-direction neighbors, check whether the cell ahead contains a perpendicular belt. If found, create a SideInject link from the new belt's line output to the target belt entity
- [x] In `on_belt_placed`, also check whether any perpendicular belt in an adjacent cell has its output aimed at the newly placed belt — if so, create a SideInject link from that belt's line to the new belt
- [x] Implement side-injection transfer in `tick()`: items at pos=0 on a line with `SideInject` output try to insert at the target offset if there's room (gap check against neighbors at that offset position)
- [x] Add `can_accept_at_offset(offset: u32) -> bool` to `TransportLine` — checks whether there is room at the given position (MIN_ITEM_GAP from nearest items on either side)
- [x] Add `insert_at_offset(item: ItemId, offset: u32)` to `TransportLine` — inserts an item at the specified position, maintaining sorted order
- [x] Handle belt removal: when a belt with a SideInject connection is removed, clean up the link (set back to Open); when the target perpendicular belt is removed, find and clean up any SideInject links pointing to its old line
- [x] Handle line splits: when output-end segment is removed, SideInject is cleared (new output-end segment may not be adjacent to target)
- [x] Unit tests: T-junction (East belt dead-ends into North belt), items transfer at correct offset
- [x] Unit tests: reverse T-junction (place perpendicular belt first, then dead-ending belt)
- [x] Unit tests: removal of either belt cleans up SideInject links
- [x] Unit tests: items respect MIN_ITEM_GAP when side-injecting (don't overlap existing items)
- [x] Unit tests: side-injection blocks when target belt segment is full (items back up on source belt)

### Design Notes

**Why SideInject instead of rerouting to input end:** The perpendicular belt's input
end may be far away. Items should appear at the exact grid cell where the two belts
meet, not teleport to the far end of the target line.

**Priority:** When both a side-injecting belt and the perpendicular belt's own
upstream items compete for the same slot, the perpendicular belt's own items take
priority (they're already on the line and advancing). Side-injection only succeeds
if there's a gap at the injection point.

**Orientation rules:** A belt going East whose output cell contains a belt going
North or South qualifies. A belt going East whose output cell contains a belt going
West does NOT qualify (that's head-on, not a T-junction — items would collide).


## Phase B2: Splitter Building

> A 1x1 building that serves as a merger, splitter, or balancer depending on how belts connect to its four sides.

### Context

The game currently has no way to combine or divide item flows. Splitters are a
fundamental logistics building in any factory game. Rather than separate merger /
splitter / balancer buildings, a single universal junction building adapts its
behavior based on which sides have input vs output belts.

### Phase B2a: Data Model & Placement

- [x] Add `Splitter` variant to `ItemId` enum — new infrastructure item
- [x] Add display_name ("Splitter"), description, icon_params (Octagon shape, distinct colors) to ItemId impl
- [x] Add `Splitter` variant to `StructureKind` enum with 1x1 footprint
- [x] Wire `StructureKind::from_item(ItemId::Splitter)` → `StructureKind::Splitter`
- [x] Add `SplitterPool` struct in new file `src/sim/splitter.rs`:
  ```
  SplitterPool {
      entities: SlotMap<SplitterId, SplitterState>,
      entity_map: HashMap<EntityId, SplitterId>,
  }
  SplitterState {
      entity: EntityId,
      inputs: SmallVec<[TransportLineId; 3]>,   // lines feeding in
      outputs: SmallVec<[TransportLineId; 3]>,   // lines feeding out
      mode: SplitterMode,                         // auto-detected
      round_robin_idx: usize,                     // for fair distribution
  }
  SplitterMode { Merger, Splitter, Balancer, Inactive }
  ```
- [x] Register splitter in world placement flow (app.rs): on place, create SplitterPool entry
- [x] On removal, clean up SplitterPool entry and disconnect belt links
- [x] Add Splitter to build menu / inventory (start with 100 in debug inventory)

### Phase B2b: Belt Connection Logic

- [x] Add `BeltEnd::Splitter { entity: EntityId }` variant — a belt endpoint connected to a splitter
- [x] When a belt is placed adjacent to a splitter, detect whether the belt feeds into or out of the splitter:
  - Belt direction points toward the splitter cell → input (belt output_end = Splitter)
  - Belt direction points away from the splitter cell → output (belt input_end = Splitter)
- [x] When a splitter is placed, scan all 4 adjacent cells for existing belts and connect them
- [x] After connections change, auto-detect `SplitterMode`:
  - 0 inputs or 0 outputs → Inactive
  - 2-3 inputs, 1 output → Merger
  - 1 input, 2-3 outputs → Splitter
  - 2 inputs, 2 outputs → Balancer
  - Other combinations (e.g., 3 inputs 2 outputs) → treat as Merger on inputs, round-robin on outputs
- [x] When a belt adjacent to a splitter is removed, update the splitter's connection list and re-detect mode
- [x] Unit tests: placing splitter then belts connects correctly
- [x] Unit tests: placing belts then splitter connects correctly
- [x] Unit tests: mode auto-detection for all valid configurations

### Phase B2c: Simulation Tick

- [x] Implement `SplitterPool::tick(belt_network)` — called each sim tick after belt advance:
  - **Merger mode:** Round-robin pull from input lines. Take item from front of next input line (pos=0), push to output line input end. Advance round_robin_idx.
  - **Splitter mode:** Take item from single input line (pos=0), push to next output line (round-robin). If target output is full, try next output. If all full, item stays on input belt.
  - **Balancer mode:** Alternate between input lines, alternate between output lines. Pull from input A → push to output A, pull from input B → push to output B. If one side backs up, overflow to the other.
- [x] Throughput: splitter transfers at belt speed (1 item per tick if gap allows), not faster
- [x] Wire `SplitterPool::tick()` into the main simulation loop (after belt tick, before machine tick)
- [x] Unit tests: merger — 2 input belts with items, 1 output belt, items alternate fairly
- [x] Unit tests: splitter — 1 input belt, 2 output belts, items distributed round-robin
- [x] Unit tests: balancer — 2 in 2 out, items balanced
- [x] Unit tests: backpressure — when output belt is full, items back up correctly
- [x] Unit tests: throughput — splitter doesn't exceed belt speed

### Phase B2d: Rendering & UI

- [x] Add `SplitterInstance` to instance buffer system or reuse MachineInstance with a splitter type
- [x] Write splitter visuals in machine.wgsl (or new splitter.wgsl): distinct appearance showing directional arrows or connection indicators based on mode
- [x] Build splitter instances each frame from SplitterPool state
- [x] Click-to-inspect UI: show mode (Merger/Splitter/Balancer), connected belt count, throughput stats
- [x] Add Splitter recipe to crafting (e.g., Composer: 2x Identity → Splitter, or 4x LineSegment → Splitter)


## Phase B3: Storage Building

> A 2x2 building that buffers up to 20 stacks of items, with 2 input and 2 output ports.

### Context

Players need item buffers to smooth out production imbalances and to stockpile
resources. Storage acts like a chest in Factorio — items flow in from belts, get
stored, and flow out on demand. At 20 stacks it holds a meaningful buffer without
being infinite.

### Phase B3a: Data Model & Placement

- [x] Add `Storage` variant to `ItemId` enum — new infrastructure item
- [x] Add display_name ("Storage"), description ("Buffered vault. Stores up to 20 stacks of items."), icon_params to ItemId impl
- [x] Add `Storage` variant to `StructureKind` enum with 2x2 footprint
- [x] Wire `StructureKind::from_item(ItemId::Storage)` → `StructureKind::Storage`
- [x] Define `StoragePool` struct in new file `src/sim/storage.rs`:
  ```
  StoragePool {
      entities: SlotMap<StorageId, StorageState>,
      entity_map: HashMap<EntityId, StorageId>,
  }
  StorageState {
      entity: EntityId,
      slots: [ItemStack; 20],     // 20 storage slots
      input_filter: Option<ItemId>,  // optional: only accept this item (future)
  }
  ```
- [x] Define `STORAGE_SLOTS: usize = 20` and `STORAGE_STACK_SIZE: u16 = 50` constants
- [x] Register storage in world placement flow
- [x] On removal, return stored items to player inventory (or drop — decide policy)
- [x] Add Storage to build menu / inventory

### Phase B3b: Port Definitions

- [x] Add storage port layout to `inserter.rs` (or a new `port_layout` match arm):
  - 2x2 footprint, canonical facing North:
    - Input 0: South side, slot 0, cell (0, 1)
    - Input 1: South side, slot 1, cell (1, 1)
    - Output 0: North side, slot 0, cell (0, 0)
    - Output 1: North side, slot 1, cell (1, 0)
  - This gives 2 inputs on the back, 2 outputs on the front — rotation works via existing port rotation system
- [x] Extend `port_layout()` to handle Storage (requires accepting StructureKind or a new StorageType, since Storage isn't a MachineType)
- [x] Belt-to-storage and storage-to-belt connections use the same `BeltEnd::MachineInput` / `BeltEnd::MachineOutput` mechanism, or introduce `BeltEnd::StorageInput` / `BeltEnd::StorageOutput` variants
- [x] Unit tests: port positions for all 4 rotations
- [x] Unit tests: belt compatibility with storage ports

### Phase B3c: Simulation Tick

- [x] Implement `StoragePool::accept_input(entity, slot, item, count) -> bool` — try to store items:
  - Find first slot with matching item and room, or first empty slot
  - Stack up to STORAGE_STACK_SIZE per slot
  - Return false if all 20 slots are full
- [x] Implement `StoragePool::provide_output(entity, slot) -> Option<ItemId>` — take one item for output:
  - Scan slots for any non-empty stack
  - Prefer round-robin across slots to drain evenly
  - Remove 1 from the stack, return the ItemId
- [x] Wire storage I/O into `tick_port_transfers()` in belt.rs — same pattern as machine ports:
  - Belt → Storage: items at pos=0 with StorageInput endpoint transfer into storage
  - Storage → Belt: storage provides items to belt input end with StorageOutput endpoint
- [x] Unit tests: items flow into storage from belt
- [x] Unit tests: items flow out of storage onto belt
- [x] Unit tests: storage fills up and backpressures input belt
- [x] Unit tests: storage empties and output belt starves
- [x] Unit tests: mixed item types stored in separate slots
- [x] Unit tests: full round-trip — belt → storage → belt

### Phase B3d: Rendering & UI

- [x] Add storage to instance buffer system (reuse MachineInstance or create StorageInstance)
- [x] Write storage visuals: 2x2 building with distinct appearance (crate/chest look), fill indicator based on how full the storage is
- [x] Click-to-inspect UI: show all 20 slots with item icons and counts, total capacity bar
- [x] Add Storage recipe to crafting (e.g., Composer: 4x Square → Storage)


## Dependencies

```
Phase B1 (side-loading) ─── no dependencies, can start immediately
Phase B2 (splitters)    ─── no hard dependency on B1, but B1's SideInject
                             mechanism informs the design
Phase B3 (storage)      ─── no dependency on B1 or B2, uses existing
                             port system from Phase 4/7
```

All three phases are independent and can be worked on in parallel (in separate worktrees).
