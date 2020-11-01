use embedded_error::mci::MciError;

use crate::bus::SpiBus;
use crate::commands::SDMMC_MCI_CMD9_SEND_CSD;
use crate::registers::csd::CsdRegister;

use super::card::Card;

impl<BUS: SpiBus> Card<BUS> {
    /// CMD9: Card sends its card specific data (CSD)
    /// self.mmc_card.csd is updated
    pub fn sd_mmc_cmd9_spi(&mut self) -> Result<(), MciError> {
        let cmd = SDMMC_MCI_CMD9_SEND_CSD.into();
        let size = core::mem::size_of::<CsdRegister>();
        self.mmc.adtc_start(cmd, (self.rca as u32) << 16, size as u16, 1, true)?;
        // self.mmc_card.csd = CsdRegister {
        //     val: self.mmc_card.mmc.get_response128()?,
        // };
        Ok(())
    }
}
