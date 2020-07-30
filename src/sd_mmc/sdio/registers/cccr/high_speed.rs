use bit_field::BitField;

pub struct HighSpeedRegister {
    pub val: u8
}

impl HighSpeedRegister {
    pub fn set_supports_high_speed(&mut self, supports: bool) {
        self.val.set_bit(0, supports);
    }

    pub fn supports_high_speed(&self) -> bool {
        self.val.get_bit(0)
    }

    pub fn set_enable_high_speed(&mut self, enable: bool) {
        self.val.set_bit(1, enable);
    }

    pub fn enable_high_speed(&self) -> bool {
        self.val.get_bit(1)
    }
}