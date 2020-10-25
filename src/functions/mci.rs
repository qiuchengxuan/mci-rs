use embedded_error::mci::MciError;
use embedded_error::mci::SetupError;
use embedded_error::ImplError;
use embedded_hal::digital::v2::InputPin;

use crate::bus::SD_MMC_BLOCK_SIZE;
use crate::card_version::MmcVersion;
use crate::command_arguments::mmc::BusWidth;
use crate::commands::{
    MMC_CMD3_SET_RELATIVE_ADDR, MMC_MCI_CMD1_SEND_OP_COND, SDMMC_CMD16_SET_BLOCKLEN,
    SDMMC_CMD2_ALL_SEND_CID, SDMMC_CMD7_SELECT_CARD_CMD, SDMMC_MCI_CMD0_GO_IDLE_STATE,
};
use crate::mci::Mci;
use crate::mci_card::{ocr_voltage_support, MciCard};
use crate::registers::ocr::{AccessMode, OcrRegister};

pub const EXT_CSD_CARD_TYPE_INDEX: u32 = 196;
pub const EXT_CSD_SEC_COUNT_INDEX: u32 = 212;
pub const EXT_CSD_BSIZE: u32 = 512;

impl<MCI, WP, DETECT> MciCard<MCI, WP, DETECT>
where
    MCI: Mci,
    WP: InputPin,     // Write protect pin
    DETECT: InputPin, // Card detect pin
{
    /// Sends operation condition command and read OCR (MCI only)
    pub fn mmc_mci_send_operation_condition(&mut self) -> Result<(), MciError> {
        let mut ocr = ocr_voltage_support();
        ocr.set_access_mode(AccessMode::Sector);
        // Timeout is 1s = 400KHz / ((6+6)*8) cycles = 4200 retries. TODO maybe a delay check?
        for i in (0..4200).rev() {
            if i == 0 {
                return Err(MciError::Impl(ImplError::TimedOut));
            }
            self.mmc_card.mmc.send_command(MMC_MCI_CMD1_SEND_OP_COND.into(), ocr.val)?;
            let response = self.mmc_card.mmc.get_response()?;
            let response = OcrRegister { val: response };
            if response.card_powered_up_status() {
                if response.access_mode() == AccessMode::Sector {
                    self.mmc_card.card_type.set_high_capacity(true);
                }
                break;
            }
        }
        Ok(())
    }

    /// Initialize the MMC card in MCI mode
    /// This function runs the initialization procedure and the identification process, then it
    /// sets the SD/MMC card in transfer state.
    /// At last, it will enable maximum bus width and transfer speed.
    pub fn sd_mmc_mci_install_mmc(&mut self) -> Result<(), MciError> {
        // CMD0 - Reset all cards to idle state.
        self.mmc_card.mmc.send_command(SDMMC_MCI_CMD0_GO_IDLE_STATE.into(), 0)?;
        self.mmc_mci_send_operation_condition()?;

        // Put the card in Identify Mode
        // Note: The CID is not used
        self.mmc_card.mmc.send_command(SDMMC_CMD2_ALL_SEND_CID.into(), 0)?;

        //Assign relative address to the card
        self.mmc_card.rca = 1;
        self.mmc_card
            .mmc
            .send_command(MMC_CMD3_SET_RELATIVE_ADDR.into(), (self.mmc_card.rca as u32) << 16)?;

        // Get the card specific data
        self.sd_mmc_cmd9_mci()?;
        self.mmc_card.mmc_decode_csd()?;

        // Select the card and put it into Transfer mode
        self.mmc_card
            .mmc
            .send_command(SDMMC_CMD7_SELECT_CARD_CMD.into(), (self.mmc_card.rca as u32) << 16)?;

        let version: usize = self.mmc_card.version.into();
        if version >= MmcVersion::Mmc4d0 as usize {
            // For MMC 4.0 Higher version
            // Get EXT_CSD
            let authorize_high_speed =
                self.mmc_card.mmc_cmd8_high_speed_capable_and_update_capacity()?;
            if BusWidth::_4BIT <= self.mmc_card.mmc.get_bus_width(self.slot)? {
                // Enable more bus width
                let bus_width = self.mmc_card.bus_width;
                self.mmc_card
                    .mmc_cmd6_set_bus_width(&bus_width)
                    .map_err(|_| MciError::Setup(SetupError::CouldNotSetBusWidth))?;
                self.sd_mmc_select_this_device_on_mci_and_configure_mci()
                    .map_err(|_| MciError::Setup(SetupError::CouldNotSetToHighSpeed))?;
            }
            if self
                .mmc_card
                .mmc
                .is_high_speed_capable()
                .map_err(|_| MciError::Setup(SetupError::CouldNotCheckIfIsHighSpeed))?
                && authorize_high_speed
            {
                self.mmc_card
                    .mmc_cmd6_set_high_speed()
                    .map_err(|_| MciError::Setup(SetupError::CouldNotSetToHighSpeed))?;
                self.sd_mmc_select_this_device_on_mci_and_configure_mci()?;
            }
        } else {
            self.sd_mmc_select_this_device_on_mci_and_configure_mci()?;
        }
        for _ in 0..10 {
            // Retry is a workaround for no compliance card (Atmel Internal ref. MMC19)
            // These cards seem not ready immediately after the end of busy of mmc_cmd6_set_high_speed
            if self
                .mmc_card
                .mmc
                .send_command(SDMMC_CMD16_SET_BLOCKLEN.into(), SD_MMC_BLOCK_SIZE as u32)
                .is_ok()
            {
                return Ok(());
            }
        }
        Err(MciError::Impl(ImplError::TimedOut))
    }
}
