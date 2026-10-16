# Particles

Particle systems are presentation only. They draw on the runtime's own Random
(`Graphics::Particle::_rng`, an instance of the same Mersenne Twister), never the game's, so
they do not touch the simulation.

Port:
- `apps/game/src/particles.rs`: the systems and the ParticleComponents;
- `apps/game/src/particle_gfx.rs`: drawing;
- `banished_formats::game::{particle_system, particle_description}`: parsing.

## ParticleSystem (`ParticleSystems\*\*.rsc`)

The resource has two blocks. The `Graphics::Particle` block (`0x180106790`) holds:
- `instanced`;
- the instance count;
- the most particles alive (`+0x28`);
- the MaterialInstance.

The `Graphics::ParticleSystem` block (`0x1800a5c70`) holds the fields below, in serialized
order. Times are `System::Time` microseconds.

| Offset | Field | Chimney smoke |
|---|---|---|
| `+0x48`, `+0x4c`, `+0x50` | rate (per second), its change, scale | 5, 0, 1 |
| `+0x58` | how long it emits (0: always) | 0 |
| `+0x60` | direction | (0, 1, 0) |
| `+0x78`, `+0x6c` | spawn box size and axes | 0, (1, 1, 1) |
| `+0x7c`, `+0x80` | cone: azimuth and elevation (±degrees) | 180, 20 |
| `+0x84`, `+0x88` | speed, ± range | 0.5, 0 |
| `+0x8c`, `+0x90` | force scale, velocity kept per update | 1, 1 |
| `+0x94`, `+0x98` | rotation, ± range (degrees) | 0, 180 |
| `+0x9c`, `+0xa0` | spin, ± range (degrees/s; either way) | 30, 5 |
| `+0xa4`, `+0xa8` | spin change, kept part | 0, 1 |
| `+0xac`, `+0xb0` | life, ± range (s) | 4.5, 1 |
| `+0xb4`, `+0xb8` | force amount, force | 1, (15, 4, 10) |
| `+0xc4`, `+0xc8` | size, ± range | 0.15, 0.05 |
| `+0xcc`, `+0xd0` | growth, kept part | 9, 1 |
| `+0xdc`, `+0xe0` | start and end colour (BGRA) | `c0c0c0c0`, `c0ffffff` |
| `+0xd4`, `+0xd8` | part of life the colour moves over | 0, 1 |
| `+0xe8`, `+0xf0` | fade in, fade out | 0.1 s, 3.4 s |

The game ships six systems:
- ChimneySmoke;
- IronSmoke;
- FireSmall, FireMedium and FireLarge;
- CropDisease.

`ParticleSystems\Tornado` is the separate Tornado class; see [The Tornado class](#the-tornado-class).

## Update (`ParticleSystemInstance::Update`, `0x1800a6550`)

**Emission.**
- The rate changes by `dt² × change × scale`. It stops when it falls below 0 or the duration has passed.
- While the time since the last particle exceeds `1 / rate` and fewer than the most are alive, a particle spawns.

**Spawn.** Random values come in this order:
1. **Life.**
2. **Position.** The spawn box gives z, then y, then x, each `axis × tol × size / 2`.
3. **Rotation.**
4. **Size.**
5. **Spin.** The spin's sign is flipped on an even draw.
6. **Speed.**
7. **Cone elevation.**
8. **Cone azimuth.**

The velocity is `(−cos e · sin a, cos e · cos a, sin e) × speed`. `Transform::FromDirection`
leaves it as is for the shipped `+Y` direction. The colour starts at the start colour, and the
alpha starts at 0.

**Each particle per update.** All the changes go by the frame time squared, as in the original:
- `v = (v + force × amount × scale × dt²) × kept`, then the position moves by `v·dt`;
- the spin and growth change the same way, and the rotation and size follow them;
- `t = (age / life − span₀) / (span₁ − span₀)`, clamped to [0, 1], moves the colour from the start colour to the end colour;
- the alpha is the colours' alpha at `t`, times the fade:
  - while `age / fade in` is at most 1, the fade is that ratio;
  - after that, the fade is `1 − (fade out − life + age) / fade out`;
- particles whose age reaches their life are removed.

**Drawing** (`ParticleInstance::Draw`). Particles are drawn newest first, a quad each, with
indices `0, 2, 1, 0, 3, 2`. Each vertex carries:
- the position;
- the colour and alpha, the alpha times the geometry's alpha;
- the uv;
- the corner `(±1, ±1) × size`;
- the rotation in radians.

## Materials

- **ParticleMaterial** (smoke, fire).
  - The vertex shader turns the corner by the rotation and lays it along the camera's right and up axes.
  - The sprite's light is a fixed `(0.632, 0.632, 0.447)`, turned with the rotation.
  - The pixel shader takes the normal from the normal map's alpha and green, with `z = 1 − |xy|`, and sets `d = 2·(n·l)`.
  - Lighting is `sat((d² + 1)/2) × ambient + sat(d² × light colour)`.
  - The result is diffuse × colour, then fogged.
- **ParticleWaterMaterial** (CropDisease). Quads lie in the system's ground plane, unlit, coloured diffuse × colour.

Both materials blend by alpha and are depth tested but not written. The port draws them after
the water.

## ParticleComponent (vtable `0x140160380`)

`Template\X.rsc:particle` (`FUN_140053dc0`) lists slots, followed by a points model at `+0x20`.
Each slot holds:
- `+0x00`: the system;
- `+0x08`: a node name;
- `+0x10..+0x13`: four flags;
- `+0x20`: the fade time;
- `+0x18`: the prewarm.

**Placement** (`FUN_140052ec0`). A slot is placed at the first of these that applies:
1. With flag 3 (the pasture) or no model, the middle of the map rectangle on the ground.
2. With a points model, its point of that name.
3. Otherwise, the model node of that name.
4. Otherwise, the model's root.

Flag 0 would follow the node every frame; no slot sets it. The rotation is always identity.

**Start** (`FUN_1400533b0`). The slot gets a new instance. It runs for the prewarm, its alpha
starts at 0, and it fades in at `1/fade` per second.

**Event 3** (`FUN_1400539f0`). The slot fades out at `−1/fade`. It goes once the alpha reaches 0
(`FUN_140053490`).

**What starts slots:**

| Flag / slot | Started by | Stopped by |
|---|---|---|
| flag 1 | activation (the tornado) | |
| flag 2 (chimney and iron smoke) | entity event 2: the first resident (`ResidenceComponent::AddResident`), or the first worker of the building's own profession (`FUN_140134ef0`) | event 3: the last one leaves |
| the FireDescription's `+0x28` slot | `FireComponent::Ignite` | `PutOut` |
| a field's, orchard's or pasture's slot | DiseaseComponent while sick | the end of the disease |

**How the port does it.** It drives the slots from the same state each frame:
- residents in the home, or workers of its own profession;
- `burn > 0`;
- disease time left.

Systems run on game time. They stop while paused and speed up with the game speed.

## The Tornado class

`Tornado` is a `Graphics::Particle` subclass (type info `DAT_1401ce160`, vtable `0x1401665b0`,
serializer `FUN_1400d76e0`, update `FUN_1400d7940`). Its particles are placed, not pushed. The
records and the draw are the runtime's (`ParticleInstance::Draw`, `0x180106ac0`).

**Data** (after the base Particle block: max particles 750 and the ChimneySmoke material):

| Offset | Meaning | Tornado |
|---|---|---|
| `+0x48` | spiral radius scale | 20 |
| `+0x4c` | rise over a life | 75 |
| `+0x50` | wobble radius | 26 |
| `+0x54` | wobble turn (°/s) | 30 |
| `+0x58` | spiral turn over a life | 400 |
| `+0x5c` | particles a second | 20 |
| `+0x60`, `+0x64` | life (s, base ± range) | 30 ± 2 |
| `+0x68`, `+0x6c` | least and most size | 2.5, 15 |
| `+0x70` | size per spiral radius | 30 |
| `+0x74`, `+0x78` | start and end colours (BGRA) | `c0c0c0 ff`, `909090 80` |
| `+0x80`, `+0x88` | fade in, fade out | 0.05 s, 1 s |

**Update**, each frame:

1. **Emit.** While the time since the last exceeds `1 / +0x5c` and fewer than the most are
   alive, it adds a particle at the origin. The particle gets:
   - life `RandFloatTol × +0x64 + +0x60`;
   - rotation `RandFloatTol × 180`;
   - size `+0x68`, the start colour, alpha 0.

   Both draws are on the runtime's Random.
2. **Turn.** The instance's angle `+0xd8` turns `dt × +0x54` degrees.
3. **Place each particle**, last to first, by its age `t` (of its life):
   - **Wobble:** `W = (cos((t − 0.2)·2π)/2 + 1/2)(t + 1/2) − 0.33`.
   - **Spiral:** `R = |(t − 0.4)³|·3.7 + 0.05 + 4·max(t − 0.3, 0)²`.
   - **Position:**
     - `x = +0x48·sin(b)·R + sin(a)·+0x50·W`;
     - `y = +0x48·cos(b)·R + cos(a)·+0x50·W`;
     - `z = |rotation|/40 + t·+0x4c`.

     Here `a` is the instance angle and `b = +0x58·t/π` (radians).
   - **Spin:** 90° a second, wrapped past 180°.
   - **Size:** `R·+0x70` clamped to `+0x68..+0x6c`.
   - **Colour and alpha:** they move from the start to the end over its life. The alpha is
     also faded in and out as for the systems.
4. **Remove** the dead, in order.

**The tornado's slot.** `Template\Tornado.rsc:particle` has a single slot:
- autostart;
- fade 10 s;
- prewarm 32 s, so the funnel is full when it forms.

At the end of its line the tornado fades the slot out (`FUN_140054080(0)`). When the alpha
reaches 0 the system goes, and with it the entity. The tornado sound's first slot takes the
slot's alpha as its volume (`FUN_140058630`). The system follows the tornado's position.

## Not ported

- **The render interpolation between fixed steps.** The port updates once per frame.
