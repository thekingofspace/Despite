use crate::prelude::line_count;
use crate::runner::padded_row_bytes;
use crate::*;

fn sample_params() -> Vec<ShaderParam> {
    vec![
        ShaderParam::new("Strength", ShaderValue::Float(0.5)),
        ShaderParam::new("Offset", ShaderValue::Vec2([1.0, 2.0])),
        ShaderParam::new("Tint", ShaderValue::Color([0.1, 0.2, 0.3, 0.4])),
        ShaderParam::new("Steps", ShaderValue::Int(-3)),
        ShaderParam::new("Enabled", ShaderValue::Bool(true)),
    ]
}

fn tiny_job(Source: &str) -> ShaderJob {
    ShaderJob::new(Source, ShaderImage::filled(4, 4, [10, 20, 30, 255]))
}

fn find_struct<'a>(Module: &'a naga::Module, Name: &str) -> (&'a [naga::StructMember], u32) {
    for (_, Type) in Module.types.iter() {
        if Type.name.as_deref() == Some(Name)
            && let naga::TypeInner::Struct {
                members: Members,
                span: Span,
            } = &Type.inner
        {
            return (Members.as_slice(), *Span);
        }
    }
    panic!("struct {Name} not found");
}

fn parse_with_prelude(Params: &[ShaderParam], InputNames: &[String], Source: &str) -> naga::Module {
    let Text = format!("{}{}", build_prelude(Params, InputNames), Source);
    naga::front::wgsl::parse_str(&Text).expect("prelude should parse")
}

fn has_comment(Text: &str) -> bool {
    let Slash = '/';
    let Star = '*';
    Text.contains(&format!("{Slash}{Slash}")) || Text.contains(&format!("{Slash}{Star}"))
}

const IdentityEffect: &str =
    "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    return sample_input(Uv);\n}\n";

#[test]
fn layout_follows_wgsl_alignment_rules() {
    let Layout = compute_param_layout(&sample_params());
    let Offsets: Vec<u32> = Layout.Fields.iter().map(|Field| Field.Offset).collect();
    assert_eq!(Offsets, vec![0, 8, 16, 32, 36]);
    assert_eq!(Layout.Size, 48);

    let Packed = compute_param_layout(&[
        ShaderParam::new("A", ShaderValue::Float(1.0)),
        ShaderParam::new("B", ShaderValue::Color([0.0; 4])),
        ShaderParam::new("C", ShaderValue::Float(1.0)),
        ShaderParam::new("D", ShaderValue::Vec2([0.0; 2])),
    ]);
    let Offsets: Vec<u32> = Packed.Fields.iter().map(|Field| Field.Offset).collect();
    assert_eq!(Offsets, vec![0, 16, 32, 40]);
    assert_eq!(Packed.Size, 48);

    assert_eq!(compute_param_layout(&[]).Size, 16);
    assert_eq!(
        compute_param_layout(&[ShaderParam::new("A", ShaderValue::Vec2([0.0; 2]))]).Size,
        16
    );
}

#[test]
fn layout_matches_naga_layouter() {
    let Cases = vec![
        sample_params(),
        vec![
            ShaderParam::new("A", ShaderValue::Bool(false)),
            ShaderParam::new("B", ShaderValue::Color([0.0; 4])),
            ShaderParam::new("C", ShaderValue::Vec2([0.0; 2])),
            ShaderParam::new("D", ShaderValue::Int(0)),
            ShaderParam::new("E", ShaderValue::Vec2([0.0; 2])),
            ShaderParam::new("F", ShaderValue::Float(0.0)),
        ],
        vec![ShaderParam::new("Only", ShaderValue::Float(0.0))],
    ];
    for Params in Cases {
        let Module = parse_with_prelude(&Params, &[], IdentityEffect);
        let (Members, Span) = find_struct(&Module, "DespiteParams");
        let Layout = compute_param_layout(&Params);
        assert_eq!(Members.len(), Layout.Fields.len());
        for (Member, Field) in Members.iter().zip(&Layout.Fields) {
            assert_eq!(Member.name.as_deref(), Some(Field.Name.as_str()));
            assert_eq!(Member.offset, Field.Offset, "offset of {}", Field.Name);
        }
        assert!(Span <= Layout.Size);
        assert_eq!(Layout.Size % 16, 0);
    }
}

#[test]
fn globals_struct_matches_rust_layout() {
    let Module = parse_with_prelude(&[], &[], IdentityEffect);
    let (Members, Span) = find_struct(&Module, "DespiteGlobals");
    let Expected = [
        ("Resolution", std::mem::offset_of!(GlobalsData, Resolution)),
        (
            "InputResolution",
            std::mem::offset_of!(GlobalsData, InputResolution),
        ),
        ("Time", std::mem::offset_of!(GlobalsData, Time)),
        ("Frame", std::mem::offset_of!(GlobalsData, Frame)),
        ("FrameCount", std::mem::offset_of!(GlobalsData, FrameCount)),
        ("Fps", std::mem::offset_of!(GlobalsData, Fps)),
        ("Duration", std::mem::offset_of!(GlobalsData, Duration)),
        ("Progress", std::mem::offset_of!(GlobalsData, Progress)),
        (
            "LoopProgress",
            std::mem::offset_of!(GlobalsData, LoopProgress),
        ),
    ];
    assert_eq!(Members.len(), Expected.len());
    for (Member, (Name, Offset)) in Members.iter().zip(Expected) {
        assert_eq!(Member.name.as_deref(), Some(Name));
        assert_eq!(Member.offset as usize, Offset, "offset of {Name}");
    }
    assert!(Span as usize <= std::mem::size_of::<GlobalsData>());
    assert_eq!(std::mem::size_of::<GlobalsData>() % 16, 0);
}

#[test]
fn globals_for_frame_values() {
    let Globals = GlobalsData::for_frame((10, 20), (5, 6), 3, 4, 2.0);
    assert_eq!(Globals.Resolution, [10.0, 20.0]);
    assert_eq!(Globals.InputResolution, [5.0, 6.0]);
    assert_eq!(Globals.Time, 1.5);
    assert_eq!(Globals.Frame, 3);
    assert_eq!(Globals.FrameCount, 4);
    assert_eq!(Globals.Duration, 2.0);
    assert_eq!(Globals.Progress, 1.0);
    assert_eq!(Globals.LoopProgress, 0.75);
    let Single = GlobalsData::for_frame((1, 1), (1, 1), 0, 1, 0.0);
    assert_eq!(Single.Time, 0.0);
    assert_eq!(Single.Progress, 0.0);
}

#[test]
fn pack_params_writes_values_at_offsets() {
    let Bytes = pack_params(&sample_params());
    assert_eq!(Bytes.len(), 48);
    let F32At = |Offset: usize| f32::from_le_bytes(Bytes[Offset..Offset + 4].try_into().unwrap());
    let I32At = |Offset: usize| i32::from_le_bytes(Bytes[Offset..Offset + 4].try_into().unwrap());
    let U32At = |Offset: usize| u32::from_le_bytes(Bytes[Offset..Offset + 4].try_into().unwrap());
    assert_eq!(F32At(0), 0.5);
    assert_eq!(U32At(4), 0);
    assert_eq!((F32At(8), F32At(12)), (1.0, 2.0));
    assert_eq!(
        [F32At(16), F32At(20), F32At(24), F32At(28)],
        [0.1, 0.2, 0.3, 0.4]
    );
    assert_eq!(I32At(32), -3);
    assert_eq!(U32At(36), 1);
    assert_eq!(pack_params(&[]), vec![0u8; 16]);
}

#[test]
fn prelude_contains_bindings() {
    let Prelude = build_prelude(&sample_params(), &["Noise".to_string(), "Mask".to_string()]);
    for Expected in [
        "struct DespiteGlobals {",
        "struct DespiteParams {",
        "    Strength: f32,",
        "    Offset: vec2<f32>,",
        "    Tint: vec4<f32>,",
        "    Steps: i32,",
        "    Enabled: u32,",
        "@group(0) @binding(0) var<uniform> Globals: DespiteGlobals;",
        "@group(0) @binding(1) var<uniform> Params: DespiteParams;",
        "@group(0) @binding(2) var Sampler: sampler;",
        "@group(0) @binding(3) var LinearSampler: sampler;",
        "@group(0) @binding(4) var NearestSampler: sampler;",
        "@group(0) @binding(5) var RepeatSampler: sampler;",
        "@group(0) @binding(6) var Input: texture_2d<f32>;",
        "@group(0) @binding(7) var Noise: texture_2d<f32>;",
        "@group(0) @binding(8) var Mask: texture_2d<f32>;",
        "fn sample_input(Uv: vec2<f32>) -> vec4<f32>",
        "fn sample_texture(Texture: texture_2d<f32>, Uv: vec2<f32>) -> vec4<f32>",
        "@vertex",
        "@fragment",
    ] {
        assert!(
            Prelude.contains(Expected),
            "prelude is missing `{Expected}`\n{Prelude}"
        );
    }
    assert!(!has_comment(&Prelude));
    assert!(Prelude.ends_with('\n'));

    let Empty = build_prelude(&[], &[]);
    assert!(Empty.contains("struct DespiteParams {\n    Padding: u32,\n}"));
    assert!(!Empty.contains("@binding(7)"));
}

#[test]
fn prelude_and_default_effect_validate() {
    let Job = tiny_job(DefaultEffectSource);
    validate(&Job).expect("default effect must validate");
    assert!(!has_comment(DefaultEffectSource));
    let Module = parse_with_prelude(&sample_params(), &["Noise".to_string()], IdentityEffect);
    let mut Validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    );
    Validator.validate(&Module).expect("prelude must validate");
}

#[test]
fn identifier_rules() {
    for Good in [
        "Strength",
        "_Hidden",
        "Noise2",
        "a",
        "GLOW_COLOR",
        "Texture_01",
    ] {
        assert_eq!(
            validate_identifier(Good),
            Ok(()),
            "{Good} should be accepted"
        );
    }
    for Bad in [
        "",
        "2Fast",
        "has space",
        "dash-name",
        "Ünicode",
        "_",
        "__Private",
        "fn",
        "let",
        "struct",
        "vec2",
        "f32",
        "sampler",
        "min",
        "Input",
        "Globals",
        "Params",
        "Sampler",
        "effect",
        "sample_input",
    ] {
        assert!(
            validate_identifier(Bad).is_err(),
            "{Bad:?} should be rejected"
        );
    }
    let TooLong = "A".repeat(MaxIdentifierLength + 1);
    assert!(validate_identifier(&TooLong).is_err());
}

#[test]
fn syntax_error_reports_user_line() {
    let Source = "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    let Color = sample_input(Uv);\n    let Broken = 1.0 +;\n    return Color;\n}\n";
    let Error = validate(&tiny_job(Source)).expect_err("syntax error expected");
    assert_eq!(Error.Line, Some(3), "{Error:?}");
    assert!(Error.Column.is_some());
    let Column = Error.Column.unwrap();
    assert!((18..=24).contains(&Column), "column {Column} for {Error:?}");
    assert!(Error.to_string().starts_with("line 3"));
}

#[test]
fn syntax_error_on_first_line() {
    let Source = "fn effect(Uv: vec2<f32>, Pixel vec2<f32>) -> vec4<f32> {\n    return sample_input(Uv);\n}\n";
    let Error = validate(&tiny_job(Source)).expect_err("syntax error expected");
    assert_eq!(Error.Line, Some(1), "{Error:?}");
    assert_eq!(Error.Column, Some(32), "{Error:?}");
}

#[test]
fn type_error_reports_user_line() {
    let Source = "fn helper() -> f32 {\n    return 1.0;\n}\n\nfn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    let Value: f32 = vec2<f32>(1.0, 2.0);\n    return vec4<f32>(Value);\n}\n";
    let Error = validate(&tiny_job(Source)).expect_err("type error expected");
    assert_eq!(Error.Line, Some(6), "{Error:?}");
}

#[test]
fn unknown_param_reports_user_line() {
    let Source = "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    return sample_input(Uv) * Params.Missing;\n}\n";
    let mut Job = tiny_job(Source);
    Job.Params = vec![ShaderParam::new("Present", ShaderValue::Float(1.0))];
    let Error = validate(&Job).expect_err("unknown member expected");
    assert_eq!(Error.Line, Some(2), "{Error:?}");
}

#[test]
fn missing_effect_is_explained() {
    let Error =
        validate(&tiny_job("fn other() -> f32 { return 1.0; }\n")).expect_err("missing effect");
    assert_eq!(Error.Line, None);
    assert!(Error.Message.contains("fn effect"), "{Error:?}");
}

#[test]
fn wrong_effect_signature_is_rejected() {
    let Error = validate(&tiny_job(
        "fn effect(Uv: vec2<f32>) -> vec4<f32> {\n    return sample_input(Uv);\n}\n",
    ))
    .expect_err("wrong signature");
    assert!(Error.Message.contains("fn effect"), "{Error:?}");
    assert_eq!(Error.Line, Some(1), "{Error:?}");
    let Error = validate(&tiny_job(
        "fn helper() {}\nfn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec3<f32> {\n    return sample_input(Uv).rgb;\n}\n",
    ))
    .expect_err("wrong return type");
    assert_eq!(Error.Line, Some(2), "{Error:?}");
}

#[test]
fn settings_are_checked() {
    let mut Job = tiny_job(IdentityEffect);
    Job.FrameCount = 0;
    assert!(validate(&Job).is_err());

    let mut Job = tiny_job(IdentityEffect);
    Job.FrameCount = 3;
    Job.Fps = 0.0;
    assert!(validate(&Job).is_err());

    let mut Job = tiny_job(IdentityEffect);
    Job.Main.Rgba.pop();
    assert!(validate(&Job).is_err());

    let mut Job = tiny_job(IdentityEffect);
    Job.OutputSize = Some((0, 5));
    assert!(validate(&Job).is_err());

    let mut Job = tiny_job(IdentityEffect);
    Job.Inputs = vec![
        ShaderInput::new("Noise", ShaderImage::filled(2, 2, [0; 4])),
        ShaderInput::new("Noise", ShaderImage::filled(2, 2, [0; 4])),
    ];
    assert!(validate(&Job).is_err());

    let mut Job = tiny_job(IdentityEffect);
    Job.Inputs = (0..MaxExtraInputs + 1)
        .map(|Index| ShaderInput::new(format!("Extra{Index}"), ShaderImage::filled(1, 1, [0; 4])))
        .collect();
    assert!(validate(&Job).is_err());

    let mut Job = tiny_job(IdentityEffect);
    Job.Params = vec![
        ShaderParam::new("Amount", ShaderValue::Float(1.0)),
        ShaderParam::new("Amount", ShaderValue::Int(1)),
    ];
    assert!(validate(&Job).is_err());

    let mut Job = tiny_job(IdentityEffect);
    Job.Params = vec![ShaderParam::new("fn", ShaderValue::Float(1.0))];
    assert!(validate(&Job).is_err());
}

#[test]
fn validate_source_without_images() {
    assert!(validate_source(IdentityEffect, &[], &[]).is_ok());
    let Error = validate_source("fn effect(", &[], &[]).expect_err("broken");
    assert_eq!(Error.Line, Some(1));
    assert_eq!(Error.Column, Some(11));
    let Unclosed =
        "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {\n    return sample_input(Uv);\n";
    let Error = validate_source(Unclosed, &[], &[]).expect_err("unclosed");
    assert_eq!(Error.Line, Some(2), "{Error:?}");
}

#[test]
fn line_counting() {
    assert_eq!(line_count(""), 0);
    assert_eq!(line_count("a\nb\n"), 2);
}

#[test]
fn padded_rows() {
    assert_eq!(padded_row_bytes(1), 256);
    assert_eq!(padded_row_bytes(37), 256);
    assert_eq!(padded_row_bytes(64), 256);
    assert_eq!(padded_row_bytes(65), 512);
}

struct Example {
    File: &'static str,
    Source: &'static str,
    Params: Vec<ShaderParam>,
    Inputs: Vec<&'static str>,
}

fn examples() -> Vec<Example> {
    vec![
        Example {
            File: "glow.wgsl",
            Source: include_str!("../examples/shaders/glow.wgsl"),
            Params: vec![
                ShaderParam::new("Radius", ShaderValue::Float(6.0)),
                ShaderParam::new("Intensity", ShaderValue::Float(1.5)),
                ShaderParam::new("GlowColor", ShaderValue::Color([1.0, 0.8, 0.3, 1.0])),
                ShaderParam::new("Pulse", ShaderValue::Bool(true)),
            ],
            Inputs: vec![],
        },
        Example {
            File: "chromatic_aberration.wgsl",
            Source: include_str!("../examples/shaders/chromatic_aberration.wgsl"),
            Params: vec![
                ShaderParam::new("Amount", ShaderValue::Float(2.0)),
                ShaderParam::new("Direction", ShaderValue::Vec2([1.0, 0.0])),
                ShaderParam::new("Wobble", ShaderValue::Float(0.5)),
            ],
            Inputs: vec![],
        },
        Example {
            File: "dissolve_with_noise_input.wgsl",
            Source: include_str!("../examples/shaders/dissolve_with_noise_input.wgsl"),
            Params: vec![
                ShaderParam::new("Amount", ShaderValue::Float(0.4)),
                ShaderParam::new("Animate", ShaderValue::Bool(true)),
                ShaderParam::new("EdgeWidth", ShaderValue::Float(0.08)),
                ShaderParam::new("EdgeColor", ShaderValue::Color([1.0, 0.5, 0.1, 1.0])),
                ShaderParam::new("NoiseScale", ShaderValue::Float(1.0)),
            ],
            Inputs: vec!["Noise"],
        },
        Example {
            File: "pixel_outline.wgsl",
            Source: include_str!("../examples/shaders/pixel_outline.wgsl"),
            Params: vec![
                ShaderParam::new("OutlineColor", ShaderValue::Color([0.0, 0.0, 0.0, 1.0])),
                ShaderParam::new("Thickness", ShaderValue::Int(1)),
                ShaderParam::new("Diagonal", ShaderValue::Bool(false)),
            ],
            Inputs: vec![],
        },
        Example {
            File: "fire_vfx.wgsl",
            Source: include_str!("../examples/shaders/fire_vfx.wgsl"),
            Params: vec![
                ShaderParam::new("Intensity", ShaderValue::Float(1.0)),
                ShaderParam::new("Speed", ShaderValue::Float(1.5)),
                ShaderParam::new("Height", ShaderValue::Float(0.2)),
            ],
            Inputs: vec![],
        },
    ]
}

#[test]
fn example_shaders_validate() {
    for Example in examples() {
        assert!(
            !has_comment(Example.Source),
            "{} has a comment",
            Example.File
        );
        let mut Job = tiny_job(Example.Source);
        Job.Params = Example.Params.clone();
        Job.Inputs = Example
            .Inputs
            .iter()
            .map(|Name| ShaderInput::new(*Name, ShaderImage::filled(8, 8, [128, 128, 128, 255])))
            .collect();
        Job.FrameCount = 4;
        if let Err(Error) = validate(&Job) {
            panic!("{} failed to validate: {Error}", Example.File);
        }
    }
}

#[test]
fn runner_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<ShaderRunner>();
    assert_send::<ShaderJob>();
    assert_send::<ShaderOutput>();
    assert_send::<ShaderError>();
}
