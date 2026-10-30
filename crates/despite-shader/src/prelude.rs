use std::fmt::Write;

use crate::types::ShaderParam;

pub const MaxExtraInputs: usize = 15;
pub const MaxIdentifierLength: usize = 64;

pub(crate) const GlobalsBinding: u32 = 0;
pub(crate) const ParamsBinding: u32 = 1;
pub(crate) const SamplerBinding: u32 = 2;
pub(crate) const LinearSamplerBinding: u32 = 3;
pub(crate) const NearestSamplerBinding: u32 = 4;
pub(crate) const RepeatSamplerBinding: u32 = 5;
pub(crate) const InputBinding: u32 = 6;
pub(crate) const FirstExtraInputBinding: u32 = 7;

pub(crate) const VertexEntryPoint: &str = "despite_vertex";
pub(crate) const FragmentEntryPoint: &str = "despite_fragment";

pub const EffectSignature: &str = "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32>";

pub const PreludeNames: &[&str] = &[
    "DespiteGlobals",
    "DespiteParams",
    "Globals",
    "Params",
    "Sampler",
    "LinearSampler",
    "NearestSampler",
    "RepeatSampler",
    "Input",
    "sample_input",
    "sample_texture",
    "load_input",
    "load_texture",
    "input_texel_size",
    "blend_over",
    "hash12",
    "value_noise",
    "fbm",
    "effect",
    "despite_vertex",
    "despite_fragment",
];

pub const DefaultEffectSource: &str = r#"fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    let Base = sample_input(Uv);
    let Texel = input_texel_size();
    var Halo = 0.0;
    for (var Step = 0; Step < 16; Step = Step + 1) {
        let Angle = f32(Step) * 0.3926991;
        let Offset = vec2<f32>(cos(Angle), sin(Angle)) * Texel * 3.0;
        Halo = Halo + sample_input(Uv + Offset).a;
    }
    Halo = Halo / 16.0;
    let Pulse = 0.5 + 0.5 * sin(Globals.Time * 3.0);
    let Hue = vec3<f32>(0.55, 0.8, 1.0) + 0.2 * vec3<f32>(sin(Globals.Time * 2.0 + Uv.y * 6.0), 0.0, cos(Globals.Time * 2.0 + Uv.x * 6.0));
    let Glow = vec4<f32>(clamp(Hue, vec3<f32>(0.0), vec3<f32>(1.0)), Halo * (0.35 + 0.35 * Pulse));
    let Shimmer = 0.08 * Pulse * Base.a;
    let Lit = vec4<f32>(min(Base.rgb + vec3<f32>(Shimmer), vec3<f32>(1.0)), Base.a);
    return blend_over(Lit, Glow);
}
"#;

const GlobalsStructSource: &str = r#"struct DespiteGlobals {
    Resolution: vec2<f32>,
    InputResolution: vec2<f32>,
    Time: f32,
    Frame: u32,
    FrameCount: u32,
    Fps: f32,
    Duration: f32,
    Progress: f32,
    LoopProgress: f32,
}
"#;

const HelperSource: &str = r#"fn sample_input(Uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(Input, Sampler, Uv, 0.0);
}

fn sample_texture(Texture: texture_2d<f32>, Uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(Texture, Sampler, Uv, 0.0);
}

fn load_texture(Texture: texture_2d<f32>, Coord: vec2<i32>) -> vec4<f32> {
    let Size = vec2<i32>(textureDimensions(Texture));
    return textureLoad(Texture, clamp(Coord, vec2<i32>(0), Size - vec2<i32>(1)), 0);
}

fn load_input(Coord: vec2<i32>) -> vec4<f32> {
    return load_texture(Input, Coord);
}

fn input_texel_size() -> vec2<f32> {
    return 1.0 / Globals.InputResolution;
}

fn blend_over(Top: vec4<f32>, Bottom: vec4<f32>) -> vec4<f32> {
    let Alpha = Top.a + Bottom.a * (1.0 - Top.a);
    if (Alpha <= 0.0) {
        return vec4<f32>(0.0);
    }
    let Color = (Top.rgb * Top.a + Bottom.rgb * Bottom.a * (1.0 - Top.a)) / Alpha;
    return vec4<f32>(Color, Alpha);
}

fn hash12(Point: vec2<f32>) -> f32 {
    var Mixed = fract(vec3<f32>(Point.x, Point.y, Point.x) * 0.1031);
    Mixed = Mixed + vec3<f32>(dot(Mixed, Mixed.yzx + vec3<f32>(33.33)));
    return fract((Mixed.x + Mixed.y) * Mixed.z);
}

fn value_noise(Point: vec2<f32>) -> f32 {
    let Cell = floor(Point);
    let Local = fract(Point);
    let Smooth = Local * Local * (vec2<f32>(3.0) - 2.0 * Local);
    let A = hash12(Cell);
    let B = hash12(Cell + vec2<f32>(1.0, 0.0));
    let C = hash12(Cell + vec2<f32>(0.0, 1.0));
    let D = hash12(Cell + vec2<f32>(1.0, 1.0));
    return mix(mix(A, B, Smooth.x), mix(C, D, Smooth.x), Smooth.y);
}

fn fbm(Point: vec2<f32>, Octaves: i32) -> f32 {
    var Sum = 0.0;
    var Amplitude = 0.5;
    var Current = Point;
    let Count = clamp(Octaves, 1, 12);
    for (var Octave = 0; Octave < Count; Octave = Octave + 1) {
        Sum = Sum + Amplitude * value_noise(Current);
        Current = Current * 2.0 + vec2<f32>(17.0, 31.0);
        Amplitude = Amplitude * 0.5;
    }
    return Sum;
}

@vertex
fn despite_vertex(@builtin(vertex_index) VertexIndex: u32) -> @builtin(position) vec4<f32> {
    let Corner = vec2<f32>(f32((VertexIndex << 1u) & 2u), f32(VertexIndex & 2u));
    return vec4<f32>(Corner * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn despite_fragment(@builtin(position) Position: vec4<f32>) -> @location(0) vec4<f32> {
    return effect(Position.xy / Globals.Resolution, Position.xy);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlobalsData {
    pub Resolution: [f32; 2],
    pub InputResolution: [f32; 2],
    pub Time: f32,
    pub Frame: u32,
    pub FrameCount: u32,
    pub Fps: f32,
    pub Duration: f32,
    pub Progress: f32,
    pub LoopProgress: f32,
    pub Padding: f32,
}

impl GlobalsData {
    pub fn for_frame(
        OutputSize: (u32, u32),
        InputSize: (u32, u32),
        Frame: u32,
        FrameCount: u32,
        Fps: f32,
    ) -> GlobalsData {
        let ValidFps = Fps.is_finite() && Fps > 0.0;
        let SafeCount = FrameCount.max(1);
        GlobalsData {
            Resolution: [OutputSize.0 as f32, OutputSize.1 as f32],
            InputResolution: [InputSize.0 as f32, InputSize.1 as f32],
            Time: if ValidFps { Frame as f32 / Fps } else { 0.0 },
            Frame,
            FrameCount: SafeCount,
            Fps: if ValidFps { Fps } else { 0.0 },
            Duration: if ValidFps {
                SafeCount as f32 / Fps
            } else {
                0.0
            },
            Progress: if SafeCount > 1 {
                Frame as f32 / (SafeCount - 1) as f32
            } else {
                0.0
            },
            LoopProgress: Frame as f32 / SafeCount as f32,
            Padding: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamField {
    pub Name: String,
    pub WgslType: &'static str,
    pub Offset: u32,
    pub Size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamLayout {
    pub Fields: Vec<ParamField>,
    pub Size: u32,
}

fn align_up(Value: u32, Alignment: u32) -> u32 {
    Value.div_ceil(Alignment) * Alignment
}

pub fn compute_param_layout(Params: &[ShaderParam]) -> ParamLayout {
    let mut Cursor = 0u32;
    let mut Fields = Vec::with_capacity(Params.len());
    for Param in Params {
        let Offset = align_up(Cursor, Param.Value.align());
        let Size = Param.Value.size();
        Fields.push(ParamField {
            Name: Param.Name.clone(),
            WgslType: Param.Value.wgsl_type(),
            Offset,
            Size,
        });
        Cursor = Offset + Size;
    }
    ParamLayout {
        Fields,
        Size: align_up(Cursor, 16).max(16),
    }
}

pub fn pack_params(Params: &[ShaderParam]) -> Vec<u8> {
    let Layout = compute_param_layout(Params);
    let mut Bytes = vec![0u8; Layout.Size as usize];
    for (Field, Param) in Layout.Fields.iter().zip(Params) {
        let Start = Field.Offset as usize;
        let Encoded = Param.Value.to_bytes();
        Bytes[Start..Start + Encoded.len()].copy_from_slice(&Encoded);
    }
    Bytes
}

pub fn build_prelude(Params: &[ShaderParam], InputNames: &[String]) -> String {
    let mut Text = String::new();
    Text.push_str(GlobalsStructSource);
    Text.push('\n');
    Text.push_str("struct DespiteParams {\n");
    if Params.is_empty() {
        Text.push_str("    Padding: u32,\n");
    }
    for Param in Params {
        let _ = writeln!(Text, "    {}: {},", Param.Name, Param.Value.wgsl_type());
    }
    Text.push_str("}\n\n");
    let _ = writeln!(
        Text,
        "@group(0) @binding({GlobalsBinding}) var<uniform> Globals: DespiteGlobals;"
    );
    let _ = writeln!(
        Text,
        "@group(0) @binding({ParamsBinding}) var<uniform> Params: DespiteParams;"
    );
    let _ = writeln!(
        Text,
        "@group(0) @binding({SamplerBinding}) var Sampler: sampler;"
    );
    let _ = writeln!(
        Text,
        "@group(0) @binding({LinearSamplerBinding}) var LinearSampler: sampler;"
    );
    let _ = writeln!(
        Text,
        "@group(0) @binding({NearestSamplerBinding}) var NearestSampler: sampler;"
    );
    let _ = writeln!(
        Text,
        "@group(0) @binding({RepeatSamplerBinding}) var RepeatSampler: sampler;"
    );
    let _ = writeln!(
        Text,
        "@group(0) @binding({InputBinding}) var Input: texture_2d<f32>;"
    );
    for (Index, Name) in InputNames.iter().enumerate() {
        let _ = writeln!(
            Text,
            "@group(0) @binding({}) var {}: texture_2d<f32>;",
            FirstExtraInputBinding + Index as u32,
            Name
        );
    }
    Text.push('\n');
    Text.push_str(HelperSource);
    Text.push('\n');
    Text
}

pub fn validate_identifier(Name: &str) -> Result<(), String> {
    if Name.is_empty() {
        return Err("Name must not be empty".to_string());
    }
    if Name.len() > MaxIdentifierLength {
        return Err(format!(
            "Name `{Name}` is too long (at most {MaxIdentifierLength} characters)"
        ));
    }
    let mut Characters = Name.chars();
    let First = Characters.next().unwrap_or('0');
    if !(First.is_ascii_alphabetic() || First == '_') {
        return Err(format!(
            "Name `{Name}` must start with a letter (A-Z, a-z) or an underscore"
        ));
    }
    if let Some(Bad) =
        Characters.find(|Character| !(Character.is_ascii_alphanumeric() || *Character == '_'))
    {
        return Err(format!(
            "Name `{Name}` contains `{Bad}`; only letters, digits and underscores are allowed"
        ));
    }
    if Name == "_" || Name.starts_with("__") {
        return Err(format!(
            "Name `{Name}` is reserved in WGSL (`_` alone and names starting with `__` are not allowed)"
        ));
    }
    if naga::keywords::wgsl::RESERVED.contains(&Name) {
        return Err(format!("Name `{Name}` is a reserved WGSL keyword"));
    }
    if naga::keywords::wgsl::BUILTIN_IDENTIFIERS.contains(&Name) {
        return Err(format!(
            "Name `{Name}` is a built-in WGSL type or function name"
        ));
    }
    if PreludeNames.contains(&Name) {
        return Err(format!(
            "Name `{Name}` is already used by the Despite shader prelude"
        ));
    }
    Ok(())
}

pub(crate) fn line_count(Text: &str) -> u32 {
    Text.bytes().filter(|Byte| *Byte == b'\n').count() as u32
}
