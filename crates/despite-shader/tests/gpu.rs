#![allow(non_snake_case, non_upper_case_globals)]

use despite_shader::{ShaderImage, ShaderInput, ShaderJob, ShaderParam, ShaderRunner, ShaderValue};

fn runner() -> Option<ShaderRunner> {
    match ShaderRunner::new_headless() {
        Ok(Runner) => {
            eprintln!("despite-shader GPU tests using {}", Runner.adapter_name());
            Some(Runner)
        }
        Err(Error) => {
            eprintln!("note: skipping GPU test, no adapter available: {Error}");
            None
        }
    }
}

fn patterned(Width: u32, Height: u32) -> ShaderImage {
    let mut Rgba = Vec::with_capacity((Width * Height * 4) as usize);
    for Y in 0..Height {
        for X in 0..Width {
            Rgba.push((X * 7 + Y * 3) as u8);
            Rgba.push((X * 13 + Y * 29 + 5) as u8);
            Rgba.push((X ^ Y).wrapping_mul(11) as u8);
            Rgba.push(255 - ((X + Y * Width) % 200) as u8);
        }
    }
    ShaderImage::new(Width, Height, Rgba)
}

fn close(Actual: u8, Expected: u8) -> bool {
    Actual.abs_diff(Expected) <= 1
}

const IdentityEffect: &str =
    "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    return sample_input(Uv);\n}\n";

#[test]
fn identity_returns_input_bytes() {
    let Some(mut Runner) = runner() else { return };
    let Main = patterned(16, 9);
    for Smooth in [false, true] {
        let mut Job = ShaderJob::new(IdentityEffect, Main.clone());
        Job.SmoothSampling = Smooth;
        let Output = Runner.run(&Job).expect("identity render");
        assert_eq!(Output.Frames.len(), 1);
        let Frame = &Output.Frames[0];
        assert_eq!((Frame.Width, Frame.Height), (16, 9));
        assert_eq!(
            Frame.Rgba, Main.Rgba,
            "identity mismatch with SmoothSampling={Smooth}"
        );
    }
    let LoadJob = ShaderJob::new(
        "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    return load_input(vec2<i32>(Pixel));\n}\n",
        Main.clone(),
    );
    let Output = Runner.run(&LoadJob).expect("load_input render");
    assert_eq!(Output.Frames[0].Rgba, Main.Rgba);
}

#[test]
fn odd_size_readback_strips_padding() {
    let Some(mut Runner) = runner() else { return };
    let Main = patterned(37, 19);
    let mut Job = ShaderJob::new(IdentityEffect, Main.clone());
    Job.SmoothSampling = false;
    let Output = Runner.run(&Job).expect("odd size render");
    let Frame = &Output.Frames[0];
    assert_eq!((Frame.Width, Frame.Height), (37, 19));
    assert_eq!(Frame.Rgba.len(), 37 * 19 * 4);
    assert_eq!(Frame.Rgba, Main.Rgba);

    let Coordinates = ShaderJob::new(
        "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    return vec4<f32>(floor(Pixel) / 255.0, Uv.x, 1.0);\n}\n",
        Main,
    );
    let Output = Runner.run(&Coordinates).expect("coordinate render");
    let Frame = &Output.Frames[0];
    for Y in 0..19u32 {
        for X in 0..37u32 {
            let Pixel = Frame.pixel(X, Y).unwrap();
            assert_eq!(
                (Pixel[0] as u32, Pixel[1] as u32),
                (X, Y),
                "pixel ({X}, {Y})"
            );
            let ExpectedUv = ((X as f32 + 0.5) / 37.0 * 255.0).round() as u8;
            assert!(
                close(Pixel[2], ExpectedUv),
                "uv at ({X}, {Y}): {} vs {ExpectedUv}",
                Pixel[2]
            );
            assert_eq!(Pixel[3], 255);
        }
    }
}

#[test]
fn params_and_extra_input() {
    let Some(mut Runner) = runner() else { return };
    let Main = ShaderImage::filled(8, 6, [200, 0, 0, 255]);
    let Overlay = ShaderImage::filled(4, 4, [0, 0, 100, 255]);
    let Source = "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    let Base = sample_input(Uv);\n    let Other = sample_texture(Overlay, Uv);\n    let Mixed = mix(Base, Other, Params.Amount);\n    let Shift = select(0.0, Params.Offset.x, Params.Enabled != 0u);\n    return vec4<f32>(Mixed.r, Params.Tint.g + Shift, Mixed.b, f32(Params.Count) / 255.0);\n}\n";
    let mut Job = ShaderJob::new(Source, Main);
    Job.Inputs = vec![ShaderInput::new("Overlay", Overlay)];
    Job.Params = vec![
        ShaderParam::new("Amount", ShaderValue::Float(0.5)),
        ShaderParam::new("Tint", ShaderValue::Color([0.0, 0.25, 0.0, 1.0])),
        ShaderParam::new("Enabled", ShaderValue::Bool(true)),
        ShaderParam::new("Offset", ShaderValue::Vec2([0.25, 0.0])),
        ShaderParam::new("Count", ShaderValue::Int(77)),
    ];
    let Output = Runner.run(&Job).expect("param render");
    let Frame = &Output.Frames[0];
    assert_eq!((Frame.Width, Frame.Height), (8, 6));
    for Pixel in Frame.Rgba.as_chunks::<4>().0 {
        assert!(close(Pixel[0], 100), "red {Pixel:?}");
        assert!(close(Pixel[1], 128), "green {Pixel:?}");
        assert!(close(Pixel[2], 50), "blue {Pixel:?}");
        assert_eq!(Pixel[3], 77, "alpha {Pixel:?}");
    }
}

#[test]
fn multi_frame_time_advances() {
    let Some(mut Runner) = runner() else { return };
    let Source = "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    return vec4<f32>(Globals.Time, f32(Globals.Frame) / 255.0, Globals.Progress, f32(Globals.FrameCount) / 255.0);\n}\n";
    let mut Job = ShaderJob::new(Source, ShaderImage::filled(5, 3, [0, 0, 0, 255]));
    Job.FrameCount = 4;
    Job.Fps = 4.0;
    let Output = Runner.run(&Job).expect("multi-frame render");
    assert_eq!(Output.Frames.len(), 4);
    let ExpectedTime = [0u8, 64, 128, 191];
    let ExpectedProgress = [0u8, 85, 170, 255];
    for (Index, Frame) in Output.Frames.iter().enumerate() {
        let Pixel = Frame.pixel(2, 1).unwrap();
        assert!(
            close(Pixel[0], ExpectedTime[Index]),
            "frame {Index} time {Pixel:?}"
        );
        assert_eq!(Pixel[1], Index as u8, "frame {Index} index {Pixel:?}");
        assert!(
            close(Pixel[2], ExpectedProgress[Index]),
            "frame {Index} progress {Pixel:?}"
        );
        assert_eq!(Pixel[3], 4);
    }
    assert_ne!(Output.Frames[0].Rgba, Output.Frames[1].Rgba);
}

#[test]
fn output_size_override_scales() {
    let Some(mut Runner) = runner() else { return };
    let Main = patterned(3, 2);
    let mut Job = ShaderJob::new(IdentityEffect, Main.clone());
    Job.OutputSize = Some((6, 4));
    Job.SmoothSampling = false;
    let Output = Runner.run(&Job).expect("scaled render");
    let Frame = &Output.Frames[0];
    assert_eq!((Frame.Width, Frame.Height), (6, 4));
    for Y in 0..4 {
        for X in 0..6 {
            assert_eq!(
                Frame.pixel(X, Y),
                Main.pixel(X / 2, Y / 2),
                "pixel ({X}, {Y})"
            );
        }
    }
}

#[test]
fn errors_are_reported_and_runner_recovers() {
    let Some(mut Runner) = runner() else { return };
    let Broken = ShaderJob::new(
        "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    return sample_input(Uv) * Params.Missing;\n}\n",
        ShaderImage::filled(4, 4, [1, 2, 3, 4]),
    );
    let Error = Runner.run(&Broken).expect_err("broken shader must fail");
    eprintln!("reported error: {Error}");
    assert_eq!(Error.Line, Some(2));

    let Missing = ShaderJob::new("fn nothing() {}\n", ShaderImage::filled(4, 4, [1, 2, 3, 4]));
    let Error = Runner.run(&Missing).expect_err("missing effect must fail");
    eprintln!("reported error: {Error}");

    let mut TooBig = ShaderJob::new(IdentityEffect, ShaderImage::filled(1, 1, [0; 4]));
    TooBig.OutputSize = Some((Runner.max_texture_size() + 1, 1));
    let Error = Runner.run(&TooBig).expect_err("oversized output must fail");
    eprintln!("reported error: {Error}");

    let Working = ShaderJob::new(IdentityEffect, ShaderImage::filled(4, 4, [9, 8, 7, 6]));
    let Output = Runner
        .run(&Working)
        .expect("runner still works after errors");
    assert_eq!(Output.Frames[0].Rgba, Working.Main.Rgba);
}

#[test]
fn example_effects_render() {
    let Some(mut Runner) = runner() else { return };
    let Sprite = {
        let mut Image = ShaderImage::filled(24, 24, [0, 0, 0, 0]);
        for Y in 8..20u32 {
            for X in 8..16u32 {
                let Start = ((Y * 24 + X) * 4) as usize;
                Image.Rgba[Start..Start + 4].copy_from_slice(&[40, 160, 220, 255]);
            }
        }
        Image
    };
    let mut Job = ShaderJob::new(
        include_str!("../examples/shaders/pixel_outline.wgsl"),
        Sprite.clone(),
    );
    Job.SmoothSampling = false;
    Job.Params = vec![
        ShaderParam::new("OutlineColor", ShaderValue::Color([1.0, 0.0, 0.0, 1.0])),
        ShaderParam::new("Thickness", ShaderValue::Int(1)),
        ShaderParam::new("Diagonal", ShaderValue::Bool(false)),
    ];
    let Output = Runner.run(&Job).expect("outline render");
    let Frame = &Output.Frames[0];
    assert_eq!(Frame.pixel(7, 10), Some([255, 0, 0, 255]));
    assert_eq!(Frame.pixel(10, 10), Sprite.pixel(10, 10));
    assert_eq!(Frame.pixel(7, 7), Some([0, 0, 0, 0]));
    assert_eq!(Frame.pixel(2, 2), Some([0, 0, 0, 0]));

    let mut Fire = ShaderJob::new(
        include_str!("../examples/shaders/fire_vfx.wgsl"),
        Sprite.clone(),
    );
    Fire.FrameCount = 3;
    Fire.Fps = 8.0;
    Fire.Params = vec![
        ShaderParam::new("Intensity", ShaderValue::Float(1.2)),
        ShaderParam::new("Speed", ShaderValue::Float(1.5)),
        ShaderParam::new("Height", ShaderValue::Float(0.3)),
    ];
    let Output = Runner.run(&Fire).expect("fire render");
    assert_eq!(Output.Frames.len(), 3);
    assert_ne!(Output.Frames[0].Rgba, Output.Frames[2].Rgba);

    let mut Dissolve = ShaderJob::new(
        include_str!("../examples/shaders/dissolve_with_noise_input.wgsl"),
        Sprite,
    );
    Dissolve.Inputs = vec![ShaderInput::new("Noise", patterned(16, 16))];
    Dissolve.FrameCount = 2;
    Dissolve.Fps = 10.0;
    Dissolve.Params = vec![
        ShaderParam::new("Amount", ShaderValue::Float(0.4)),
        ShaderParam::new("Animate", ShaderValue::Bool(true)),
        ShaderParam::new("EdgeWidth", ShaderValue::Float(0.08)),
        ShaderParam::new("EdgeColor", ShaderValue::Color([1.0, 0.5, 0.1, 1.0])),
        ShaderParam::new("NoiseScale", ShaderValue::Float(1.0)),
    ];
    let Output = Runner.run(&Dissolve).expect("dissolve render");
    assert!(
        Output.Frames[1]
            .Rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|Pixel| Pixel[3] == 0)
    );
}

#[test]
fn runner_can_move_to_worker_thread() {
    let Some(mut Runner) = runner() else { return };
    let Handle = std::thread::spawn(move || {
        let Job = ShaderJob::new(IdentityEffect, ShaderImage::filled(2, 2, [5, 6, 7, 8]));
        Runner.run(&Job).map(|Output| Output.Frames[0].Rgba.clone())
    });
    let Bytes = Handle
        .join()
        .expect("worker thread")
        .expect("render on worker");
    assert_eq!(Bytes, ShaderImage::filled(2, 2, [5, 6, 7, 8]).Rgba);
}
