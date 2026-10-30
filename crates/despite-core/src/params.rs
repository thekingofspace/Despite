use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::image::ImageId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamKind {
    Float { Min: f32, Max: f32 },
    Int { Min: i32, Max: i32 },
    Bool,
    Color,
    Vec2,
    Image,
    Text,
}

impl ParamKind {
    pub const Names: &'static [&'static str] = &["Float", "Int", "Bool", "Color", "Vec2", "Image", "Text"];

    pub fn name(&self) -> &'static str {
        match self {
            ParamKind::Float { .. } => "Float",
            ParamKind::Int { .. } => "Int",
            ParamKind::Bool => "Bool",
            ParamKind::Color => "Color",
            ParamKind::Vec2 => "Vec2",
            ParamKind::Image => "Image",
            ParamKind::Text => "Text",
        }
    }

    pub fn from_name(Name: &str) -> Option<ParamKind> {
        Some(match Name.to_ascii_lowercase().as_str() {
            "float" => ParamKind::Float { Min: 0.0, Max: 1.0 },
            "int" => ParamKind::Int { Min: 0, Max: 100 },
            "bool" => ParamKind::Bool,
            "color" => ParamKind::Color,
            "vec2" => ParamKind::Vec2,
            "image" => ParamKind::Image,
            "text" => ParamKind::Text,
            _ => return None,
        })
    }

    pub fn default_value(&self) -> ParamValue {
        match self {
            ParamKind::Float { Min, Max } => ParamValue::Float(((Min + Max) * 0.5).clamp(*Min, *Max)),
            ParamKind::Int { Min, .. } => ParamValue::Int(*Min),
            ParamKind::Bool => ParamValue::Bool(false),
            ParamKind::Color => ParamValue::Color(Color::White),
            ParamKind::Vec2 => ParamValue::Vec2([0.0, 0.0]),
            ParamKind::Image => ParamValue::Image(None),
            ParamKind::Text => ParamValue::Text(String::new()),
        }
    }

    pub fn accepts(&self, Value: &ParamValue) -> bool {
        matches!(
            (self, Value),
            (ParamKind::Float { .. }, ParamValue::Float(_))
                | (ParamKind::Int { .. }, ParamValue::Int(_))
                | (ParamKind::Bool, ParamValue::Bool(_))
                | (ParamKind::Color, ParamValue::Color(_))
                | (ParamKind::Vec2, ParamValue::Vec2(_))
                | (ParamKind::Image, ParamValue::Image(_))
                | (ParamKind::Text, ParamValue::Text(_))
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    Float(f32),
    Int(i32),
    Bool(bool),
    Color(Color),
    Vec2([f32; 2]),
    Image(Option<ImageId>),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamDecl {
    pub Name: String,
    pub Label: String,
    pub Kind: ParamKind,
    pub Value: ParamValue,
}

impl ParamDecl {
    pub fn new(Name: impl Into<String>, Kind: ParamKind) -> Self {
        let Name = Name.into();
        let Value = Kind.default_value();
        Self { Label: Name.clone(), Name, Kind, Value }
    }

    pub fn with_value(mut self, Value: ParamValue) -> Self {
        self.Value = Value;
        self.sanitize();
        self
    }

    pub fn display_label(&self) -> &str {
        if self.Label.trim().is_empty() { &self.Name } else { &self.Label }
    }

    pub fn sanitize(&mut self) {
        if !self.Kind.accepts(&self.Value) {
            self.Value = self.Kind.default_value();
        }
        match (&mut self.Kind, &mut self.Value) {
            (ParamKind::Float { Min, Max }, ParamValue::Float(Current)) => {
                if *Min > *Max {
                    std::mem::swap(Min, Max);
                }
                *Current = Current.clamp(*Min, *Max);
            }
            (ParamKind::Int { Min, Max }, ParamValue::Int(Current)) => {
                if *Min > *Max {
                    std::mem::swap(Min, Max);
                }
                *Current = (*Current).clamp(*Min, *Max);
            }
            _ => {}
        }
    }
}

pub fn is_valid_identifier(Name: &str) -> bool {
    let mut Characters = Name.chars();
    match Characters.next() {
        Some(First) if First.is_ascii_alphabetic() || First == '_' => {}
        _ => return false,
    }
    Characters.all(|Character| Character.is_ascii_alphanumeric() || Character == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_clamps_and_fixes_kind() {
        let mut Decl = ParamDecl::new("Strength", ParamKind::Float { Min: 0.0, Max: 2.0 }).with_value(ParamValue::Float(5.0));
        assert_eq!(Decl.Value, ParamValue::Float(2.0));
        Decl.Value = ParamValue::Bool(true);
        Decl.sanitize();
        assert_eq!(Decl.Value, ParamValue::Float(1.0));
    }

    #[test]
    fn identifiers() {
        assert!(is_valid_identifier("Glow_Amount2"));
        assert!(!is_valid_identifier("2fast"));
        assert!(!is_valid_identifier("has space"));
        assert!(!is_valid_identifier(""));
    }
}
