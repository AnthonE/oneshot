//! FNV-1a over the world's state, field by field in a fixed order: the
//! number native and wasm must agree on (docs/DESIGN.md §17).

pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    pub fn u8(&mut self, v: u8) {
        self.0 ^= v as u64;
        self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
    }

    pub fn u16(&mut self, v: u16) {
        for b in v.to_le_bytes() {
            self.u8(b);
        }
    }

    pub fn u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.u8(b);
        }
    }

    pub fn u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.u8(b);
        }
    }

    pub fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }

    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    pub fn v3(&mut self, v: crate::geom::V3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }

    pub fn bool(&mut self, v: bool) {
        self.u8(v as u8);
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}
