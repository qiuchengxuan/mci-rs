use crate::sd_mmc::sd_mmc::{SdMmcCard, ocr_voltage_support, SD_MMC_TRANS_UNITS, MMC_TRANS_MULTIPLIERS};
use crate::sd_mmc::mci::Mci;
use atsamd_hal::hal::digital::v2::InputPin;
use crate::sd_mmc::commands::{MMC_MCI_CMD1_SEND_OP_COND, MMC_CMD6_SWITCH, MMC_CMD8_SEND_EXT_CSD};
use crate::sd_mmc::registers::ocr::{OcrRegister, AccessMode};
use crate::sd_mmc::command::mmc_commands::{BusWidth, Cmd6, Access};
use crate::sd_mmc::mode_index::ModeIndex;
use crate::sd_mmc::registers::sd::card_status::CardStatusRegister;
use crate::sd_mmc::registers::csd::CsdRegister;
use bit_field::BitField;
use crate::sd_mmc::card_version::CardVersion::{Mmc, Unknown};
use crate::sd_mmc::card_version::MmcVersion;

pub const EXT_CSD_CARD_TYPE_INDEX: u32 = 196;
pub const EXT_CSD_SEC_COUNT_INDEX: u32 = 212;
pub const EXT_CSD_BSIZE: u32 = 512;

impl <MCI, WP, DETECT> SdMmcCard<MCI, WP, DETECT>
    where MCI: Mci,
          WP: InputPin,       // Write protect pin
          DETECT: InputPin    // Card detect pin
{
    /// Sends operation condition command and read OCR (MCI only)
    pub fn mmc_mci_send_operation_condition(&mut self) -> Result<(), ()> {
        let mut ocr = ocr_voltage_support();
        ocr.set_access_mode(AccessMode::Sector);
        // Timeout is 1s = 400KHz / ((6+6)*8) cycles = 4200 retries. TODO maybe a delay check?
        for i in (0..4200).rev() {
            if i == 0 {
                return Err(()) // TODO proper error
            }
            self.mci.send_command(MMC_MCI_CMD1_SEND_OP_COND.into(), ocr.val)?;
            let response = self.mci.get_response();
            let response = OcrRegister { val: response };
            if response.card_powered_up_status() {
                if response.access_mode() == AccessMode::Sector {
                    self.card_type.set_high_capacity(true);
                }
                break;
            }
        }
        Ok(())
    }

    /// CMD6 for MMC - Switches the bus width mode
    pub fn mmc_cmd6_set_bus_width(&mut self, bus_width: BusWidth) -> Result<bool, ()> {
        let mut arg = Cmd6::default();
        arg.set_access(Access::SetBits)
            .set_bus_width(&bus_width)
            .set_mode_index(ModeIndex::BusWidth);
        self.mci.send_command(MMC_CMD6_SWITCH.into(), arg.val)?;
        let ret = CardStatusRegister { val: self.mci.get_response() };
        if ret.switch_error() {
            // Not supported, not a protocol error
            return Ok(false)
        }
        self.bus_width = bus_width;
        Ok(true)
    }

    /// CMD6 for MMC - Switches in high speed mode
    /// self.high_speed is updated
    /// self.clock is updated
    pub fn mmc_cmd6_set_high_speed(&mut self) -> Result<bool, ()> {
        let mut arg = Cmd6::default();
        arg.set_access(Access::WriteByte)
            .set_mode_index(ModeIndex::HsTimingIndex)
            .set_hs_timing_enable(true);
        self.mci.send_command(MMC_CMD6_SWITCH.into(), arg.val)?;
        let ret = CardStatusRegister { val: self.mci.get_response() };
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
    pub fn mmc_cmd8_high_speed_capable_and_update_capacity(&mut self) -> Result<bool, ()> {
        self.mci.adtc_start(MMC_CMD8_SEND_EXT_CSD.into(), 0, 512, 1, false)?;

        let mut index = 0u32;
        let mut read = (0u32, 0u8);
        // Read in bytes (4 at a time) and not to a buffer to "fast forward" to the card type
        while index < ((EXT_CSD_CARD_TYPE_INDEX + 4) / 4) {
            read = self.mci.read_word()?;
            index += 1;
        }
        let high_speed_capable = (read.0 >> (EXT_CSD_CARD_TYPE_INDEX % 4) * 8).get_bits(0..2) == 0x2;   // 52MHz = 0x2, 26MHz = 0x1

        if self.csd.card_size() == 0xFFF {
            // For high capacity SD/MMC card, memory capacity = sec_count * 512 bytes
            while index < (EXT_CSD_SEC_COUNT_INDEX + 4) / 4 {
                read = self.mci.read_word()?;
                index += 1;
            }
            self.capacity = read.0
        }
        // Forward to the end
        while index < EXT_CSD_BSIZE / 4 {
            self.mci.read_word()?;
            index += 1;
        }
        Ok(high_speed_capable)
    }

    /// Decode CSD for MMC
    pub fn mmc_decode_csd(&mut self) -> Result<(), ()> {
        self.version = match self.csd.mmc_csd_spec_version() {
            0 => Mmc(MmcVersion::Mmc_1_2),
            1 => Mmc(MmcVersion::Mmc_1_4),
            2 => Mmc(MmcVersion::Mmc_2_2),
            3 => Mmc(MmcVersion::SdMmc_3_0),
            4 => Mmc(MmcVersion::Mmc_4),
            _ => Unknown
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
            let block_nr = ((self.csd.card_size() as u32) + 1) * ((self.csd.card_size_multiplier() as u32) + 2);
            self.capacity = block_nr * (1 << self.csd.read_bl_length() as u32) / 1024;
        }
        Ok(())
    }
}