# Per-view layer data

Every view draws the same layers, but a view can draw some of them from
other cell data. The fog of war uses this now: while the city screen is open,
its view shows the city screen's dimming and every other view keeps the
player's fog. Co-op later needs the same mechanism, with some layers shared
by every player and some kept per player.

## The renderer (`tile-render`, game agnostic)

A layer's cell records live in a **data slot**, a region of the one cell-data
storage buffer. Each layer has its own region, the *shared* slot. A
`TileWorld` can hold more slots for a layer (`add_slot`, `set_in`,
`set_phase_in`, `set_offset_in`). A slot has the layer's record format and one
record per cell.

A **binding** says which slot each layer is drawn from (`TileWorld::bind`).
Layers it does not list use their shared slot. A camera names its binding
(`Camera::binding`). Binding 0 (`SHARED_BINDING`) uses every shared slot, and
so does a binding the world does not have. Cameras that bind the same slot
read the same records. Nothing is copied, and binding or unbinding writes no
cell data.

On the GPU, the layer table (`LayerDesc`s) appears once per binding. The
shared table comes first, and each binding's copy differs only in the
`data_byte_offset` of the layers it binds. `FrameParams::layer_table` is the
start of the view's table. The vertex shader reads
`layers[layer_table + layer]` and then the records at that layer's offset.
The CPU reference renderer does the same through `table_of`. GPU-vs-reference
tests cover two views with different slots, with the same slot, and with
slots changing incrementally (`tile-render/tests/per_view_layers.rs`).

Costs:

- A slot costs its records (cells × record bytes). A binding costs one layer
  table (layers × 64 bytes).
- `free_slot` unbinds the slot everywhere. The freed region is reused by the
  next slot of the same size, and it is released when no later slot is live.
- A new slot marks its whole region dirty (`Dirty::Records` with
  `ALL_CELLS`). Its cells then update record by record like any layer's.
  Binding changes mark only the tables dirty.
- `TileRenderer::update_cells` writes changed records, and the layer tables
  when they change, with queued buffer writes. The cell buffer has spare
  room, so a new slot usually writes only its own records. When a slot
  outgrows the buffer, the buffer is replaced and written in full. The GPU
  never waits for the CPU, and the CPU never maps a buffer the GPU uses.

## The scene (`scene`, wire protocol 41)

- `Scene::layers` holds the shared cells, as before.
- `Scene::slots` holds `LayerSlot { id, layer, cells }`: other cells for one
  layer, chosen by the driver.
- `Scene::bindings` holds `Binding { id, slots: [(layer, slot id)] }`.
- `View::binding` names the binding a view draws with (0: shared).
- Updates: `Slot { id, slot }` adds, replaces or removes a slot.
  `SlotCells` carries changed values. `Bindings` replaces the bindings.
  `updates_since` sends only changed slot values while a slot's layer and
  shape stay the same.
- `Changes::slots` reports that the set of slots or bindings changed, and
  `Changes::slot_cells` lists changed slot cells.
- A driver sends a slot only when a view actually differs. With no slots,
  every view binds the shared data.

The viewer (`tile-viewer`) mirrors this. `scene_cameras` copies
`View::binding` to the view's camera, so cameras derived from it (stacked,
fitted into a pane) inherit the binding. The viewer's own views (second pane,
insets, magnifier) use the shared binding. `scene_slots` rebuilds the world's
slots and bindings when the set of slots changes, and
`scene_slot_cells_changed` applies cell changes.

## Use now: the city screen's fog

`civ3_bridge::scene_map::Mapper::scene` always writes the player's fog into
the shared `fog` and `fog_edge` layers. When a city is in focus, it also
computes the city screen's fog (`rules::fog_with`, docs §16.8). Each fog layer
whose cells differ goes into a slot (`CITY_FOG_SLOTS`), and those slots are
bound to `CITY_BINDING`. The city screen's map view (`CITY_VIEW`, set by
`city_view`) uses `CITY_BINDING`. With the city screen in pane A, A shows the
dimming and pane B shows the normal fog. When the screen closes, the slots
and the binding disappear, and the viewer frees them.

## Co-op model (not implemented)

The game keeps one world, but each player sees it as of their last map
update. The renderer can reproduce that with one slot per player per layer:

| Data | Where | Why |
| --- | --- | --- |
| base terrain, rivers, polar caps, terrain changes (climate change, global warming) | shared slots | the game updates terrain for everyone, even without map trading |
| overlays a player knows: roads, railroads, mines, irrigation, forest and jungle clearing, cities as last seen, pollution, borders | one slot per player | updated only by that player's sight or map trading |
| visibility and fog of war (`fog`, `fog_edge`) | one slot per player (plus per-screen slots such as the city screen's) | each player's explored and visible tiles |
| units and labels | per player (slots for cell layers; free sprites per view, as now) | what each player sees |

Each player gets a binding that lists its slots. Every view of that player
(main view, panes, insets, the player's screens) uses that binding. A screen
that differs again, such as one player's city screen, gets a binding that
reuses the player's slots and adds its own fog slot. Slots are shared
wherever data is equal, so terrain is stored once. The game's own shortcut
carries over directly: terrain changes are global and overlays change only
after a map trade.

The scene can already carry this. A co-op driver sends each player's slots
and bindings and gives each view its player's binding. These parts are still
missing:

- Viewer-owned views (second pane, insets, magnifier) always use the shared
  binding. In co-op they need the binding of the player they belong to,
  which could come from a per-player view or a `Scene` field.
- Free sprites (units, labels) are filtered per view (`PlacedSprite::view`),
  not per binding. Per-player sprites would be sent per view, or a binding
  filter could be added to free sprites.
- A completely different world per camera (another grid shape) is not
  supported. A slot has its layer's grid. That would take a second
  `TileWorld` drawn by its own `TileRenderer` pass, which the current design
  does not prevent.
