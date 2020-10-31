use crate::registers::register_address::RegisterAddress;
use bit_field::BitField;

/// FN0 block size, LSB first
pub struct BlockSize {
    pub val: u16,
}

impl BlockSize {
    pub fn set_lsb(&mut self, val: u8) {
        self.val.set_bits(8..15, val as u16);
    }

    pub fn lsb(&self) -> u8 {
        self.val.get_bits(8..15) as u8
    }

    pub fn set_msb(&mut self, val: u8) {
        self.val.set_bits(0..8, val as u16);
    }

    pub fn msb(&mut self) -> u8 {
        self.val.get_bits(0..8) as u8
    }
}

impl RegisterAddress for BlockSize {
    fn address() -> u8 {
        0x10u8
    }
}
