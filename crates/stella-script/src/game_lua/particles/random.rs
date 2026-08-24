//! Process-global xorshift/CMWC generator (`sub_10057B42C`).

#[derive(Debug, Clone)]
pub(crate) struct NativeParticleRandom {
    initialized: bool,
    pub(crate) index: u32,
    carry: u32,
    values: Box<[u32; 4096]>,
}

impl Default for NativeParticleRandom {
    fn default() -> Self {
        Self {
            initialized: false,
            index: 0,
            carry: 0,
            values: Box::new([0; 4096]),
        }
    }
}

impl NativeParticleRandom {
    fn initialize(&mut self) {
        // 0x10057B454..0x10057B470 builds these immediates with MOV/MOVK.
        // They are numeric words, not byte strings; byte-reversing each word
        // changes every particle and ThemeLayer worldW/worldH sample.
        let mut x = 0x075b_cd15_u32;
        let mut y = 0x159a_55e5_u32;
        let mut z = 0x1f12_3bb5_u32;
        let mut w = 0x0549_1333_u32;
        for value in self.values.iter_mut() {
            let mixed = x ^ x.wrapping_shl(11);
            x = y;
            y = z;
            z = w;
            w = z ^ (z >> 19) ^ mixed ^ (mixed >> 8);
            *value = w;
        }
        self.index = 0;
        self.carry = 362_436;
        self.initialized = true;
    }

    pub(crate) fn next(&mut self) -> f64 {
        if !self.initialized {
            self.initialize();
        } else {
            self.index = self.index.wrapping_add(1) & 0x0fff;
        }
        let index = self.index as usize;
        let product = 18_782_u64 * u64::from(self.values[index]) + u64::from(self.carry);
        let mut carry = (product >> 32) as u32;
        let (mut sum, overflow) = (product as u32).overflowing_add(carry);
        if overflow {
            carry = carry.wrapping_add(1);
            sum = sum.wrapping_add(1);
        }
        self.carry = carry;
        let result = 0xffff_fffe_u32.wrapping_sub(sum);
        self.values[index] = result;
        f64::from(result) * f64::from_bits(0x3df0_0000_0000_0000)
    }
}
