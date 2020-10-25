use embedded_error::mci::MciError;
use embedded_error::mci::MciError::UnusableCard;
use embedded_error::ImplError;
use embedded_hal::digital::v2::InputPin;

use crate::bus::SD_MMC_BLOCK_SIZE;
use crate::card_state::CardState;
use crate::command_arguments::mmc::BusWidth;
use crate::commands::{
    SDMMC_CMD12_STOP_TRANSMISSION, SDMMC_CMD17_READ_SINGLE_BLOCK, SDMMC_CMD18_READ_MULTIPLE_BLOCK,
    SDMMC_CMD24_WRITE_BLOCK, SDMMC_CMD25_WRITE_MULTIPLE_BLOCK, SDMMC_MCI_CMD13_SEND_STATUS,
    SDMMC_MCI_CMD9_SEND_CSD,
};
use crate::mci::Mci;
use crate::mci_card::MciCard;
use crate::registers::csd::CsdRegister;
use crate::registers::sd::card_status::CardStatusRegister;
use crate::transaction::Transaction;

impl<MCI, WP, DETECT> MciCard<MCI, WP, DETECT>
where
    MCI: Mci,
    WP: InputPin,
    DETECT: InputPin,
{
    /// CMD9: Card sends its card specific data (CSD)
    /// self.mmc_card.csd is updated
    pub fn sd_mmc_cmd9_mci(&mut self) -> Result<(), MciError> {
        let arg = (self.mmc_card.rca as u32) << 16;
        self.mmc_card.mmc.send_command(SDMMC_MCI_CMD9_SEND_CSD.into(), arg)?;
        self.mmc_card.csd = CsdRegister { val: self.mmc_card.mmc.get_response128()? };
        Ok(())
    }

    /// CMD13: Get status register.
    /// Waits for the clear of the busy flag
    pub fn sd_mmc_cmd13_get_status_and_wait_for_ready_for_data_flag(
        &mut self,
    ) -> Result<CardStatusRegister, MciError> {
        let mut status = CardStatusRegister::default();
        // TODO maybe proper timeout
        for i in (0..200_000u32).rev() {
            if i == 0 {
                return Err(MciError::Impl(ImplError::TimedOut));
            }
            self.mmc_card.mmc.send_command(
                SDMMC_MCI_CMD13_SEND_STATUS.into(),
                (self.mmc_card.rca as u32) << 16,
            )?;
            status = CardStatusRegister { val: self.mmc_card.mmc.get_response()? };
            if status.ready_for_data() {
                break;
            }
        }
        Ok(status)
    }

    pub fn sd_mmc_deselect_this_device(&mut self) -> Result<(), MciError> {
        self.mmc_card.mmc.deselect_device(self.slot)
    }

    pub fn sd_mmc_select_this_device_on_mci_and_configure_mci(&mut self) -> Result<(), MciError> {
        self.mmc_card
            .mmc
            .select_device(
                self.slot,
                self.mmc_card.clock,
                &self.mmc_card.bus_width,
                self.mmc_card.high_speed,
            )
            .map_err(|_| MciError::CouldNotSelectDevice)
    }

    /// Select this instance's card slot and initialize the associated driver
    pub fn sd_mmc_select_slot(&mut self) -> Result<(), MciError> {
        // Check card detection
        if self.wp.is_high().map_err(|_| MciError::PinLevelReadError)? != self.wp_high_activated {
            // TODO proper error for pin check
            if self.mmc_card.state == CardState::Debounce {
                // TODO Timeout stop?
            }
            self.mmc_card.state = CardState::NoCard;
            return Err(MciError::NoCard);
        }

        if self.mmc_card.state == CardState::Debounce {
            if false {
                // TODO check if timed out
                return Err(MciError::Impl(ImplError::TimedOut));
            }
            self.mmc_card.state = CardState::Init;
            // Set 1-bit bus width and low clock for initialization
            self.mmc_card.clock = 400_000;
            self.mmc_card.bus_width = BusWidth::_1BIT;
            self.mmc_card.high_speed = false;
        }
        if self.mmc_card.state == CardState::Unusable {
            return Err(UnusableCard);
        }
        self.sd_mmc_select_this_device_on_mci_and_configure_mci()?;
        if self.mmc_card.state == CardState::Init {
            Ok(())
        } else {
            Ok(())
        } // TODO if it is still ongoing should return ongoing
    }

    pub fn sd_mmc_init_read_blocks(
        &mut self,
        start: u32,
        num_blocks: u16,
    ) -> Result<Transaction, MciError> {
        self.sd_mmc_select_this_device_on_mci_and_configure_mci()?;
        // Wait for data status
        self.sd_mmc_cmd13_get_status_and_wait_for_ready_for_data_flag()?;
        let cmd: u32 = if num_blocks > 1 {
            SDMMC_CMD18_READ_MULTIPLE_BLOCK.into()
        } else {
            SDMMC_CMD17_READ_SINGLE_BLOCK.into()
        };

        // SDSC Card (CCS=0) uses byte unit address,
        // SDHC and SDXC Cards (CCS=1) use block unit address (512 Bytes unit).
        let mut arg = start;
        if !self.mmc_card.card_type.high_capacity() {
            arg = start * SD_MMC_BLOCK_SIZE as u32;
        }
        self.mmc_card.mmc.adtc_start(cmd, arg, SD_MMC_BLOCK_SIZE as u16, num_blocks, true)?;
        Ok(Transaction::new(num_blocks))
    }

    pub fn sd_mmc_start_read(
        &mut self,
        transaction: &mut Transaction,
        destination: &mut [u8],
    ) -> Result<(), MciError> {
        if self.mmc_card.mmc.read_blocks(destination).is_err() {
            transaction.remain = 0;
            return Err(MciError::ReadError);
        }
        transaction.remain -= (destination.len() / SD_MMC_BLOCK_SIZE) as u16;
        Ok(())
    }

    pub fn sd_mmc_wait_end_of_read_blocks(
        &mut self,
        abort: bool,
        transaction: &mut Transaction,
    ) -> Result<(), MciError> {
        self.mmc_card.mmc.wait_until_read_finished()?;
        if abort {
            transaction.remain = 0;
        } else if transaction.remain > 0 {
            return Ok(());
        }

        // All blocks are transferred then stop read operation
        if transaction.remain == 1 {
            return Ok(());
        }

        // WORKAROUND for no compliance card (Atmel Internal ref. !MMC7 !SD19)
        // The errors on this cmmand must be ignored and one retry can be necessary in SPI mode
        // for non-complying card
        if self.mmc_card.mmc.adtc_stop(SDMMC_CMD12_STOP_TRANSMISSION.into(), 0).is_err() {
            self.mmc_card.mmc.adtc_stop(SDMMC_CMD12_STOP_TRANSMISSION.into(), 0)?;
            // TODO proper error
        }
        Ok(())
    }

    pub fn sd_mmc_init_write_blocks(
        &mut self,
        start: u32,
        num_blocks: u16,
    ) -> Result<Transaction, MciError> {
        self.sd_mmc_select_this_device_on_mci_and_configure_mci()?;
        if self.write_protected()? {
            return Err(MciError::WriteProtected); // TODO proper write protection error
        }

        let cmd: u32 = if num_blocks > 1 {
            SDMMC_CMD25_WRITE_MULTIPLE_BLOCK.into()
        } else {
            SDMMC_CMD24_WRITE_BLOCK.into()
        };

        // SDSC Card (CCS=0) uses byte unit address,
        // SDHC and SDXC Cards (CCS=1) use block unit address (512 Bytes unit).
        let mut arg = start;
        if !self.mmc_card.card_type.high_capacity() {
            arg = start * SD_MMC_BLOCK_SIZE as u32;
        }

        self.mmc_card.mmc.adtc_start(cmd, arg, SD_MMC_BLOCK_SIZE as u16, num_blocks, true)?; // TODO proper error

        let resp = CardStatusRegister { val: self.mmc_card.mmc.get_response()? };
        if resp.write_protect_violation() {
            return Err(MciError::WriteProtected);
        }

        Ok(Transaction { total: num_blocks, remain: num_blocks })
    }

    pub fn sd_mmc_start_write_blocks(
        &mut self,
        transaction: &mut Transaction,
        blocks: &[u8],
    ) -> Result<(), MciError> {
        if self.mmc_card.mmc.write_blocks(blocks).is_err() {
            transaction.remain = 0;
            return Err(MciError::WriteError); // TODO proper error
        }
        transaction.remain -= (blocks.len() / SD_MMC_BLOCK_SIZE) as u16;
        Ok(())
    }

    pub fn sd_mmc_wait_end_of_write_blocks(
        &mut self,
        abort: bool,
        transaction: &mut Transaction,
    ) -> Result<(), MciError> {
        self.mmc_card.mmc.wait_until_write_finished()?;
        if abort {
            transaction.remain = 0;
        } else if transaction.remain > 0 {
            return Ok(()); // TODO proper return?
        }

        // All blocks are transferred then stop write operation
        if transaction.remain == 1 {
            // Single block transfer, then nothing to do
            return Ok(()); // TODO proper return?
        }

        // Note SPI multi-block writes terminate using a special token, not a STOP_TRANSMISSION request
        self.mmc_card.mmc.adtc_stop(SDMMC_CMD12_STOP_TRANSMISSION.into(), 0)?;
        Ok(())
    }
}
