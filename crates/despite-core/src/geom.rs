use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct IRect {
    pub X: i32,
    pub Y: i32,
    pub W: i32,
    pub H: i32,
}

impl IRect {
    pub const Empty: IRect = IRect { X: 0, Y: 0, W: 0, H: 0 };

    pub const fn new(X: i32, Y: i32, W: i32, H: i32) -> Self {
        Self { X, Y, W, H }
    }

    pub fn from_min_max(X0: i32, Y0: i32, X1: i32, Y1: i32) -> Self {
        Self { X: X0, Y: Y0, W: X1 - X0, H: Y1 - Y0 }
    }

    pub fn from_f32_bounds(X0: f32, Y0: f32, X1: f32, Y1: f32) -> Self {
        Self::from_min_max(X0.floor() as i32, Y0.floor() as i32, X1.ceil() as i32, Y1.ceil() as i32)
    }

    pub fn sized(W: u32, H: u32) -> Self {
        Self { X: 0, Y: 0, W: W as i32, H: H as i32 }
    }

    pub fn is_empty(&self) -> bool {
        self.W <= 0 || self.H <= 0
    }

    pub fn right(&self) -> i32 {
        self.X + self.W
    }

    pub fn bottom(&self) -> i32 {
        self.Y + self.H
    }

    pub fn area(&self) -> u64 {
        if self.is_empty() { 0 } else { self.W as u64 * self.H as u64 }
    }

    pub fn contains(&self, X: i32, Y: i32) -> bool {
        X >= self.X && Y >= self.Y && X < self.right() && Y < self.bottom()
    }

    pub fn intersect(&self, Other: &IRect) -> IRect {
        let X0 = self.X.max(Other.X);
        let Y0 = self.Y.max(Other.Y);
        let X1 = self.right().min(Other.right());
        let Y1 = self.bottom().min(Other.bottom());
        if X1 <= X0 || Y1 <= Y0 { IRect::Empty } else { IRect::from_min_max(X0, Y0, X1, Y1) }
    }

    pub fn union(&self, Other: &IRect) -> IRect {
        if self.is_empty() {
            return *Other;
        }
        if Other.is_empty() {
            return *self;
        }
        IRect::from_min_max(
            self.X.min(Other.X),
            self.Y.min(Other.Y),
            self.right().max(Other.right()),
            self.bottom().max(Other.bottom()),
        )
    }

    pub fn expand(&self, Amount: i32) -> IRect {
        if self.is_empty() {
            return *self;
        }
        IRect::new(self.X - Amount, self.Y - Amount, self.W + 2 * Amount, self.H + 2 * Amount)
    }

    pub fn translate(&self, Dx: i32, Dy: i32) -> IRect {
        IRect::new(self.X + Dx, self.Y + Dy, self.W, self.H)
    }

    pub fn clamp_to(&self, W: u32, H: u32) -> IRect {
        self.intersect(&IRect::sized(W, H))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersect_and_union() {
        let First = IRect::new(0, 0, 10, 10);
        let Second = IRect::new(5, 5, 10, 10);
        assert_eq!(First.intersect(&Second), IRect::new(5, 5, 5, 5));
        assert_eq!(First.union(&Second), IRect::new(0, 0, 15, 15));
        assert!(First.intersect(&IRect::new(20, 20, 1, 1)).is_empty());
        assert_eq!(IRect::Empty.union(&First), First);
    }
}
