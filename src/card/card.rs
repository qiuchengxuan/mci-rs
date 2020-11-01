use bit_field::BitField;
use embedded_hal::blocking::spi;

use crate::command_arguments::mmc::BusWidth;
use crate::registers::csd::CsdRegister;

use super::version::CardVersion;

// SD/MMC transfer rate unit codes (10K) list
pub const SD_MMC_TRANS_UNITS: [u32; 7] = [10, 100, 1_000, 10_000, 0, 0, 0];
// SD transfer multiplier factor codes (1/10) list
pub const SD_TRANS_MULTIPLIERS: [u32; 16] =
    [0, 10, 12, 13, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 70, 80];
// MMC transfer multiplier factor codes (1/10) list
pub const MMC_TRANS_MULTIPLIERS: [u32; 16] =
    [0, 10, 12, 13, 15, 20, 26, 30, 35, 40, 45, 52, 55, 60, 70, 80];

#[derive(Default)]
pub struct Type(u8);

impl Type {
    pub fn set_unknown(&mut self) -> &mut Self {
        self.0 = 0x0;
        self
    }

    pub fn set_sd(&mut self, sd: bool) -> &mut Self {
        self.0.set_bit(1, sd);
        self
    }

    pub fn sd(&self) -> bool {
        self.0.get_bit(1)
    }

    pub fn set_mmc(&mut self, mmc: bool) -> &mut Self {
        self.0.set_bit(2, mmc);
        self
    }

    pub fn mmc(&self) -> bool {
        self.0.get_bit(2)
    }

    pub fn set_sdio(&mut self, sdio: bool) -> &mut Self {
        self.0.set_bit(3, sdio);
        self
    }

    pub fn sdio(&self) -> bool {
        self.0.get_bit(3)
    }

    pub fn set_high_capacity(&mut self, hc: bool) -> &mut Self {
        self.0.set_bit(4, hc);
        self
    }

    pub fn high_capacity(&self) -> bool {
        self.0.get_bit(4)
    }
}

#[derive(PartialEq)]
pub enum State {
    Ready,
    Debounce,
    Init,
    Unusable,
    NoCard,
}

pub struct Card<BUS> {
    /// Bus, MMD, SD or SPI
    pub bus: BUS,
    /// Card access clock. Defaults to 400khz
    pub clock: u32,
    /// Card capacity in KBytes
    pub capacity: u32,
    /// Relative card address
    pub rca: u16,
    /// Card state
    pub state: State,
    /// Card type
    pub card_type: Type,
    /// Card version
    pub version: CardVersion,
    /// Number of DATA lines on bus (MCI only)
    pub bus_width: BusWidth,
    /// CSD register
    pub csd: CsdRegister,
    /// High speed card
    pub high_speed: bool,
}

impl<WE, TE, SPI: spi::Write<u8, Error = WE> + spi::Transfer<u8, Error = TE>> Card<SPI> {
    pub fn spi(bus: SPI) -> Self {
        Self {
            bus,
            clock: 400_000,
            capacity: 0,
            rca: 0,
            state: State::NoCard,
            card_type: Type::default(),
            version: CardVersion::Unknown,
            bus_width: BusWidth::_1BIT,
            csd: Default::default(),
            high_speed: false,
        }
    }
}
