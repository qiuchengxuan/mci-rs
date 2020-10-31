use bit_field::BitField;
use embedded_error::mci::MciError;
use embedded_error::ImplError;
use embedded_hal::digital::v2::InputPin;

use crate::card_version::CardVersion::SdCard;
use crate::card_version::SdCardVersion;
use crate::command_arguments::mmc::BusWidth;
use crate::command_arguments::sd::cmd6::{Cmd6, Cmd6Mode};
use crate::command_arguments::sd::cmd8::Cmd8;
use crate::command_flags::CommandFlag;
use crate::command_responses::Response;
use crate::commands::{
    Command, SDMMC_CMD55_APP_CMD, SD_ACMD51_SEND_SCR, SD_ACMD6_SET_BUS_WIDTH, SD_CMD6_SWITCH_FUNC,
    SD_CMD8_SEND_IF_COND, SD_MCI_ACMD41_SD_SEND_OP_COND,
};
use crate::mci::Mci;
use crate::mci_card::{ocr_voltage_support, MciCard};
use crate::mmc_card::{SD_MMC_TRANS_UNITS, SD_TRANS_MULTIPLIERS};
use crate::registers::csd::SdCsdStructureVersion;
use crate::registers::ocr::OcrRegister;
use crate::registers::sd::scr::ScrRegister;
use crate::registers::sd::switch_status::{SwitchStatusRegister, SD_SW_STATUS_FUN_GRP_RC_ERROR};
use crate::sd::sd_physical_specification::SdPhysicalSpecification;

impl<MCI, WP, DETECT> MciCard<MCI, WP, DETECT>
where
    MCI: Mci,
    WP: InputPin,
    DETECT: InputPin,
{
    /// Ask all cards to send their operations conditions (MCI only).
    /// # Arguments
    /// * `v2` Shall be true if it is a SD card V2
    pub fn sd_mci_operations_conditions(&mut self, v2: bool) -> Result<(), MciError> {
        // Timeout 1s = 400KHz / ((6+6+6+6)*8) cycles = 2100 retry
        for i in (0..2100).rev() {
            if i == 0 {
                return Err(MciError::Impl(ImplError::TimedOut));
            }
            // CMD55 - Indicate to the card that the next command is an
            // application specific command rather than a standard command.

            self.mmc_card.mmc.send_command(SDMMC_CMD55_APP_CMD.into(), 0)?;
            let mut arg = ocr_voltage_support();
            arg.val.set_bit(30, v2); // SD_ACMD41_HCS ACMD41 High Capacity Support
            self.mmc_card.mmc.send_command(SD_MCI_ACMD41_SD_SEND_OP_COND.into(), arg.val)?;
            let resp = self.mmc_card.mmc.get_response()?;
            let resp = OcrRegister { val: resp };
            if resp.card_powered_up_status() {
                if resp.card_capacity_status() {
                    self.mmc_card.card_type.set_high_capacity(true);
                }
                break;
            }
        }
        Ok(())
    }

    pub fn sd_cmd6<RESPONSE: Response, FLAG: CommandFlag>(
        &mut self,
        command: Command<RESPONSE, FLAG>,
        arg: Cmd6,
    ) -> Result<SwitchStatusRegister, MciError> {
        let mut buf = [0u8; 64];
        self.mmc_card.mmc.adtc_start(command.into(), arg.val, 64, 1, true)?;
        self.mmc_card.mmc.read_blocks(&mut buf)?;
        self.mmc_card.mmc.wait_until_read_finished()?;

        let ret: SwitchStatusRegister = buf.into();
        Ok(ret)
    }

    /// CMD6 for SD - Switch card in high speed mode
    /// CMD6 is valid under the trans state
    /// self.high_speed is updated
    /// self.mmc_card.clock is updated
    ///
    /// True if set to high speed
    pub fn sd_cmd6_set_to_high_speed_mode(&mut self) -> Result<bool, MciError> {
        let mut arg = Cmd6 { val: 0 };
        arg.set_function_group_1_access_mode(true)
            .set_function_group2_command_system(false)
            .set_function_group3(true)
            .set_function_group4(true)
            .set_function_group5(true)
            .set_function_group6(true)
            .set_mode(Cmd6Mode::Switch);
        let status = self.sd_cmd6(SD_CMD6_SWITCH_FUNC, arg)?;

        if status.group1_info_status() == SD_SW_STATUS_FUN_GRP_RC_ERROR {
            // Not supported, not a protocol error
            return Ok(false);
        }

        if status.group1_busy() > 0 {
            return Err(MciError::GroupBusy);
        }

        // CMD6 function switching period is within 8 clocks after then bit of status data
        self.mmc_card.mmc.send_clock()?;

        self.mmc_card.high_speed = true;
        self.mmc_card.clock *= 2;

        Ok(false)
    }

    /// CMD8 for SD card - send interface condition command
    /// Send SD Memory Card interface condition, which includes host supply
    /// voltage information and asks the card whether card supports voltage.
    /// Should be performed at initialization time to detect the card type.
    ///
    pub fn sd_cmd8_is_v2(&mut self) -> Result<bool, MciError> {
        let mut arg = Cmd8::default();
        arg.set_cmd8_pattern(true).set_high_voltage(true);

        if self.mmc_card.mmc.send_command(SD_CMD8_SEND_IF_COND.into(), arg.val as u32).is_err() {
            return Ok(false); // Not V2
        }
        let ret = self.mmc_card.mmc.get_response()?;
        if ret == 0xFFFF_FFFF {
            // No compliance R7 value
            return Ok(false);
        }
        if ret != arg.val as u32 {
            return Err(MciError::Impl(ImplError::InvalidConfiguration));
        }
        // Is a V2
        Ok(true)
    }

    /// Decodes the SD CSD register
    /// updates self.mmc_card.clock, self.mmc_card.capacity
    pub fn sd_decode_csd(&mut self) -> Result<(), MciError> {
        // 	Get SD memory maximum transfer speed in Hz.
        let trans_speed = self.mmc_card.csd.transmission_speed();
        let unit = SD_MMC_TRANS_UNITS[(trans_speed & 0x7) as usize];
        let mult = SD_TRANS_MULTIPLIERS[((trans_speed >> 3) & 0xF) as usize];
        self.mmc_card.clock = unit * mult * 1000;

        if self.mmc_card.csd.sd_csd_structure_version() as u8
            >= (SdCsdStructureVersion::Ver2d0 as u8)
        {
            self.mmc_card.capacity = (self.mmc_card.csd.sd_2_0_card_size() + 1) * 512;
        } else {
            let block_nr = ((self.mmc_card.csd.card_size() as u32) + 1)
                * ((self.mmc_card.csd.card_size_multiplier() as u32) + 2);
            self.mmc_card.capacity =
                block_nr * (1 << self.mmc_card.csd.read_bl_length() as u32) / 1024;
        }
        Ok(())
    }

    /// ACMD6 = Define the data bus width to be 4 bits
    pub fn sd_acmd6_set_data_bus_width_to_4_bits(&mut self) -> Result<(), MciError> {
        self.mmc_card
            .mmc
            .send_command(SDMMC_CMD55_APP_CMD.into(), (self.mmc_card.rca as u32) << 16)?;
        self.mmc_card.mmc.send_command(SD_ACMD6_SET_BUS_WIDTH.into(), 0x2)?;
        self.mmc_card.bus_width = BusWidth::_4BIT;
        Ok(())
    }

    /// Get the SD Card configuration register (ACMD51)
    pub fn sd_scr(&mut self) -> Result<ScrRegister, MciError> {
        let mut buf = [0u8; 8];
        self.mmc_card
            .mmc
            .send_command(SDMMC_CMD55_APP_CMD.into(), (self.mmc_card.rca as u32) << 16)?;
        self.mmc_card.mmc.adtc_start(SD_ACMD51_SEND_SCR.into(), 0, 8, 1, true)?;
        self.mmc_card.mmc.read_blocks(&mut buf)?;
        self.mmc_card.mmc.wait_until_read_finished()?;

        Ok(buf.into())
    }

    /// ACMD51 - Read the SD Card configuration register (SCR)
    /// SCR provides information on the SD Memory Card's special features that were configured
    /// into the given card. The SCR register is 64 bits.
    /// Updates self.version
    pub fn sd_acmd51(&mut self) -> Result<(), MciError> {
        let scr = self.sd_scr()?;
        self.mmc_card.version = match scr.sd_specification_version() {
            SdPhysicalSpecification::Revision1d01 => SdCard(SdCardVersion::Sd1d0),
            SdPhysicalSpecification::Revision1d10 => SdCard(SdCardVersion::Sd1d10),
            SdPhysicalSpecification::Revision2d00 => SdCard(SdCardVersion::Sd2d0),
            _ => SdCard(SdCardVersion::Sd1d0),
        };
        Ok(())
    }
}
