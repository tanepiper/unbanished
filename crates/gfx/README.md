# unbanished-gfx

Rendering code that the game (`apps/game`) and the viewer (`apps/viewer`) share. It uses
[wgpu](https://wgpu.rs), so it runs on Vulkan, Metal, DirectX 12, OpenGL, and in browsers on
WebGPU or WebGL 2.

| Part | What it does |
|---|---|
| `UiRenderer` | Draws the quads of `unbanished-ui` in one alpha-blended pipeline, in batches split by texture and clip rectangle |
| `upload_image`, `upload_rgba`, `upload_cube`, … | Upload the game's textures (`ImageBuffer`) to the GPU, with their mip levels |
| `decode_levels` | Decode every level of a texture to RGBA8, for GPUs without BC texture support |
| `material` | Which ported shader a model's material uses (`MaterialKind`), and the atlas of citizens' outfits |
| `model` | The draw ranges of a mesh: which material, shader, textures and node transform each range uses |
| `skin` | GPU skinning: the vertex layout of skinned meshes, the bone palette and its WGSL |

The game's own scene renderer (terrain, water, models, shadows, particles) is in `apps/game`. This
crate holds only what both programs need.

## Use it

```toml
[dependencies]
unbanished-gfx = { path = "crates/gfx" }
unbanished-ui = { path = "crates/ui" }
unbanished-formats = { path = "crates/formats" }
wgpu = "30"
```

### Draw the interface

Create the renderer once, with the format of your colour and depth targets. Each frame, pass the
quads from `Ui::draw`. The renderer uploads the sprite sheets that the quads use the first time
it sees them.

```rust
use unbanished_gfx::UiRenderer;
use unbanished_ui::{NodeId, Ui};

fn draw_ui<'p>(
    renderer: &'p mut UiRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'p>,
    ui: &Ui,
    root: NodeId,
    size: (u32, u32),
) {
    let quads = ui.draw(root);
    renderer.draw(device, queue, pass, ui, &quads, size);
}

fn make(device: &wgpu::Device) -> UiRenderer {
    UiRenderer::new(device, wgpu::TextureFormat::Bgra8UnormSrgb, wgpu::TextureFormat::Depth32Float)
}
```

`set_external(device, id, view)` registers an application texture for quads with
`Texture::External(id)`, for example a preview picture in a dialog. `set_time` drives the
animated interface materials.

### Upload a texture

```rust
use unbanished_formats::pkg::Archive;
use unbanished_formats::types::image_buffer;

fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    archive: &Archive,
    reference: &str,
) -> Option<wgpu::TextureView> {
    let image = image_buffer(&archive.read_ref(reference).ok()?).ok()?;
    unbanished_gfx::upload_image(device, queue, reference, &image)
}
```

`upload_image` decodes every mip level to RGBA8 on the CPU (`decode_levels`), because WebGL 2
often has no support for BC-compressed textures, and uploads the result as an sRGB texture.
`upload_image_linear` uploads without sRGB conversion; `upload_cube` uploads a cube map.

## Rules for code in this crate

- It must work on WebGL 2: no storage buffers in vertex shaders, no base-vertex draws, the
  `downlevel_webgl2_defaults` limits.
- In WGSL, do every texture sample before any `discard`.

[Port a shader](https://tanepiper.github.io/unbanished/develop/guides/shader) explains these rules.

## The API

The full API documentation is at
<https://tanepiper.github.io/unbanished/api/unbanished_gfx/index.html>.
