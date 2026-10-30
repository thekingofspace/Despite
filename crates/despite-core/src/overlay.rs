use serde::{Deserialize, Serialize};

use crate::image::ImageId;
use crate::scene::SceneId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OverlaySource {
    Image(ImageId),
    Scene(SceneId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Overlay {
    pub Source: Option<OverlaySource>,
    pub Opacity: f32,
    pub OffsetX: f32,
    pub OffsetY: f32,
    pub Above: bool,
    pub Visible: bool,
    pub Frame: u32,
}

impl Default for Overlay {
    fn default() -> Self {
        Self { Source: None, Opacity: 0.35, OffsetX: 0.0, OffsetY: 0.0, Above: false, Visible: true, Frame: 0 }
    }
}

impl Overlay {
    pub fn new(Source: OverlaySource) -> Self {
        Self { Source: Some(Source), ..Default::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OnionSkin {
    pub Enabled: bool,
    pub Before: u32,
    pub After: u32,
    pub Opacity: f32,
    pub Tint: bool,
}

impl Default for OnionSkin {
    fn default() -> Self {
        Self { Enabled: false, Before: 1, After: 1, Opacity: 0.3, Tint: true }
    }
}
