# Game loop step 19: tree playback, completion and original time units

This slice ports the tree-family animation/removal path and corrects two game-loop fidelity errors.
Evidence is Steam 1.0.7 Build 170910 Application/Runtime x64 Ghidra pseudocode, focused SSE
disassembly, complete local resource decoding, and original-function execution under Unicorn.
Native GPU tests/captures execute the remake. No running original settlement or pixel comparison
was captured; shared animation instances, blend stacks and spatial audio remain open.

## Time and loop-order corrections

Runtime `System::Timer::Startup` (`0x1800cd030`) sets **1,000,000 ticks/second** and seconds-per-tick
to `1.0 / 1,000,000`. `CurrentTime` (`0x1800ccfd0`) normalizes the platform performance counter
to those units. The step-18 interpretation as machine-specific performance-counter ticks was
incorrect. `scheduler.rs` now uses the original microsecond units; `growth.py` invokes the
original Timer startup with only its platform setup stubbed, then regenerates all growth fixtures.

Time construction still divides f32 seconds using f64 and truncates (`0x1800078c0`). A requested
f32 delta need not convert to the nearest microsecond. The new calendar retains integer elapsed
ticks, matching the calendar object at Game `+0x128`; it no longer accumulates rounded f32 deltas.

`FUN_14009b850` establishes this simulation order, confirmed through bounded emulation:

1. Map update, then AnimationManager update. The root-only flag is true for every iteration except
   the last at accelerated speeds.
2. Entity manager: bit-0x04 component updates, scheduled buckets, deferred removal.
3. Calendar tick addition and one annual-threshold subtraction when reached.
4. Environment update, then game-data/citizen-manager work.
5. After the iterations, the bit-0x10 follow-up pass, camera work and UI/audio work.

The previous Rust driver ran calendar/environment before entities. It now follows the original
order: natural resources see the previous environment temperature, and their RNG draws precede
that iteration's weather draws. Toolbar pause yields zero iterations but still allows follow-up
smoothing. Original modal UI pause skips both simulation and follow-up; gameplay pause dialogs
and their modal controller wiring are still unported.

`tools/emu/game_loop.py` observes the original function through the simulation/follow-up block,
stopping at `FUN_1400a95b0`. Managers and camera/UI calls are stubbed; the original guards, iteration
count, root-only flags, calendar arithmetic and rollover execute unchanged. Its **96** cases cover
all four speeds, toolbar/modal pause, zero/nonzero deltas and year boundaries. The test checks
calendar results and this recorded phase contract; it does not execute unported manager bodies.

## Preserved animation resource metadata

The complete AnimationGroup serializer is `0x1800fdf30`; its clip serializer is `0x1800fd910`.
The format reader previously discarded metadata required for playback. It now retains:

| Runtime fields | Stored values |
|---|---|
| Clip `+0x30/+0x34/+0x38` | Source frame range and declared sample rate |
| Clip `+0x3c/+0x40` | Loop flag and source loop-start frame |
| Clip `+0x28/+0x2c` | Shared instance count and instanced flag |
| Clip `+0x44/+0x48/+0x4c/+0x50` | Sample rate/interval, node count and sample-frame count |
| Clip `+0x58` | Serialized integer duration, in microseconds |
| Clip `+0x00/+0x08` | Ordered source-frame/event-reference records |
| Group `+0x30/+0x38/+0x48` | Default blend, named overrides and compiled transition matrix |

Python corpus decoding consumes every byte in all **15** AnimationGroup resources. The Rust corpus
test verifies their **64** clips and compiled blend matrices. The nine shipped tree groups have
three clips in order: Fall, Sink, Remove. Index **2** is Remove: **76 samples at 15 Hz**, serialized
duration **5,066,667 ticks**, two nodes, nonlooping and noninstanced. Its duration is used directly;
recomputing `frames / rate` can give a different final tick.

All **17** shipped natural/fruit tree meshes have two nodes and nonempty draw ranges attached to
node 1. This is verified from package data, rather than inferred from animation names.

## Playback and pose arithmetic

`FUN_14012bc50` sets the natural-resource removal flag and dispatches state 2. `FUN_14012a5c0`
selects the current model variant's AnimationGroup and passes index 2 and start time zero to
MeshInstance::PlayAnimation (`0x1801020d0`). Tree expiry starts with an empty playback state, so
there is no preceding clip to blend out. `Playback` ports this unique, single-clip case; a generic
blend stack and AnimationManager's shared instance allocation are not claimed here.

AnimationState::Update (`0x1801003b0`) publishes the pose at the **old** clock before advancing
the playback clock. Unit speed adds the original integer delta directly; other speeds convert
the scaled f32 delta back to Time. Nonlooping clocks clamp at serialized duration. Looping clocks
subtract full durations before adding the stored source loop point. MeshInstance::IsAnimating
(`0x180102060`) tests the first state's loop flag or `elapsed < duration`.

Animation::GetPoseAtTime (`0x1800fd530`) selects adjacent samples and follows the original loop
index arithmetic. Translation interpolation preserves multiply/add order. Quaternion::Lerp
(`0x18001dde0`) chooses the opposite-sign branch at dot <= 0 and normalizes with its y/x/z/w sum
order. Sampling performs native-convention arithmetic before conjugating the result; merely
negating inputs first changes signed-zero bits. Root rotation resets to identity and root
translation becomes a frame movement delta, as in the original sampler.

Animation::ApplyEvents (`0x1800fd230`) converts each event's **source** frame through the source
range and stored sample interval. It uses a lower-inclusive, upper-exclusive tick interval and
the original wrap check. Ordinary TreeAnims' Remove clip includes a TreeFallEffect event; fruit
trees have none. Rust emits the resource reference at the original interval. Spatial
AnimAudioEvent/SoundEffect dispatch remains unported; the gameplay driver currently records the
emission in debug logging rather than synthesizing a different audio behavior.

## Live tree lifecycle and rendering

The entity bit-0x04 phase runs ModelComponent (`FUN_140129e30`) before NaturalResourceComponent
(`FUN_14012b110`) in template component order. Playback advances once per simulation iteration.
Once IsAnimating becomes false, the resource queues the existing removal callback; its material
guard was already cleared by the growth transition, so no harvest output is created.

The port now starts actual playback instead of retaining `pending_animation`. Trees remain stamped
and visible during the clip, fall and sink through the packaged node pose, then leave the map and
scheduler at completion. Toolbar pause freezes playback. Instance transforms incorporate the
animated node using the mesh's stored inverse bind, while cached bind uniforms, geometry and
textures remain in place. This supports the shipped single-rigid-node tree meshes; animated skins
and models with several distinct rigid nodes require their separate renderer path. No original
pixel equivalence is claimed for these captures.

## Verification

- **23,370** pose cases across all 64 clips match original GetPoseAtTime f32 bits, covering integer/
  fractional samples, end/loop boundaries and three frame deltas.
- **3,072** unique-state clock/completion/event cases match original Update, IsAnimating and event
  callbacks. Shared instance IDs are deliberately disabled in these observations.
- **96** original loop/calendar/phase cases match the contract above.
- Regenerated **15,360** growth cases, **480** initialization cases and **640** scheduler visits
  match at the original microsecond frequency.
- The generated-town GPU test forces one tree across its traced growth threshold, plays the loaded
  Remove clip, verifies per-instance motion, pause, the last two ticks before completion, removal
  at exactly 5,066,667 ticks and restoration of the cell's terrain-derived type. The tested cell is
  rough ground, so cleanup must not force every removed tree's cell to buildable ground.

The standing, falling and removed captures under
`game-data/derived/captures/tree-expiry/` were opened and inspected: the selected pine stands,
falls, and disappears while the surrounding terrain/trees remain visible. They are ignored local
artifacts. Regenerate with `UNBANISHED_CAPTURE_TREE_EXPIRY=game-data/derived/captures/tree-expiry
cargo test --release -p unbanished-app -- --nocapture`.

Full release workspace tests: **75 passed, 0 failed, 2 pre-existing save tests ignored**.
The release WASM build passed. Browser interaction and original-settlement/save parity remain open.

Next work: ongoing SpawnComponent behavior, spatial animation-event audio, citizen animation/AI
and production, shared/blended animation states, and original-save comparisons. See the
[checklist](GAME-LOOP-CHECKLIST.md); this slice does not establish a complete 1:1 game.
