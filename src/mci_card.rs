use embedded_error::mci::MciError;
use embedded_hal::digital::v2::InputPin;

use crate::card_state::CardState;
use crate::card_type::CardType;
use crate::card_version::CardVersion;
use crate::command_arguments::mmc::BusWidth;
use crate::mci::Mci;
use crate::mmc_card::MmcCard;
use crate::registers::ocr::OcrRegister;

pub struct MciCard<MCI: Mci, WP: InputPin, DETECT: InputPin> {
    /// SDMCI card definition
    pub mmc_card: MmcCard<MCI>,
    /// This card's slot number
    pub slot: u8,
    /// Write protect pin
    pub wp: WP,
    /// Whether a pulled high pin is logic true that write protection is activated
    pub wp_high_activated: bool,
    /// Card detection pin
    pub detect: DETECT,
    /// Whether a pulled high pin is logic true that a card is detected
    pub detect_high_activated: bool,
}

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

impl<MCI: Mci, WP: InputPin, DETECT: InputPin> MciCard<MCI, WP, DETECT> {
    /// Create a new SD MCI instance
    pub fn new(
        mci: MCI,
        write_protect_pin: WP,
        wp_high_activated: bool,
        detect_pin: DETECT,
        detect_high_activated: bool,
        slot: u8,
    ) -> Self {
        let mmc_card = MmcCard {
            mmc: mci,
            clock: 400_000,
            capacity: 0,
            rca: 0,
            state: CardState::NoCard,
            card_type: CardType { val: 0 },
            version: CardVersion::Unknown,
            bus_width: BusWidth::_1BIT,
            csd: Default::default(),
            high_speed: false,
        };
        MciCard {
            mmc_card,
            slot,
            wp: write_protect_pin,
            wp_high_activated,
            detect: detect_pin,
            detect_high_activated,
        }
    }

    pub fn write_protected(&self) -> Result<bool, MciError> {
        let level = self.wp.is_high().map_err(|_| MciError::PinLevelReadError)?; //TODO proper error for pin fault
        Ok(level == self.wp_high_activated)
    }
}
