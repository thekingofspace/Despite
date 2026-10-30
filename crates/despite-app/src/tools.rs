use despite_core::brush::DitherPattern;
use despite_core::fill::GradientKind;
use despite_core::{BlendMode, SelectionOp};
use egui::Key;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    Brush,
    Pencil,
    Eraser,
    Fill,
    Gradient,
    Text,
    Picker,
    Line,
    Rectangle,
    Ellipse,
    RectSelect,
    EllipseSelect,
    Lasso,
    MagicWand,
    Move,
    Symmetry,
    Pan,
}

impl Tool {
    pub const All: &'static [Tool] = &[
        Tool::Brush,
        Tool::Pencil,
        Tool::Eraser,
        Tool::Fill,
        Tool::Gradient,
        Tool::Text,
        Tool::Picker,
        Tool::Line,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::RectSelect,
        Tool::EllipseSelect,
        Tool::Lasso,
        Tool::MagicWand,
        Tool::Move,
        Tool::Symmetry,
        Tool::Pan,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Brush => "Brush",
            Tool::Pencil => "Pixel Pencil",
            Tool::Eraser => "Eraser",
            Tool::Fill => "Bucket Fill",
            Tool::Gradient => "Gradient",
            Tool::Text => "Text",
            Tool::Picker => "Color Picker",
            Tool::Line => "Line",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::RectSelect => "Rectangle Select",
            Tool::EllipseSelect => "Ellipse Select",
            Tool::Lasso => "Lasso Select",
            Tool::MagicWand => "Magic Wand",
            Tool::Move => "Move / Transform",
            Tool::Symmetry => "Symmetry Ruler",
            Tool::Pan => "Pan",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Tool::Brush => "Brush",
            Tool::Pencil => "Pencil",
            Tool::Eraser => "Eraser",
            Tool::Fill => "Fill",
            Tool::Gradient => "Gradient",
            Tool::Text => "Text",
            Tool::Picker => "Picker",
            Tool::Line => "Line",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::RectSelect => "RectSelect",
            Tool::EllipseSelect => "EllipseSelect",
            Tool::Lasso => "Lasso",
            Tool::MagicWand => "MagicWand",
            Tool::Move => "Move",
            Tool::Symmetry => "Symmetry",
            Tool::Pan => "Pan",
        }
    }


    pub fn shortcut(self) -> Option<(Key, bool)> {
        Some(match self {
            Tool::Brush => (Key::B, false),
            Tool::Pencil => (Key::N, false),
            Tool::Eraser => (Key::E, false),
            Tool::Fill => (Key::G, false),
            Tool::Gradient => (Key::G, true),
            Tool::Text => (Key::T, false),
            Tool::Picker => (Key::I, false),
            Tool::Line => (Key::L, false),
            Tool::Rectangle => (Key::U, false),
            Tool::Ellipse => (Key::O, false),
            Tool::RectSelect => (Key::M, false),
            Tool::EllipseSelect => (Key::M, true),
            Tool::Lasso => (Key::Q, false),
            Tool::MagicWand => (Key::W, false),
            Tool::Move => (Key::V, false),
            Tool::Symmetry => (Key::Y, false),
            Tool::Pan => (Key::H, false),
        })
    }

    pub fn shortcut_label(self) -> String {
        match self.shortcut() {
            Some((Pressed, true)) => format!("Shift+{}", Pressed.name()),
            Some((Pressed, false)) => Pressed.name().to_string(),
            None => String::new(),
        }
    }

    pub fn is_painting(self) -> bool {
        matches!(self, Tool::Brush | Tool::Pencil | Tool::Eraser)
    }


    pub fn works_on_scene(self) -> bool {
        matches!(self, Tool::Move | Tool::Pan | Tool::Picker)
    }

    pub fn hint(self) -> &'static str {
        match self {
            Tool::Brush => "Paint with the current brush. Alt-click picks a color. Shift-click draws a straight line from the last point.",
            Tool::Pencil => "Pixel-perfect pencil for pixel art. Removes L-shaped corners automatically.",
            Tool::Eraser => "Erase with its own brush settings.",
            Tool::Fill => "Flood fill an area. Set tolerance, contiguous, sample merged and grow.",
            Tool::Gradient => "Drag to draw a gradient from primary to secondary color. Use steps + dither for pixel art.",
            Tool::Text => "Click to type text with any installed font. Click an existing text layer to edit it.",
            Tool::Picker => "Click to pick the primary color, right-click for secondary.",
            Tool::Line => "Drag to draw a line with the current brush. Shift snaps the angle.",
            Tool::Rectangle => "Drag to draw a rectangle. Shift makes a square.",
            Tool::Ellipse => "Drag to draw an ellipse. Shift makes a circle.",
            Tool::RectSelect => "Drag to select a rectangle. Shift adds, Alt subtracts.",
            Tool::EllipseSelect => "Drag to select an ellipse. Shift adds, Alt subtracts.",
            Tool::Lasso => "Draw a freehand selection. Shift adds, Alt subtracts.",
            Tool::MagicWand => "Select similar colors. Shift adds, Alt subtracts.",
            Tool::Move => "Drag to move the active layer, or scene items in a scene.",
            Tool::Symmetry => "Click or drag to place the symmetry center used by all painting tools.",
            Tool::Pan => "Drag to pan. Middle mouse or Space also pans anywhere.",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionMode {
    #[default]
    Replace,
    Add,
    Subtract,
    Intersect,
}

impl SelectionMode {
    pub const All: &'static [SelectionMode] = &[SelectionMode::Replace, SelectionMode::Add, SelectionMode::Subtract, SelectionMode::Intersect];

    pub fn name(self) -> &'static str {
        match self {
            SelectionMode::Replace => "Replace",
            SelectionMode::Add => "Add",
            SelectionMode::Subtract => "Subtract",
            SelectionMode::Intersect => "Intersect",
        }
    }

    pub fn op(self) -> SelectionOp {
        match self {
            SelectionMode::Replace => SelectionOp::Replace,
            SelectionMode::Add => SelectionOp::Add,
            SelectionMode::Subtract => SelectionOp::Subtract,
            SelectionMode::Intersect => SelectionOp::Intersect,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolOptions {
    pub FillTolerance: f32,
    pub FillContiguous: bool,
    pub FillSampleMerged: bool,
    pub FillExpand: u32,
    pub FillOpacity: f32,
    pub FillBlend: BlendMode,
    pub WandTolerance: f32,
    pub WandContiguous: bool,
    pub WandSampleMerged: bool,
    pub Selection: SelectionMode,
    pub SelectionAntialias: bool,
    pub ShapeFilled: bool,
    pub ShapePixelExact: bool,
    pub GradientKind: GradientKind,
    pub GradientDither: DitherPattern,
    pub GradientSteps: u32,
    pub GradientOpacity: f32,
    pub GradientBlend: BlendMode,
    pub PickerMerged: bool,
    pub MoveSelectsItems: bool,
    pub Text: despite_core::text::TextStyle,
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            FillTolerance: 0.0,
            FillContiguous: true,
            FillSampleMerged: false,
            FillExpand: 0,
            FillOpacity: 1.0,
            FillBlend: BlendMode::Normal,
            WandTolerance: 0.05,
            WandContiguous: true,
            WandSampleMerged: false,
            Selection: SelectionMode::Replace,
            SelectionAntialias: false,
            ShapeFilled: false,
            ShapePixelExact: true,
            GradientKind: GradientKind::Linear,
            GradientDither: DitherPattern::None,
            GradientSteps: 0,
            GradientOpacity: 1.0,
            GradientBlend: BlendMode::Normal,
            PickerMerged: true,
            MoveSelectsItems: true,
            Text: despite_core::text::TextStyle::default(),
        }
    }
}
