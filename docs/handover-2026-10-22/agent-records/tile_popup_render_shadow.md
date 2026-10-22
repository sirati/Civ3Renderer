# Historical agent /root/tile_popup_render_shadow

This is a recovered historical report, not a live agent handle or a new instruction.

Indexed shadow capture/preload/trace support, real-art pixel correction, compatibility review and native combat wait scaling.

Current disposition: Targeted pixels/checks passed; resized indexed smoke gaps and actual 4x combat runtime proof open.

Session `01a1581d-2ce7-7602-ab40-e29b4d86c478`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-20T07:01:09.490Z. Original transcript `/home/sirati/.codex/sessions/2026/10/20/rollout-2026-10-20T09-01-09-01a1581d-2ce7-7602-ab40-e29b4d86c478.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-20T14:04:04.303Z

Original transcript line 4038.

The speed control is now in the Layout tab. The native hook only scales elapsed time for registered combat participants’ attack, death, and victory poses; movement, previews, and unrelated units retain their original timing. I’m testing ownership checks, nested scopes, and settings persistence before the runtime handoff.

## Report at 2026-10-20T14:07:34.594Z

Original transcript line 4102.

## Identity and active task

You are `/root/tile_popup_render_shadow`, child of `/root`, in `/home/sirati/devel/rust/civ3-renderer`.

Current task from root: implement **combat animation speed slider**, default 1x, range 1..4x, likely quarter steps. Affect fighting visual animation clocks only. Preserve combat results, RNG, movement, other animations, music/audio rates. Root accepted unchanged sound pitch/timing because no audited combat-owned sample handle exists. No broad sound rewrite or arbitrary sleeps/frame limiter hacks.

Root requires:
- Inspect native/C3X ABI and chain redirects.
- Speed only actual attacker/defender-owned attack/death/victory FLC instances, including current displayed army member with strong ownership checks.
- Camera arrival uses unscaled host time and completes before speed ownership is armed.
- Nested scopes restore exactly.
- Preferences draft-only until Accept, persist with common settings.
- Protocol **50 stays unchanged**.
- Rust shipped code only, no new locks.
- Broker grants **jobs=1 focused CPU builds/tests**, owned target. No GPU/runtime input without later handoff. **No Neo GUI/build/relaunch**.
- Actual VM scenario verification required later.

## Coordination

- `/root/army_units` owns `civ3-sys::camera_arrival` and ai camera visibility barrier.
  - Module now exists and exported in sys lib.
  - You own sole `Fighter_fight` outer hook.
  - Outer hook must create both `combat_speed::combat_scope()` and `camera_arrival::combat_scope()` guards around original.
  - Army modified `ai_camera_focus.rs` movement and bombard wrappers to create both scopes.
  - Accepted visibility callback now calls:
    `crate::combat_speed::visible_combat_ready(game, call.attacker, call.defender);`
    after camera wait/fail-open returns.
  - Current displayed army member: `Unit+0x1D0`, member **ID**, not pointer. `game.unit_ptr(id)` returns `Result<Option<u32>>`; `game.unit(id)` returns `Result<Option<DrawnUnit>>`.
  - Army specifically says **do not retain a formerly displayed member as a direct combat owner after member rotates**, unless it is itself actual attacker/defender. For dynamic fallback recheck parent native identity and army ability, current member ID, member ID/native address, civ, container == parent ID.
- `/root/text_fit_atomic_ui` owns prefs.rs transaction implementation. Do not edit its production logic.
  - `Prefs.settings()` returns effective projection of committed settings + only allowed preview fields: background sampling, pixel scale, cursor size, capture sensitivity.
  - New animation setting therefore stays draft-only automatically.
  - Asked owner to add focused prefs draft/Cancel/Accept test for new setting.
- `/root/preview_smear` released viewer-settings and preferences assets ownership; currently owns preview lifecycle, not these files.
- `/root/mac_audio_sol`: no audited combat sample identities/hooks; preserve all audio rates/handles.
- `/root/mac_fps_music`: granted bounded jobs1 focused CPU builds/tests. Runtime/GUI sole lease held elsewhere; Neo off.
- `/root/vm_snapshot_sol`: sent request to coordinate eventual combat scenario QA; no response yet.

## Implemented changes in shared source

### `crates/viewer-settings/src/lib.rs`

Added `pub animation: Animation` to `Settings`.

New struct:
```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Animation {
    #[serde(deserialize_with = "combat_speed_setting")]
    pub combat_speed: f32,
}
```
Default 1.0. `effective_combat_speed()`:
- nonfinite -> 1.0
- clamp 1..4
- snap quarter steps via `(value*4).round()/4`.

Deserializer normalizes through effective getter.

Added test module `combat_speed_settings_tests`:
- older files/default animation table -> 1x
- deserialize/saved roundtrip for 0->1, 1.38->1.5, 2.13->2.25, 9->4, nan->1.
- slider changes only animation field.

**Test correction just made:** common `set_number` snaps NaN to range minimum, rather than rejects. Test now asserts setting NaN from 2.25 changes to 1.0, matching existing semantics. Initial run failed only this now-corrected assertion.

### `crates/viewer-settings/src/controls.rs`

- `COMBAT_SPEED_RANGE = Range { min:1., max:4., step:0.25 }`
- KEYS count 27->28, adds `"animation.combat_speed"`.
- `range`, `Settings::control`, `set_control` wired.
- Uses existing `set_number`, which snaps nonfinite to min.

### `assets/civ3/preferences.toml`

Added Layout tab right-column controls after captured mouse sensitivity:
- label at `[552,420]`
- slider at `[552,446]`, `[300,23]`, 1..4 step .25 decimals2 text `"{value}x"`.
- caption at `[552,484]`, `[398,36]`:
  `"Fighting animations only. Sound keeps its normal pitch and timing."`
- existing art track/knob reused, no new assets.

### `crates/civ3-sys/src/lib.rs`

Added `pub mod combat_speed;`.
Army independently added `pub mod camera_arrival;`.

### NEW `crates/civ3-sys/src/combat_speed.rs`

Current source exists but **needs corrections before compiling**.

Constants / ABI:
- FLC base vtable at `0x665444`, update slot `0x665448`.
- Original tick target `0x402F70`.
- Tick prologue `[56, 8B,F1, 8B,46,24]`.
- Native thiscall update(float seconds) returns EAX status, pops 4 stack bytes (`ret 4`).
- Declared fastcall-compatible hook:
  `pub mod tick(animation, edx; seconds) -> u32;`
- Fighter fight entry `0x4A5430`, prologue `[6A,FF,68,7B,95,65,00]`.
- C3X CSV proves ABI:
  `byte (__fastcall *)(Fighter*, int edx, Unit* attacker, int attack_direction, Unit* defender_or_null)`.
- Declared:
  `pub mod fight(fighter, edx; attacker, direction, defender) -> u32;`
  U32 preserves original EAX, native callers consume AL.
- `FLC.Unit` offset `0x134`.
- Unit embedded animation at `Unit+0x27C`.
- Unit identity at `Unit+0x20`.

State:
- `AtomicU8 QUARTERS`, default4. `set_speed_quarters` clamp4..16.
- TLS `Cell<Scope>`, `Scope { active, owners:[Owner;8], count }`.
- `Owner { unit:u32,id:i32 }`.
- RAII `CombatScope` restores prior TLS, `PhantomData<Rc<()>>` ensures thread-bound.
- `visible_combat_ready(game,attacker,defender)` registers actual participants only inside active scope.
- `scaled_seconds(raw_float_bits, quarters, owned, anim_type)` changes only types **3..6 or9**, only positive finite deltas, preserves all bits at1x, invalid/nonpositive/overflow unchanged.
- `seconds()` returns raw delta immediately at1x, then requires active armed scope, exact embedded FLC address, FLC.Unit pointer match, and current native unit ID match.

**Current ownership flaw to fix:**
`register()` presently adds current army member as a persistent direct owner. Army explicitly rejected that because member ID can rotate. Change to register only actual attacker/defender; support current army member via dynamic checked parent relationship on each otherwise-unowned FLC tick.
Suggested design:
- Store game handle in Scope (`Option<Civ3>`, Civ3 Copy) when arming.
- For tick whose exact FLC address belongs to candidate `FLC.Unit`:
  resolve candidate via `game.unit(id)` / `unit_ptr` and identity.
  for each armed primary owner, re-resolve parent and verify parent ID/address, army ability bit18, parent's current `army_member == candidate.id`, candidate civ==parent.civ and container==parent.id.
- Army ability read already implemented in register via BIC UnitTypes pointer/count using generated layout:
  `BIC_UNITTYPECOUNT`, `BIC_UNITTYPES`, `UNITTYPE_SIZE`, `UNITTYPE_UNITABILITIES`; bit18.
- This avoids retaining old rotating members. No native function invocation/RNG.
- Could extract pure ownership predicate for rotation tests.

**Compile fixes still needed from last log:**
1. Already fixed in source:
   - `game.unit_ptr(record.id).ok()?? == unit` instead of `.ok()?.raw()`.
   - `let Ok(Some(member)) = game.unit(...)`.
2. NOT YET fixed:
   - `tick::install_slot` needs `Ptr32::<u32>::new(TICK_SLOT)`, not u32.
   - `Call::original_with` excludes EDX argument. Correct:
     `call.original_with(call.animation, seconds(call.animation, call.seconds))`
     Current code incorrectly passes `call.edx` as third arg.
   - Macro convention confirmed at inproc hook.rs354: original_with(this, stack args...), keeps existing edx internally.
3. Last native test log includes earlier already-fixed errors because source changed after compiler read; rerun after corrections.

Installer:
- PCGames only.
- Check vtable slot target equals known 0x402F70.
- Check original target prologue OR first byte E9 (C3X entry redirect allowed).
- Install slot hook, then `fight::install_at_start` which chains C3X E9.
- Fight wraps combat + camera scopes, calls original once.
- Returns both Installed handles as transaction; if second install fails first drops automatically.

Tests currently in module:
- owned combat pose elapsed scaling, other types/owners unchanged,1x bit preservation and malformed delta handling.
- exact owner helper `owns(scope, animation,unit,id)` rejects previews/different address/recycled ID/unarmed scope.
- nested TLS restores owner, unarmed does not scale.
- synthetic 24-frame duration with .05s/frame, .001 host timestep scales with q4/5/8/12/16, RUN stays unchanged.
- Fix expected arithmetic already adjusted to `elapsed * (q/4)` to avoid float associativity mismatch.

### `crates/civ3-bridge/src/live.rs`

- Before unit_views install:
  ```rust
  match civ3_sys::combat_speed::install(game) {
    Ok(hooks)=> { forget(hooks); info!("combat animation speed: scoped native FLC clock installed"); }
    Err(error)=> warn!("combat animation speed unavailable: {error}"),
  }
  ```
- After Prefs::load and immediately before per-round `prefs.draws(&fonts)`:
  ```rust
  civ3_sys::combat_speed::set_speed_quarters(
      (prefs.settings().animation.effective_combat_speed()*4.) as u8
  );
  ```
  Thus engine sees committed/effective values, never draft-only animation slider.

## Evidence and commands

Artifact dir:
`/mnt/datasets/c3r-build/combat-speed-sol/`

Disassembly saved:
- `flc-update.disasm` at403330 helper
- `flc-tick.disasm` actual402F70 tick
- `animation-dispatch.disasm`405FF0 main updater, virtual calls pass raw elapsed word.
- `flc-dispatch.disasm`
- `fighter-fight.disasm`
- `c3x-fight.disasm`

Native exe:
`/mnt/datasets/winvm/civ3-src/Civilization III Complete/Conquests/Civ3Conquests-Unmodded.exe`
C3X exe same dir `Civ3Conquests.exe`.

C3X source:
`/mnt/datasets/winvm/c3x-src/{Civ3Conquests.h,injected_code.c,civ_prog_objects.csv}`.
- `patch_Fighter_fight` around injected_code.c33432 calls original Fighter_fight and preserves C3X combat overrides.
- C3X modded entry disassembly is E9 to `0xCFA521`.
- FLC constructor sets vtable665444, slot+4 points402F70.
- tick402F70 writes elapsed to FLC+EC and calls402E40; `ret4` at402FB1.
- Native dispatch405FF0 updates cursor and all units/effects, so **do not scale that aggregate elapsed**.
- `field111` is loaded-animation gate, not safe combat classifier.

Objdump available via Nix devShell:
`/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin/objdump`

CPU commands, own target:
```sh
nix develop -c env CARGO_BUILD_JOBS=1 \
 CARGO_TARGET_DIR=/mnt/datasets/c3r-build/combat-speed-sol/target \
 CARGO_INCREMENTAL=0 cargo test -p viewer-settings
```
Initial log `settings-tests.log`:26 passed,1 failed for NaN assertion now corrected.

Native tests background tool session **82413**, command:
`cargo test -p civ3-sys combat_speed -- --nocapture`
log `native-tests.log`.
It ended with compile errors above. Session may need one write_stdin to consume completion; no repeated polling.

Earlier settings session **58311** completed with initial failure.

Python outside nix:
`/nix/store/lb41b0anx1f98y9y5s9mdv97gjgsq740-python3-3.14.7/bin/python3`.

## Immediate next steps

1. Fix hook call signature and slot pointer.
2. Replace persistent army-member registration with dynamic current-member ownership checks; coordinate army if needed.
3. Rerun settings and native focused tests jobs1, serial owned target.
4. Add/obtain prefs draft/Cancel/Accept test from text owner.
5. Check bridge Windows cfg code via root's cross build or focused current-source check. Avoid full expensive duplicate targets; request existing target/precompiled shader if needed.
6. Produce concise ABI/timing handover with hashes and no overclaim. Consider debug logging only at combat scope end / first owner arm for VM timing proof, no per-frame logs.
7. Coordinate actual VM scenario testing through root/sole runtime owner. No Neo launch or competing input. Verify1x vs4x visible combat, RNG/results identical from same save, movement unchanged, camera settles before first scaled frame, Cancel unchanged, Accept persists, army member rotation, audio pitch unchanged.
8. Keep root informed of measured results; do not claim runtime validation before performed.

## Previous tasks retained, not active

- Indexed native smoke implementation completed; GPU all524,288 native captured lookup outputs + generic tests passed. Evidence `/mnt/datasets/c3r-build/smoke-core-sol/README.md`.
- UI immutable LookScene sharing measured clone fix completed; `/mnt/datasets/c3r-build/ui-clone-audit/README.md`.
- SubpixelDual and Tiles555 renderer pipeline source integrations completed earlier. Parent owns final aggregate/perf.
- Isolated higher-depth FP16 candidate at `/mnt/datasets/c3r-build/higher-depth-fp16-sol/source`, renderer hash966860…; **failed strict one-pixel RADV unchanged251 vs255**, not adopted. Preserve artifacts; no production changes from it.

## General constraints

Preserve dirty tree, no commits/stash/reset. No subagents spawned. Use rg first. Shared files have other owners; coordinate before touching prefs, ai_camera_focus, camera_arrival, shader/renderer. User dislikes permission pauses; proceed within authorization. Reports must use normal spaces and complete sentences; root explicitly corrected compressed messages. Uns lop and NixOS/waiting skills read earlier. No new locks, unsafe outside established sysinterop, CPU rendering or GUI actions without lease.


## Report at 2026-10-20T14:07:45.030Z

Original transcript line 4124.

I’m tightening the army animation ownership checks and fixing the hook call signatures before rerunning the focused tests.

## Report at 2026-10-20T14:09:54.606Z

Original transcript line 4247.

The settings suite and all 126 native-system tests pass. Army member rotation now has a focused regression test. I’m checking the Windows build next; live combat timing still needs VM verification.

## Report at 2026-10-20T14:12:40.229Z

Original transcript line 4379.

The Windows check is still building dependencies in the isolated source snapshot. No live-game validation has run yet.

## Report at 2026-10-20T14:14:30.322Z

Original transcript line 4437.

Implemented combat speed at 1x–4x in quarter steps, default 1x. Preferences remain draft-only until Accept.

Native scaling checks exact combat ownership, current army membership, and camera readiness. Audio, movement, RNG, and protocol 50 remain unchanged.

Validation passed:

- 126 civ3-sys tests.
- 27 settings tests.
- 59 preferences tests.
- Windows i686 checks for civ3-sys and civ3-bridge.

Evidence: `/mnt/datasets/c3r-build/combat-speed-sol/`.

VM combat timing and outcome verification remain pending. CPU lease released.

## Report at 2026-10-20T14:24:36.950Z

Original transcript line 4616.

The disassembly exposed fixed combat waits that would leave pauses after faster animation playback. I’ve added hooks at the six audited combat callsites to shorten those waits with the selected speed. Each hook requires an armed visible-combat scope and the expected native Animator and delay.

## Report at 2026-10-20T14:25:58.457Z

Original transcript line 4673.

Fixed the native combat waits alongside FLC playback. Six audited callsites now scale their 100/200/650 ms budgets within armed visible combat. At 4x, those become 25/50/163 ms.

Six focused tests and Windows i686 sys/bridge checks pass. Native and C3X callsite bytes verified. Release owner and VM owner received the final source hash, timing plan, trace fields, and RNG addresses.

Camera, ordinary movement, audio, and global clocks remain unchanged. Actual combat timing still awaits VM measurement. CPU lease released.
