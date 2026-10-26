Keyed-read fixtures, 26 October 2026

Source records: /tmp/c3r-live-integrate/owned/civ3-bridge.log, queries at
54.735 seconds (HUD) and 146.751 seconds (F1 city row).

HUD canvas 0x9fdd84 (1920x1080), query (1850,1010):
- key clear at [1621,966,294,64]
- box right color.pcx [0,24,294,64] at (1621,966), premultiplied mask
  box right alpha.pcx at (0,24).
- WorkerDefault.flc chunk 33 at (1729,969), clip [1180,966,1920,1030].
The queried PCX sample is (229,68): colour index 230, RGB [255,255,232],
mask index 0. The FLC sample is (121,41), outside its 50x51 file extent.
The test constructs frame headers with that extent, omitting unqueried pixel
bodies. It verifies that the model only becomes answerable when the viewer's
resolved source identity connects to the decoded animation cache.
PCX SHA256:
8f76ed3f42830a04a7a5efb5466fa59829d1cfa233fecd404c156f20aeaf5976 colour
4144e5036566729e32cceec07c4450392e4cbf243f4980e8dcaaa39c837879c2 alpha

F1 canvas 0xa38e4c (1024x768), query (132,324):
The relevant text is "Alcácer do...", Lucida Sans -12, italic, black,
canvas baseline (111,329), placed on screen at (559,485). The model query
corresponds to screen (580,480). first-city-glyphs.toml retains only the
characters in this caption from the renderer's measured dark/light atlas,
with their metrics. The test builds the normal atlas source through
Fonts::publish_hit and checks the queried point and nearby ink/empty pixels
at both coordinate systems. It needs no installation, GDI drawing or scratch
canvas. It tests the atlas path; live Wine uses the shared outline path too.
Original atlas SHA256:
56dab0755b0290d3bf62b91d9779fc633c82a1168f417bf4a9d0c97c1e1893d0 dark
f7217f493ac1ba9ff4fae004dc8aad8721b1ef484a6d44c0481bb0e38b75d41b light
f79dd2994376abfff205bd736329fc5dffe1694ca21b19aca76aa364a27d849f metrics
