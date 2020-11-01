use embedded_error::mci::MciError;
use embedded_error::mci::SetupError;
use embedded_error::ImplError;
use embedded_hal::digital::v2::InputPin;

use crate::bus::{SdMmcBus, SD_MMC_BLOCK_SIZE};
use crate::card::version::MmcVersion;
use crate::command_arguments::mmc::BusWidth;
use crate::commands::{
    MMC_CMD3_SET_RELATIVE_ADDR, SDMMC_CMD16_SET_BLOCKLEN, SDMMC_CMD2_ALL_SEND_CID,
    SDMMC_CMD7_SELECT_CARD_CMD, SDMMC_MCI_CMD0_GO_IDLE_STATE,
};

use super::controller::Controller;

impl<BUS: SdMmcBus, WP: InputPin, DETECT: InputPin> Controller<BUS, WP, DETECT> {
    /// Initialize the MMC card in MCI mode
    /// This function runs the initialization procedure and the identification process, then it
    /// sets the SD/MMC card in transfer state.
    /// At last, it will enable maximum bus width and transfer speed.
    pub fn init(&mut self) -> Result<(), MciError> {
        // CMD0 - Reset all cards to idle state.
        self.card.bus.send_command(SDMMC_MCI_CMD0_GO_IDLE_STATE.into(), 0)?;
        self.load_ocr_mmc()?;

        // Put the card in Identify Mode
        // Note: The CID is not used
        self.card.bus.send_command(SDMMC_CMD2_ALL_SEND_CID.into(), 0)?;

        //Assign relative address to the card
        self.card.rca = 1;
        self.card
            .bus
            .send_command(MMC_CMD3_SET_RELATIVE_ADDR.into(), (self.card.rca as u32) << 16)?;

        // Get the card specific data
        self.card.mci_load_csd()?;
        self.card.decode_csd()?;

        // Select the card and put it into Transfer mode
        self.card
            .bus
            .send_command(SDMMC_CMD7_SELECT_CARD_CMD.into(), (self.card.rca as u32) << 16)?;

        let version: usize = self.card.version.into();
        if version >= MmcVersion::Mmc4d0 as usize {
            // For MMC 4.0 Higher version
            // Get EXT_CSD
            let authorize_high_speed = self.card.load_extcsd()?;
            if BusWidth::_4BIT <= self.card.bus.get_bus_width(self.slot)? {
                // Enable more bus width
                let bus_width = self.card.bus_width;
                self.card
                    .set_bus_width(&bus_width)
                    .map_err(|_| MciError::Setup(SetupError::CouldNotSetBusWidth))?;
                self.select().map_err(|_| MciError::Setup(SetupError::CouldNotSetToHighSpeed))?;
            }
            if self
                .card
                .bus
                .is_high_speed_capable()
                .map_err(|_| MciError::Setup(SetupError::CouldNotCheckIfIsHighSpeed))?
                && authorize_high_speed
            {
                self.card
                    .set_high_speed()
                    .map_err(|_| MciError::Setup(SetupError::CouldNotSetToHighSpeed))?;
                self.select()?;
            }
        } else {
            self.select()?;
        }
        for _ in 0..10 {
            // Retry is a workaround for no compliance card (Atmel Internal ref. MMC19)
            // These cards seem not ready immediately after the end of busy of mmc_cmd6_set_high_speed
            if self
                .card
                .bus
                .send_command(SDMMC_CMD16_SET_BLOCKLEN.into(), SD_MMC_BLOCK_SIZE as u32)
                .is_ok()
            {
                return Ok(());
            }
        }
        Err(MciError::Impl(ImplError::TimedOut))
    }
}
