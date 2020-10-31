use crate::registers::register_address::RegisterAddress;
use crate::sd::sd_physical_specification::SdPhysicalSpecification;
use bit_field::BitField;

pub struct SdPhysicalSpecificationRegister {
    pub val: u8,
}

impl RegisterAddress for SdPhysicalSpecificationRegister {
    fn address() -> u8 {
        0x01u8
    }
}

impl SdPhysicalSpecificationRegister {
    pub fn set_specification(&mut self, val: SdPhysicalSpecification) {
        self.val.set_bits(0..8, val as u8);
    }

    pub fn specification(&self) -> SdPhysicalSpecification {
        self.val.get_bits(0..8).into()
    }
}
