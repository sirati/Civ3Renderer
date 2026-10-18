# Clean map per view

Civ III's Clean Map command and its original Clean Map Preferences now affect
views that use **Game** mode. Checked original preferences take effect while
Clean Map is enabled. Toggling Clean Map off restores those views.

The renderer's Preferences screen has a **Clean map** tab with separate columns
for A, B, and the zoom view. Each column offers:

- **Game** follows Civ III's current Clean Map mode and checked preferences.
- **Normal** always shows the map's normal detail.
- **Custom** hides that column's checked categories, independently of the game
  and other views. Checking a category selects Custom for that view.

All thirteen original categories are available. Cities include their labels
and map icons. Units include their bodies, team discs, selection indicators,
shadows, and status graphics. Hills have a separate category from mountains.

Animated effects keep the original game's behavior and remain visible.

Fog of war and the viewing player's unit visibility always apply. Clean map
changes the drawing in each camera. It does not erase terrain, cities, or units
from the shared world or change a game asset.

The choices persist in the existing `settings.toml`, for example:

```toml
[clean_map.a]
mode = "Inherit"
hidden = 0

[clean_map.b]
mode = "Custom"
hidden = 4128 # roads/railroads + units

[clean_map.zoom]
mode = "Normal"
hidden = 0
```

`hidden` uses bits 0 through 12, in the original dialog's order: rivers/flood
plains, mountains, hills, forests/jungles, terrain buildings, roads/railroads,
irrigation, natural resources, food/shield icons, pollution, territory
boundaries, cities, units. Settings files created before these controls existed
continue to load with all views following Game mode.
