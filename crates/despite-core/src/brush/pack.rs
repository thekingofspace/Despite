use serde::{Deserialize, Serialize};

use super::Brush;
use crate::error::{Error, Result};

pub const PackExtension: &str = "despitebrush";
pub const PackFormat: &str = "despite-brush";
pub const PackVersion: u32 = 1;
const MaxPackBytes: usize = 32 * 1024 * 1024;
const MaxBrushes: usize = 512;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrushPack {
    pub Format: String,
    pub Version: u32,
    pub Name: String,
    pub Author: String,
    pub Description: String,
    pub Brushes: Vec<Brush>,
}

impl Default for BrushPack {
    fn default() -> Self {
        Self { Format: PackFormat.into(), Version: PackVersion, Name: "Brushes".into(), Author: String::new(), Description: String::new(), Brushes: Vec::new() }
    }
}

fn sanitize(Source: &mut Brush) {
    Source.Size = Source.Size.clamp(0.5, 1000.0);
    Source.Opacity = Source.Opacity.clamp(0.0, 1.0);
    Source.Flow = Source.Flow.clamp(0.0, 1.0);
    Source.Hardness = Source.Hardness.clamp(0.0, 1.0);
    Source.Spacing = Source.Spacing.clamp(0.01, 10.0);
    Source.Roundness = Source.Roundness.clamp(0.05, 1.0);
    Source.Smoothing = Source.Smoothing.clamp(0.0, 0.95);
    Source.DabCount = Source.DabCount.clamp(1, 64);
    Source.Scatter = Source.Scatter.clamp(0.0, 50.0);
    Source.Advanced.BoilFrames = Source.Advanced.BoilFrames.min(12);
    Source.Advanced.EchoCount = Source.Advanced.EchoCount.min(16);
    Source.Advanced.AnimatedTips.truncate(64);
    let Valid = |Tip: &super::TipImage| Tip.Width > 0 && Tip.Height > 0 && Tip.Width <= 1024 && Tip.Height <= 1024 && Tip.Alpha.len() == (Tip.Width * Tip.Height) as usize;
    if Source.TipImage.as_ref().is_some_and(|Tip| !Valid(Tip)) {
        Source.TipImage = None;
        if Source.Tip == super::TipShape::Image {
            Source.Tip = super::TipShape::Round;
        }
    }
    Source.Advanced.AnimatedTips.retain(Valid);
    if Source.Name.trim().is_empty() {
        Source.Name = "Brush".into();
    }
    Source.Name.truncate(80);
}

impl BrushPack {
    pub fn new(Name: impl Into<String>, Brushes: Vec<Brush>) -> Self {
        Self { Name: Name.into(), Brushes, ..Default::default() }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    pub fn from_bytes(Bytes: &[u8]) -> Result<BrushPack> {
        if Bytes.len() > MaxPackBytes {
            return Err(Error::InvalidData("brush file is larger than 32 MiB".into()));
        }
        let Value: serde_json::Value = serde_json::from_slice(Bytes)?;
        let mut Pack = if Value.get("Brushes").is_some() {
            let Pack: BrushPack = serde_json::from_value(Value)?;
            if Pack.Format != PackFormat {
                return Err(Error::InvalidData(format!("not a Despite brush file ({})", Pack.Format)));
            }
            if Pack.Version > PackVersion {
                return Err(Error::UnsupportedVersion(Pack.Version));
            }
            Pack
        } else {
            let Single: Brush = serde_json::from_value(Value)?;
            BrushPack::new(Single.Name.clone(), vec![Single])
        };
        if Pack.Brushes.is_empty() {
            return Err(Error::InvalidData("the brush file contains no brushes".into()));
        }
        Pack.Brushes.truncate(MaxBrushes);
        for Entry in &mut Pack.Brushes {
            sanitize(Entry);
        }
        if Pack.Name.trim().is_empty() {
            Pack.Name = Pack.Brushes[0].Name.clone();
        }
        Pack.Name.truncate(80);
        Ok(Pack)
    }

    pub fn file_stem(&self) -> String {
        crate::export::sanitize_file_name(&self.Name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::advanced_presets;

    #[test]
    fn roundtrip_with_animated_tips() {
        let Brushes = advanced_presets();
        let Pack = BrushPack::new("Fancy", Brushes.clone());
        let Bytes = Pack.to_bytes().unwrap();
        let Loaded = BrushPack::from_bytes(&Bytes).unwrap();
        assert_eq!(Loaded.Brushes.len(), Brushes.len());
        let Stars = Loaded.Brushes.iter().find(|Entry| !Entry.Advanced.AnimatedTips.is_empty()).unwrap();
        assert!(Stars.Advanced.AnimatedTips.len() > 1);
        assert_eq!(Loaded.Brushes, Brushes);
    }

    #[test]
    fn accepts_single_brush_json_and_rejects_garbage() {
        let Single = serde_json::to_vec(&Brush { Name: "Solo".into(), ..Default::default() }).unwrap();
        let Loaded = BrushPack::from_bytes(&Single).unwrap();
        assert_eq!(Loaded.Name, "Solo");
        assert!(BrushPack::from_bytes(b"{\"Format\":\"other\",\"Brushes\":[{}]}").is_err());
        assert!(BrushPack::from_bytes(b"not json").is_err());
    }

    #[test]
    fn sanitizes_hostile_values() {
        let mut Wild = Brush { Size: 1e9, DabCount: 100_000, ..Default::default() };
        Wild.Advanced.BoilFrames = 9999;
        Wild.TipImage = Some(super::super::TipImage { Width: 4, Height: 4, Alpha: vec![0; 3] });
        let Bytes = BrushPack::new("X", vec![Wild]).to_bytes().unwrap();
        let Loaded = BrushPack::from_bytes(&Bytes).unwrap();
        let Entry = &Loaded.Brushes[0];
        assert_eq!(Entry.Size, 1000.0);
        assert_eq!(Entry.DabCount, 64);
        assert_eq!(Entry.Advanced.BoilFrames, 12);
        assert!(Entry.TipImage.is_none());
    }
}
