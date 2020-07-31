use bit_field::BitField;
use crate::sd_mmc::registers::registers::Register;

/// Address pointer to CSA, 3 bytes, LSB first
pub struct CsaPointerRegister {
    pub val: u32
}

impl CsaPointerRegister {
    pub fn set_lsb(&mut self, val: u8) {
        self.val.set_bits(16..23, val as u32);
    }

    pub fn lsb(&self) -> u8 {
        self.val.get_bits(16..23) as u8
    }

    pub fn set_mid(&mut self, val: u8) {
        self.val.set_bits(8..15, val as u32);
    }

    pub fn mid(&self) -> u8 {
        self.val.get_bits(16..23) as u8
    }

    pub fn set_msb(&mut self, val: u8) {
        self.val.set_bits(0..7, val as u32);
    }

    pub fn msb(&self) -> u8 {
        self.val.get_bits(0..7) as u8
    }
}

impl Register<u32> for CsaPointerRegister {
    fn value(&self) -> u32 {
        self.val
    }

    fn address() -> u8 {
        0x9
    }
}