use embedded_error::mci::MciError;
use embedded_hal::digital::v2::InputPin;

use crate::bus::{Adtc, Bus, Read, Write};
use crate::card::Card;
use crate::registers::ocr::OcrRegister;

pub fn ocr_voltage_support() -> OcrRegister {
    let mut ocr = OcrRegister { val: 0 };
    ocr.set_vdd_27_28(true)
        .set_vdd_28_29(true)
        .set_vdd_29_30(true)
        .set_vdd_30_31(true)
        .set_vdd_31_32(true)
        .set_vdd_32_33(true);
    ocr
}

pub struct Controller<BUS, WP, DETECT> {
    pub card: Card<BUS>,
    pub slot: u8,
    pub write_protect_pin: WP,
    pub detect_pin: DETECT,
    pub lower_is_true: bool,
}

impl<BUS: Adtc + Bus + Read + Write, WP: InputPin, DETECT: InputPin> Controller<BUS, WP, DETECT> {
    /// Create a new SD BUS instance
    pub fn new(
        card: Card<BUS>,
        write_protect_pin: WP,
        detect_pin: DETECT,
        lower_is_true: bool,
        slot: u8,
    ) -> Self {
        Controller { card, slot, write_protect_pin, detect_pin, lower_is_true }
    }

    pub fn write_protected(&self) -> Result<bool, MciError> {
        let level = self.write_protect_pin.is_low().map_err(|_| MciError::PinLevelReadError)?; //TODO proper error for pin fault
        Ok(level == self.lower_is_true)
    }
}
