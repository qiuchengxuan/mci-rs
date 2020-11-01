use bit_field::BitField;
use embedded_error::mci::MciError;

use crate::bus::SdMmcBus;
use crate::command_arguments::mmc::{Access, BusWidth, Cmd6};
use crate::commands::{
    MMC_CMD6_SWITCH, MMC_CMD8_SEND_EXT_CSD, SDMMC_CMD55_APP_CMD, SDMMC_MCI_CMD9_SEND_CSD,
    SD_ACMD6_SET_BUS_WIDTH,
};
use crate::mode_index::ModeIndex;
use crate::registers::csd::CsdRegister;
use crate::registers::sd::card_status::CardStatusRegister;

use super::card::{Card, MMC_TRANS_MULTIPLIERS, SD_MMC_TRANS_UNITS};
use super::version::{CardVersion, MmcVersion};

pub const EXT_CSD_CARD_TYPE_INDEX: u32 = 196;
pub const EXT_CSD_SEC_COUNT_INDEX: u32 = 212;
pub const EXT_CSD_BSIZE: u32 = 512;

impl<BUS: SdMmcBus> Card<BUS> {
    /// ACMD6 = Define the data bus width to be 4 bits
    pub fn set_data_bus_width_to_4_bits(&mut self) -> Result<(), MciError> {
        self.bus.send_command(SDMMC_CMD55_APP_CMD.into(), (self.rca as u32) << 16)?;
        self.bus.send_command(SD_ACMD6_SET_BUS_WIDTH.into(), 0x2)?;
        self.bus_width = BusWidth::_4BIT;
        Ok(())
    }

    /// CMD9: Card sends its card specific data (CSD)
    /// self.card.csd is updated
    pub fn mci_load_csd(&mut self) -> Result<(), MciError> {
        let arg = (self.rca as u32) << 16;
        self.bus.send_command(SDMMC_MCI_CMD9_SEND_CSD.into(), arg)?;
        self.csd = CsdRegister(self.bus.get_response128()?);
        Ok(())
    }

    /// CMD6 for MMC - Switches the bus width mode
    pub fn set_bus_width(&mut self, bus_width: &BusWidth) -> Result<bool, MciError> {
        let mut arg = Cmd6::default();
        arg.set_access(Access::SetBits)
            .set_bus_width(&bus_width)
            .set_mode_index(ModeIndex::BusWidth);
        self.bus.send_command(MMC_CMD6_SWITCH.into(), arg.val)?;
        let ret = CardStatusRegister { val: self.bus.get_response()? };
        if ret.switch_error() {
            // Not supported, not a protocol error
            return Ok(false);
        }
        self.bus_width = bus_width.clone();
        Ok(true)
    }

    /// CMD6 for MMC - Switches in high speed mode
    /// self.high_speed is updated
    /// self.clock is updated
    pub fn set_high_speed(&mut self) -> Result<bool, MciError> {
        let mut arg = Cmd6::default();
        arg.set_access(Access::WriteByte)
            .set_mode_index(ModeIndex::HsTimingIndex)
            .set_hs_timing_enable(true);
        self.bus.send_command(MMC_CMD6_SWITCH.into(), arg.val)?;
        let ret = CardStatusRegister { val: self.bus.get_response()? };
        if ret.switch_error() {
            // Not supported, not a protocol error
            return Ok(false);
        }
        self.high_speed = true;
        self.clock = 52_000_000u32;
        Ok(true)
    }

    /// CMD8 - The card sends its EXT_CSD as a block of data
    /// Returns whether high speed can be handled by this
    /// self.capacity is updated
    pub fn load_extcsd(&mut self) -> Result<bool, MciError> {
        self.bus.adtc_start(MMC_CMD8_SEND_EXT_CSD.into(), 0, 512, 1, false)?;

        let mut index = 0u32;
        let mut word = 0u32;
        // Read in bytes (4 at a time) and not to a buffer to "fast forward" to the card type
        while index < ((EXT_CSD_CARD_TYPE_INDEX + 4) / 4) {
            word = self.bus.read_word()?;
            index += 1;
        }
        let high_speed_capable =
            (word >> ((EXT_CSD_CARD_TYPE_INDEX % 4) * 8)).get_bits(0..2) == 0x2; // 52MHz = 0x2, 26MHz = 0x1

        if self.csd.card_size() == 0xFFF {
            // For high capacity SD/MMC card, memory capacity = sec_count * 512 bytes
            while index < (EXT_CSD_SEC_COUNT_INDEX + 4) / 4 {
                word = self.bus.read_word()?;
                index += 1;
            }
            self.capacity = word
        }
        // Forward to the end
        while index < EXT_CSD_BSIZE / 4 {
            self.bus.read_word()?;
            index += 1;
        }
        Ok(high_speed_capable)
    }

    /// Decode CSD for MMC
    /// Updates self.version, self.clock, self.capacity
    pub fn decode_csd(&mut self) -> Result<(), MciError> {
        self.version = match self.csd.mmc_csd_spec_version() {
            0 => CardVersion::Mmc(MmcVersion::Mmc1d2),
            1 => CardVersion::Mmc(MmcVersion::Mmc1d4),
            2 => CardVersion::Mmc(MmcVersion::Mmc2d2),
            3 => CardVersion::Mmc(MmcVersion::SdMmc3d0),
            4 => CardVersion::Mmc(MmcVersion::Mmc4d0),
            _ => CardVersion::Unknown,
        };

        // 	Get MMC memory max transfer speed in Hz
        let trans_speed = self.csd.transmission_speed();
        let unit = SD_MMC_TRANS_UNITS[(trans_speed & 0x7) as usize];
        let mult = MMC_TRANS_MULTIPLIERS[((trans_speed >> 3) & 0xF) as usize];
        self.clock = unit * mult * 1000;

        // 	 Get card capacity.
        // 	 ----------------------------------------------------
        // 	 For normal SD/MMC card:
        // 	 memory capacity = BLOCKNR * BLOCK_LEN
        // 	 Where
        // 	 BLOCKNR = (C_SIZE+1) * MULT
        // 	 MULT = 2 ^ (C_SIZE_MULT+2)       (C_SIZE_MULT < 8)
        // 	 BLOCK_LEN = 2 ^ READ_BL_LEN      (READ_BL_LEN < 12)
        // 	 ----------------------------------------------------
        // 	 For high capacity SD/MMC card:
        // 	 memory capacity = SEC_COUNT * 512 byte

        if self.csd.card_size() != 0xFFF {
            let block_nr = ((self.csd.card_size() as u32) + 1)
                * ((self.csd.card_size_multiplier() as u32) + 2);
            self.capacity = block_nr * (1 << self.csd.read_bl_length() as u32) / 1024;
        }
        Ok(())
    }
}
