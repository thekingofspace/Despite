#[derive(Clone, Debug)]
pub struct Rng {
    State: u64,
}

impl Rng {
    pub fn new(Seed: u64) -> Self {
        Self { State: Seed ^ 0x9E37_79B9_7F4A_7C15 }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.State = self.State.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut Mixed = self.State;
        Mixed = (Mixed ^ (Mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        Mixed = (Mixed ^ (Mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Mixed ^ (Mixed >> 31)
    }

    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn signed(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }

    pub fn below(&mut self, Count: usize) -> usize {
        if Count == 0 { 0 } else { (self.next_u64() % Count as u64) as usize }
    }
}

pub fn hash_noise(X: i32, Y: i32, Seed: u64) -> f32 {
    let mut Generator = Rng::new(Seed ^ (((X as u32 as u64) << 32) | Y as u32 as u64));
    Generator.next_f32()
}
