use embedded_error::mci::MciError;

use crate::bus::SpiBus;
use crate::commands::SDMMC_SPI_CMD9_SEND_CSD;

use super::card::Card;

impl<BUS: SpiBus> Card<BUS> {
    pub fn spi_load_csd(&mut self) -> Result<(), MciError> {
        self.bus.adtc_start(SDMMC_SPI_CMD9_SEND_CSD.into(), (self.rca as u32) << 16, 4, 1, true)?;
        let bytes: &mut [u8; 16] = unsafe { core::mem::transmute(&mut self.csd.0) };
        self.bus.read_blocks(bytes)?;
        self.bus.wait_until_read_finished()
    }
}
