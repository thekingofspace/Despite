# despite-shader

Desktop-only WGSL shader effects and VFX renderer for Despite. A user writes a
single WGSL function, `effect`, that is run once per output pixel. The crate
generates everything else (bindings, helpers, a fullscreen-triangle vertex
stage and the fragment entry point), renders one or more frames headlessly
with wgpu, and returns plain RGBA8 images. It does not depend on
`despite-core`; it uses its own small types (`ShaderImage`, `ShaderJob`, ...).

## The effect function

Every effect must define exactly this function:

```wgsl
fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    return sample_input(Uv);
}
```

- `Uv` runs from 0 to 1 across the output, origin at the top-left (the same
  orientation as image rows).
- `Pixel` is the pixel-centre coordinate in output space: the top-left pixel is
  `(0.5, 0.5)`, the bottom-right is `(Width - 0.5, Height - 0.5)`.
- The returned colour is straight (non-premultiplied) RGBA in 0..1 and is
  written as-is into an `Rgba8Unorm` texture.

You may add any other functions, constants and structs, as long as their
names do not clash with the prelude names listed below. Do not declare your
own `@group`/`@binding` resources, and do not use `enable` directives (the
prelude is placed before your code, and directives must come first).

## Colour handling

Images go in and come out as RGBA8 bytes, sRGB-encoded, straight alpha,
row-major with no padding. Textures are `Rgba8Unorm` (not `*Srgb`), so bytes
pass through unchanged: a byte value of 128 reads as `128.0 / 255.0` in the
shader, and an effect that returns `sample_input(Uv)` reproduces the input
exactly. Colour parameters are passed through unchanged as well, so they
should use the same sRGB-encoded 0..1 values as the image bytes.

## Prelude bindings

`build_prelude(Params, InputNames)` returns the generated WGSL that is placed
in front of the user's source. It is meant to be shown read-only in the editor.
Users write these names exactly as shown:

| Name | WGSL type | Binding | Meaning |
| --- | --- | --- | --- |
| `Globals` | `var<uniform> DespiteGlobals` | 0 | Per-frame values, see below |
| `Params` | `var<uniform> DespiteParams` | 1 | User parameters, one field per parameter |
| `Sampler` | `sampler` | 2 | Default sampler, clamp to edge; linear when `SmoothSampling` is true, nearest when false |
| `LinearSampler` | `sampler` | 3 | Always linear, clamp to edge |
| `NearestSampler` | `sampler` | 4 | Always nearest, clamp to edge |
| `RepeatSampler` | `sampler` | 5 | Repeat addressing, filtering follows `SmoothSampling` |
| `Input` | `texture_2d<f32>` | 6 | The main image (`ShaderJob.Main`) |
| *input name* | `texture_2d<f32>` | 7, 8, ... | One per `ShaderJob.Inputs` entry, named exactly like `ShaderInput.Name`, in order |

### Globals

| Field | Type | Value |
| --- | --- | --- |
| `Globals.Resolution` | `vec2<f32>` | Output size in pixels |
| `Globals.InputResolution` | `vec2<f32>` | Size of `Input` in pixels |
| `Globals.Time` | `f32` | Seconds: `Frame / Fps` (0 for a single frame) |
| `Globals.Frame` | `u32` | Frame index, `0..FrameCount` |
| `Globals.FrameCount` | `u32` | Number of frames being rendered |
| `Globals.Fps` | `f32` | Frames per second of the job |
| `Globals.Duration` | `f32` | `FrameCount / Fps` in seconds |
| `Globals.Progress` | `f32` | `Frame / (FrameCount - 1)`: 0 on the first frame, exactly 1 on the last |
| `Globals.LoopProgress` | `f32` | `Frame / FrameCount`: 0 up to but excluding 1, for seamless loops |

For a looping GIF or sprite sheet use `LoopProgress` (for example
`sin(Globals.LoopProgress * 6.2831853)`), so the last frame flows into the
first. For a one-shot animation that must end fully applied (a dissolve that
finishes empty) use `Progress`.

### Params

Parameters are defined in the app UI, not in the shader source. Each
`ShaderParam` becomes a field of the `DespiteParams` struct with the
parameter's name, and is read as `Params.Name`:

| `ShaderValue` | WGSL field type | Notes |
| --- | --- | --- |
| `Float(f32)` | `f32` | |
| `Int(i32)` | `i32` | |
| `Bool(bool)` | `u32` | Uniforms cannot hold `bool`; the value is 0 or 1. Test with `Params.Name != 0u` |
| `Vec2([f32; 2])` | `vec2<f32>` | |
| `Color([f32; 4])` | `vec4<f32>` | RGBA, 0..1, same encoding as the image bytes |

Fields keep the order of `ShaderJob.Params`. The uniform buffer uses the
standard WGSL layout rules (scalars align to 4, `vec2` to 8, `vec4` to 16,
total size rounded up to 16 bytes); `compute_param_layout` and `pack_params`
expose the exact offsets and bytes. With no parameters the struct contains a
single unused `Padding: u32` field, because WGSL structs cannot be empty.

### Extra inputs

Each `ShaderInput` becomes a module-scope `texture_2d<f32>` named after
`ShaderInput.Name`. With an input named `Noise`:

```wgsl
fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    let Grain = sample_texture(Noise, Uv).r;
    let Tiled = textureSampleLevel(Noise, RepeatSampler, Uv * 4.0, 0.0);
    let NoiseSize = vec2<f32>(textureDimensions(Noise));
    return sample_input(Uv) * Grain;
}
```

Inputs may have any size; `Uv` addresses each texture in its own 0..1 space.
At most `MaxExtraInputs` (15) extra inputs are allowed, fewer if the GPU
reports a lower texture limit (`ShaderRunner::max_extra_inputs`).

### Naming rules

Parameter and input names are checked by `validate_identifier`:

- ASCII only: `[A-Za-z_][A-Za-z0-9_]*`, at most 64 characters.
- Not `_` alone and not starting with `__`.
- Not a WGSL keyword or reserved word (`fn`, `let`, `struct`, `loop`, ...).
- Not a WGSL built-in type or function name (`vec2`, `f32`, `texture_2d`,
  `min`, `step`, ...).
- Not one of the prelude names (`PreludeNames`): `Globals`, `Params`,
  `DespiteGlobals`, `DespiteParams`, `Sampler`, `LinearSampler`,
  `NearestSampler`, `RepeatSampler`, `Input`, `sample_input`,
  `sample_texture`, `load_input`, `load_texture`, `input_texel_size`,
  `blend_over`, `hash12`, `value_noise`, `fbm`, `effect`, `despite_vertex`,
  `despite_fragment`.
- Unique within the parameters and unique within the inputs.

PascalCase names (`GlowColor`, `NoiseScale`) are recommended.

### Helper functions

| Function | Description |
| --- | --- |
| `sample_input(Uv: vec2<f32>) -> vec4<f32>` | Samples `Input` with `Sampler` |
| `sample_texture(Texture: texture_2d<f32>, Uv: vec2<f32>) -> vec4<f32>` | Samples any texture with `Sampler` |
| `load_input(Coord: vec2<i32>) -> vec4<f32>` | Exact texel of `Input`, coordinates clamped to the edge |
| `load_texture(Texture: texture_2d<f32>, Coord: vec2<i32>) -> vec4<f32>` | Exact texel of any texture, clamped |
| `input_texel_size() -> vec2<f32>` | `1.0 / Globals.InputResolution`, the size of one input pixel in `Uv` units |
| `blend_over(Top: vec4<f32>, Bottom: vec4<f32>) -> vec4<f32>` | Straight-alpha "over" compositing |
| `hash12(Point: vec2<f32>) -> f32` | Cheap 2D hash in 0..1 |
| `value_noise(Point: vec2<f32>) -> f32` | Smooth value noise in 0..1 |
| `fbm(Point: vec2<f32>, Octaves: i32) -> f32` | Fractal value noise, 1 to 12 octaves |

The sampling helpers use `textureSampleLevel`, so they can be called anywhere,
including inside branches and loops. `textureSample` also works but WGSL only
allows it in uniform control flow.

## Rust API

```rust
use despite_shader::{ShaderImage, ShaderJob, ShaderParam, ShaderRunner, ShaderValue};

fn render(Image: ShaderImage, Source: &str) -> Result<Vec<ShaderImage>, despite_shader::ShaderError> {
    let mut Runner = ShaderRunner::new_headless()?;
    let mut Job = ShaderJob::new(Source, Image);
    Job.Params = vec![ShaderParam::new("Strength", ShaderValue::Float(0.5))];
    Job.FrameCount = 12;
    Job.Fps = 12.0;
    Job.SmoothSampling = false;
    Ok(Runner.run(&Job)?.Frames)
}
```

- `validate(&Job)` parses and validates the full shader with naga on the CPU,
  without a GPU. `validate_source(Source, Params, InputNames)` does the same
  without needing images, which suits live checking while the user types.
- `ShaderError` carries `Message`, plus `Line` and `Column` (1-based) relative
  to the user's source, with the prelude removed. `Column` counts characters.
  When the problem is in generated code (for example `effect` is missing),
  `Line` and `Column` are `None` and the message says what to fix.
- `ShaderRunner` owns its own wgpu device (high-performance adapter first,
  then a fallback/software adapter). It is `Send`, so it can live on a worker
  thread, and it is reused across runs. `run` validates first, checks the
  GPU limits (texture size, number of inputs), then renders every frame and
  reads it back. GPU validation errors are returned as `ShaderError` instead of
  panicking.
- `DefaultEffectSource` is a starter effect (an animated glow and shimmer) that
  needs no parameters or extra inputs.

## Examples

`examples/shaders/` holds ready-made effects. They expect these parameters
and inputs:

| File | Parameters | Inputs |
| --- | --- | --- |
| `glow.wgsl` | `Radius: Float` (pixels), `Intensity: Float`, `GlowColor: Color`, `Pulse: Bool` | none |
| `chromatic_aberration.wgsl` | `Amount: Float` (pixels), `Direction: Vec2`, `Wobble: Float` | none |
| `dissolve_with_noise_input.wgsl` | `Amount: Float`, `Animate: Bool`, `EdgeWidth: Float`, `EdgeColor: Color`, `NoiseScale: Float` | `Noise` |
| `pixel_outline.wgsl` | `OutlineColor: Color`, `Thickness: Int`, `Diagonal: Bool` | none |
| `fire_vfx.wgsl` | `Intensity: Float`, `Speed: Float`, `Height: Float` (fraction of the image) | none |

`pixel_outline.wgsl` works best with `SmoothSampling` off. `fire_vfx.wgsl` and
`glow.wgsl` animate over time; render several frames to see the motion.

## Tests

`cargo test -p despite-shader` runs CPU tests (layout, prelude, naming,
error locations, example validation) and GPU tests. The GPU tests print a note
and pass without checking anything when no adapter can be created.
