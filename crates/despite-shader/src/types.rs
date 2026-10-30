use std::fmt;

#[derive(Clone, PartialEq, Eq, Default)]
pub struct ShaderImage {
    pub Width: u32,
    pub Height: u32,
    pub Rgba: Vec<u8>,
}

impl ShaderImage {
    pub fn new(Width: u32, Height: u32, Rgba: Vec<u8>) -> ShaderImage {
        ShaderImage {
            Width,
            Height,
            Rgba,
        }
    }

    pub fn filled(Width: u32, Height: u32, Color: [u8; 4]) -> ShaderImage {
        let PixelCount = Width as usize * Height as usize;
        let mut Rgba = Vec::with_capacity(PixelCount * 4);
        for _ in 0..PixelCount {
            Rgba.extend_from_slice(&Color);
        }
        ShaderImage {
            Width,
            Height,
            Rgba,
        }
    }

    pub fn expected_len(&self) -> usize {
        self.Width as usize * self.Height as usize * 4
    }

    pub fn pixel(&self, X: u32, Y: u32) -> Option<[u8; 4]> {
        if X >= self.Width || Y >= self.Height {
            return None;
        }
        let Start = (Y as usize * self.Width as usize + X as usize) * 4;
        let Slice = self.Rgba.get(Start..Start + 4)?;
        Some([Slice[0], Slice[1], Slice[2], Slice[3]])
    }
}

impl fmt::Debug for ShaderImage {
    fn fmt(&self, Formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        Formatter
            .debug_struct("ShaderImage")
            .field("Width", &self.Width)
            .field("Height", &self.Height)
            .field("RgbaLen", &self.Rgba.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShaderValue {
    Float(f32),
    Int(i32),
    Bool(bool),
    Vec2([f32; 2]),
    Color([f32; 4]),
}

impl ShaderValue {
    pub fn wgsl_type(&self) -> &'static str {
        match self {
            ShaderValue::Float(_) => "f32",
            ShaderValue::Int(_) => "i32",
            ShaderValue::Bool(_) => "u32",
            ShaderValue::Vec2(_) => "vec2<f32>",
            ShaderValue::Color(_) => "vec4<f32>",
        }
    }

    pub fn align(&self) -> u32 {
        match self {
            ShaderValue::Float(_) | ShaderValue::Int(_) | ShaderValue::Bool(_) => 4,
            ShaderValue::Vec2(_) => 8,
            ShaderValue::Color(_) => 16,
        }
    }

    pub fn size(&self) -> u32 {
        match self {
            ShaderValue::Float(_) | ShaderValue::Int(_) | ShaderValue::Bool(_) => 4,
            ShaderValue::Vec2(_) => 8,
            ShaderValue::Color(_) => 16,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            ShaderValue::Float(Value) => Value.to_le_bytes().to_vec(),
            ShaderValue::Int(Value) => Value.to_le_bytes().to_vec(),
            ShaderValue::Bool(Value) => u32::from(*Value).to_le_bytes().to_vec(),
            ShaderValue::Vec2(Values) => Values
                .iter()
                .flat_map(|Value| Value.to_le_bytes())
                .collect(),
            ShaderValue::Color(Values) => Values
                .iter()
                .flat_map(|Value| Value.to_le_bytes())
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderParam {
    pub Name: String,
    pub Value: ShaderValue,
}

impl ShaderParam {
    pub fn new(Name: impl Into<String>, Value: ShaderValue) -> ShaderParam {
        ShaderParam {
            Name: Name.into(),
            Value,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderInput {
    pub Name: String,
    pub Image: ShaderImage,
}

impl ShaderInput {
    pub fn new(Name: impl Into<String>, Image: ShaderImage) -> ShaderInput {
        ShaderInput {
            Name: Name.into(),
            Image,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderJob {
    pub Source: String,
    pub Main: ShaderImage,
    pub OutputSize: Option<(u32, u32)>,
    pub Inputs: Vec<ShaderInput>,
    pub Params: Vec<ShaderParam>,
    pub FrameCount: u32,
    pub Fps: f32,
    pub SmoothSampling: bool,
    pub OnlyFrame: Option<u32>,
}

impl ShaderJob {
    pub fn new(Source: impl Into<String>, Main: ShaderImage) -> ShaderJob {
        ShaderJob {
            Source: Source.into(),
            Main,
            OutputSize: None,
            Inputs: Vec::new(),
            Params: Vec::new(),
            FrameCount: 1,
            Fps: 12.0,
            SmoothSampling: true,
            OnlyFrame: None,
        }
    }

    pub fn output_size(&self) -> (u32, u32) {
        self.OutputSize
            .unwrap_or((self.Main.Width, self.Main.Height))
    }

    pub fn input_names(&self) -> Vec<String> {
        self.Inputs.iter().map(|Input| Input.Name.clone()).collect()
    }

    pub fn frame_time(&self, Frame: u32) -> f32 {
        if self.Fps.is_finite() && self.Fps > 0.0 {
            Frame as f32 / self.Fps
        } else {
            0.0
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ShaderOutput {
    pub Frames: Vec<ShaderImage>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{}", describe_error(.Message, *.Line, *.Column))]
pub struct ShaderError {
    pub Message: String,
    pub Line: Option<u32>,
    pub Column: Option<u32>,
}

impl ShaderError {
    pub fn new(Message: impl Into<String>) -> ShaderError {
        ShaderError {
            Message: Message.into(),
            Line: None,
            Column: None,
        }
    }

    pub fn at(Message: impl Into<String>, Line: Option<u32>, Column: Option<u32>) -> ShaderError {
        ShaderError {
            Message: Message.into(),
            Line,
            Column,
        }
    }
}

fn describe_error(Message: &str, Line: Option<u32>, Column: Option<u32>) -> String {
    match (Line, Column) {
        (Some(Line), Some(Column)) => format!("line {Line}, column {Column}: {Message}"),
        (Some(Line), None) => format!("line {Line}: {Message}"),
        _ => Message.to_string(),
    }
}
