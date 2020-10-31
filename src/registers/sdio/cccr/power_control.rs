use crate::registers::register_address::RegisterAddress;
use bit_field::BitField;

pub struct PowerControlRegister {
    pub val: u8,
}

impl RegisterAddress for PowerControlRegister {
    fn address() -> u8 {
        0x12
    }
}

impl PowerControlRegister {
    pub fn set_supports_master_power_control(&mut self, supports: bool) {
        self.val.set_bit(0, supports);
    }

    pub fn supports_master_power_control(&self) -> bool {
        self.val.get_bit(0)
    }

    pub fn set_enable_master_power_control(&mut self, enable: bool) {
        self.val.set_bit(1, enable);
    }

    pub fn enable_master_power_control(&self) -> bool {
        self.val.get_bit(1)
    }
}
