use serde::{Deserialize, Serialize};

use crate::geom::IRect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub A: f32,
    pub B: f32,
    pub C: f32,
    pub D: f32,
    pub Tx: f32,
    pub Ty: f32,
}

impl Affine {
    pub const Identity: Affine = Affine { A: 1.0, B: 0.0, C: 0.0, D: 1.0, Tx: 0.0, Ty: 0.0 };

    pub fn translate(X: f32, Y: f32) -> Self {
        Affine { Tx: X, Ty: Y, ..Self::Identity }
    }

    pub fn scale(ScaleX: f32, ScaleY: f32) -> Self {
        Affine { A: ScaleX, D: ScaleY, ..Self::Identity }
    }

    pub fn rotate_deg(Degrees: f32) -> Self {
        let (Sin, Cos) = Degrees.to_radians().sin_cos();
        Affine { A: Cos, B: Sin, C: -Sin, D: Cos, Tx: 0.0, Ty: 0.0 }
    }

    pub fn then(self, Next: Affine) -> Affine {
        Next.mul(&self)
    }

    pub fn mul(&self, Other: &Affine) -> Affine {
        Affine {
            A: self.A * Other.A + self.C * Other.B,
            B: self.B * Other.A + self.D * Other.B,
            C: self.A * Other.C + self.C * Other.D,
            D: self.B * Other.C + self.D * Other.D,
            Tx: self.A * Other.Tx + self.C * Other.Ty + self.Tx,
            Ty: self.B * Other.Tx + self.D * Other.Ty + self.Ty,
        }
    }

    #[inline]
    pub fn apply(&self, X: f32, Y: f32) -> (f32, f32) {
        (self.A * X + self.C * Y + self.Tx, self.B * X + self.D * Y + self.Ty)
    }

    #[inline]
    pub fn apply_vector(&self, X: f32, Y: f32) -> (f32, f32) {
        (self.A * X + self.C * Y, self.B * X + self.D * Y)
    }

    pub fn inverse(&self) -> Option<Affine> {
        let Determinant = self.A * self.D - self.B * self.C;
        if Determinant.abs() < 1e-12 {
            return None;
        }
        let Inverse = 1.0 / Determinant;
        let A = self.D * Inverse;
        let B = -self.B * Inverse;
        let C = -self.C * Inverse;
        let D = self.A * Inverse;
        Some(Affine { A, B, C, D, Tx: -(A * self.Tx + C * self.Ty), Ty: -(B * self.Tx + D * self.Ty) })
    }

    pub fn bounds_of(&self, Width: f32, Height: f32) -> IRect {
        let Corners = [self.apply(0.0, 0.0), self.apply(Width, 0.0), self.apply(0.0, Height), self.apply(Width, Height)];
        let (mut X0, mut Y0, mut X1, mut Y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (X, Y) in Corners {
            X0 = X0.min(X);
            Y0 = Y0.min(Y);
            X1 = X1.max(X);
            Y1 = Y1.max(Y);
        }
        IRect::from_f32_bounds(X0, Y0, X1, Y1)
    }

    pub fn mean_scale(&self) -> f32 {
        ((self.A * self.A + self.B * self.B).sqrt() + (self.C * self.C + self.D * self.D).sqrt()) * 0.5
    }

    pub fn rotation_deg(&self) -> f32 {
        self.B.atan2(self.A).to_degrees()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerTransform {
    pub OffsetX: f32,
    pub OffsetY: f32,
    pub ScaleX: f32,
    pub ScaleY: f32,
    pub Rotation: f32,
    pub FlipX: bool,
    pub FlipY: bool,
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self { OffsetX: 0.0, OffsetY: 0.0, ScaleX: 1.0, ScaleY: 1.0, Rotation: 0.0, FlipX: false, FlipY: false }
    }
}

impl LayerTransform {
    pub fn is_identity(&self) -> bool {
        *self == LayerTransform::default()
    }

    pub fn integer_offset(&self) -> Option<(i32, i32)> {
        let WithoutOffset = LayerTransform { OffsetX: 0.0, OffsetY: 0.0, ..*self };
        if WithoutOffset.is_identity() && self.OffsetX.fract() == 0.0 && self.OffsetY.fract() == 0.0 {
            Some((self.OffsetX as i32, self.OffsetY as i32))
        } else {
            None
        }
    }

    pub fn matrix(&self, LayerWidth: u32, LayerHeight: u32) -> Affine {
        let CenterX = LayerWidth as f32 * 0.5;
        let CenterY = LayerHeight as f32 * 0.5;
        let ScaleX = self.ScaleX * if self.FlipX { -1.0 } else { 1.0 };
        let ScaleY = self.ScaleY * if self.FlipY { -1.0 } else { 1.0 };
        Affine::translate(-CenterX, -CenterY)
            .then(Affine::scale(ScaleX, ScaleY))
            .then(Affine::rotate_deg(self.Rotation))
            .then(Affine::translate(CenterX + self.OffsetX, CenterY + self.OffsetY))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_roundtrip() {
        let Transform = LayerTransform {
            OffsetX: 5.0,
            OffsetY: -3.0,
            ScaleX: 2.0,
            ScaleY: 0.5,
            Rotation: 30.0,
            FlipX: true,
            ..Default::default()
        };
        let Matrix = Transform.matrix(10, 20);
        let Inverse = Matrix.inverse().unwrap();
        let (X, Y) = Matrix.apply(3.0, 7.0);
        let (BackX, BackY) = Inverse.apply(X, Y);
        assert!((BackX - 3.0).abs() < 1e-4 && (BackY - 7.0).abs() < 1e-4);
    }

    #[test]
    fn identity_and_offset() {
        let mut Transform = LayerTransform::default();
        assert_eq!(Transform.matrix(4, 4).apply(1.0, 2.0), (1.0, 2.0));
        Transform.OffsetX = 3.0;
        assert_eq!(Transform.integer_offset(), Some((3, 0)));
        Transform.Rotation = 90.0;
        assert_eq!(Transform.integer_offset(), None);
    }
}
