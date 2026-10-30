# Despite

Despite is an open source art program written in Rust for pixel art, painting, sprite sheets, scenes, shaders and generated images. It runs on Linux, Windows, macOS, iOS and Android.

## Features

### Everywhere (desktop and mobile)

- **Projects with many images.** A project holds any number of images, each with its own layers. Images can be cloned, reordered and exported together.
- **Scenes.** A scene arranges many images together with position, scale, rotation, flip, opacity and blend mode per item. Items can play an image's layers as animation frames, with hold, offset and in/out frames.
- **Alignment overlays and onion skin.** Show any other image or scene above or below the one you are editing, at any opacity and offset. Overlays are never exported.
- **Layers.** Opacity, 14 blend modes, visibility, lock, alpha lock, non-destructive transforms (move, scale, rotate, flip), merge, flatten, live thumbnails and drag-and-drop reordering.
- **Tools.** Brush, pixel-perfect pencil, eraser, bucket fill (tolerance, contiguous, sample all layers, grow), gradient (linear, radial, reflected, conical, banded and dithered for pixel art), color picker, line, rectangle and ellipse (pixel-exact Bresenham shapes), rectangle/ellipse/lasso selection, magic wand, move/transform, symmetry ruler (vertical, horizontal, both, radial, kaleidoscope) and pan.
- **Pixel art.** Pixel-perfect lines, exact 1–N px round/square/plus/diamond tips, dither brushes (checker, Bayer 2/4/8), palette shading, color replace, wrap-around painting and tile preview for seamless tiles, pixel and tile grids, and outline and palette reduction filters.
- **Custom brushes.** Size, opacity, flow, hardness, spacing, angle, roundness, stabilizer, pressure curves (size/opacity/flow/hardness), pen tilt (angle and flat tips), jitter, scatter, multiple dabs per step, color dynamics, taper, image tips, dithering and blend modes. Presets can be saved, imported and exported as JSON.
- **Pens and tablets.** Apple Pencil, Android styluses and Windows pens report pressure. On Linux Wayland, Despite reads drawing tablets directly for pressure and tilt. On other systems you can simulate pressure from mouse speed.
- **Home screen.** Lists your projects with thumbnails, in a grid or list view, with search and sort.
- **Copy and paste.** Works for pixels, layers, whole images, scenes and scene items, and goes through the system clipboard. Pasting an image larger than the canvas offers to expand the canvas, scale the image down to fit, or paste it as a new image.
- **Effect brushes.** Blur, smudge, sharpen, grayscale, desaturate, saturate, dodge, burn, hue shift, invert, pixelate, color replace, and palette shade lighter/darker.
- **Filters.** Grayscale, invert, sepia, brightness/contrast, hue/saturation/lightness, gaussian, box and motion blur, sharpen, posterize, threshold, pixelate, noise, pixel-art outline, reduce to palette (ordered or Floyd–Steinberg dithering), replace color, remove color, hard alpha, drop shadow and vignette, all with live preview and selection masking.
- **Resampling.** Nearest neighbour, bilinear, bicubic, Lanczos 3, gaussian, box, Scale2x and Scale3x, for scaling images and for upscaling exports.
- **Export.** PNG of a whole image, a single layer, every layer or every image; sprite sheets (grid or strip, padding, margin, power-of-two) with engine-friendly JSON; animated GIF; PNG sequences. Every export can be upscaled.
- **Project files** (`.despite`) save every image, layer, scene, script and shader. Undo history is not saved by default. *Settings → History & saving* can store it, which makes files larger and slower to open.

### Desktop only

- **Python project code.** Paste Python into the project's script box and press Compile. Scripts can read every image and layer, generate new images and frames, and compose them into scenes. Results can be previewed as an image, sprite sheet, GIF, video or scene, then added to the project or exported. Your own Python is used, so numpy and Pillow work if you have them installed.
- **Shaders and VFX.** WGSL effects run on the GPU over an image or scene. Any project image can be an extra input texture. Parameters are edited in the UI, and many frames can be rendered with `Time` for animated effects.
- **Video export** to MP4, WebM (with transparency), ProRes 4444 MOV, high-quality GIF and animated WebP through ffmpeg.
- **Advanced brushes.** Animated tips (frames taken from an image's layers), color cycling, gradients along the stroke, wiggle, fade, spin, echoes and sparkles.
- **Line boil.** Boil brushes such as *Shaky Pencil*, *Wobbly Ink* and *Crayon Scribble* paint every stroke into several frame layers, each with its own wobble and pencil grain. The canvas, scenes and GIF/video exports play these frames, so the line art shakes like hand-drawn animation. The playback speed comes from the image's *Frame ms*.
- **Plugins.** A plugin is a folder with a `plugin.toml` that adds shader effects, Python tools, brush presets and palettes.

The code boxes are plain text areas meant for pasting code written in your own editor; they have no highlighting or autocomplete.

## Building

You need a recent stable Rust toolchain (`rustup`).

### Linux, Windows, macOS (native)

```sh
cargo run --release
```

On Linux, `scripts/install-linux.sh` installs the binary, desktop entry, icons and `.despite` file type into `~/.local`.

### Windows (cross-compiled from Linux)

Install `mingw-w64-gcc`, then run `scripts/build-windows.sh`.

### macOS app bundle

On a Mac, run `scripts/bundle-macos.sh` to build a universal `Despite.app`. Set `DMG=1` to also build a disk image, and `MACOS_SIGN_IDENTITY` to sign with your certificate.

### iOS

On a Mac with Xcode, run `scripts/build-ios.sh` for the simulator; add `INSTALL=1` to install it into the booted simulator. For a device, run `scripts/build-ios.sh aarch64-apple-ios` with `IOS_SIGN_IDENTITY` and `IOS_PROVISIONING_PROFILE` set. The mobile build leaves out the desktop-only features.

### Android

Install the Android SDK, NDK, build-tools and platform (for example into `~/Android/Sdk`), plus `cargo install cargo-ndk`. Then run:

```sh
scripts/build-android.sh
```

This produces `target/android/despite.apk`, signed with the debug key. `INSTALL=1` installs and launches it over adb; `ABIS="arm64-v8a"` builds a single ABI.

### Tests

```sh
cargo test --workspace
```

## Layout

| Crate | Purpose |
| --- | --- |
| `despite-core` | Document model, compositing, brushes, selections, fill, filters, resampling, export and file format. Pure Rust, all platforms. |
| `despite-app` | The egui/wgpu user interface. The `desktop` feature (on by default) enables the PC-only crates. |
| `despite-script` | Runs Python project code in a subprocess. |
| `despite-shader` | WGSL shader effects and VFX via wgpu. |
| `despite-plugin` | Plugin discovery and loading. |
| `despite-video` | Video export through ffmpeg. |
| `despite-android` | Android entry point (NativeActivity). |

## Python project code

```python
import despite

Project = despite.project()
Hero = Project.image("Hero")
Outline = Hero.layer("Outline").Pixels
Inside = Outline.enclosed()
Filled = Outline.copy()
Filled.fill_mask(Inside, (255, 200, 80, 255))
Out = despite.output()
Out.add_image("Filled", Filled)
Stage = Out.scene("Stage", 320, 180, Background=(0.1, 0.1, 0.15, 1.0))
Stage.place("Filled", X=40, Y=30, Scale=2)
```

`despite.params()` returns the parameters defined in the UI, and `despite.inputs()` returns the images chosen for Image parameters. See `crates/despite-script/examples` for more.

## Shaders

Write `fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32>`. The main image is `Input`, sampled with `sample_input(Uv)`. Parameters are read as `Params.Name`. Image parameters become textures with the parameter's name. `Globals.Time`, `Globals.Frame` and `Globals.Resolution` drive animation. See `crates/despite-shader/README.md` and its examples.

## Plugins

Plugins are loaded from the data folder (`~/.local/share/Despite/plugins` on Linux, `%APPDATA%\Despite\plugins` on Windows, `~/Library/Application Support/Despite/plugins` on macOS), from extra folders set in Settings, and from a `plugins` folder next to the executable. `plugins/example` shows the format:

```toml
[Plugin]
Id = "example"
Name = "Example Pack"

[[Shader]]
Name = "Tint Glow"
File = "tint_glow.wgsl"
Params = [{ Name = "Strength", Kind = "Float", Min = 0.0, Max = 2.0, Default = 0.8 }]

[[Script]]
Name = "Checker Background"
File = "checker.py"

[[Brush]]
File = "brushes/sparkle.json"

[[Palette]]
Name = "Warm Ramp"
Colors = ["#2b0f0e", "#a14a2b", "#f6c26b"]
```

## License

Dual-licensed under either the MIT license or the Apache License 2.0, at your option.
