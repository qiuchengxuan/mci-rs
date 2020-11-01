mod controller;
mod sdcard;
mod sdmmc;

use embedded_error::mci::MciError;
use embedded_error::mci::MciError::UnusableCard;
use embedded_error::ImplError;
use embedded_hal::digital::v2::InputPin;

use crate::bus::{Adtc, Bus, Read, Write, SD_MMC_BLOCK_SIZE};
use crate::card::State;
use crate::command_arguments::mmc::BusWidth;
use crate::commands::{
    SDMMC_CMD12_STOP_TRANSMISSION, SDMMC_CMD17_READ_SINGLE_BLOCK, SDMMC_CMD18_READ_MULTIPLE_BLOCK,
    SDMMC_CMD24_WRITE_BLOCK, SDMMC_CMD25_WRITE_MULTIPLE_BLOCK, SDMMC_MCI_CMD13_SEND_STATUS,
};
use crate::registers::sd::card_status::CardStatusRegister;
use crate::transaction::Transaction;

pub use controller::Controller;

impl<BUS: Adtc + Bus + Read + Write, WP: InputPin, DETECT: InputPin> Controller<BUS, WP, DETECT> {
    /// CMD13: Get status register.
    /// Waits for the clear of the busy flag
    pub fn load_status(&mut self) -> Result<CardStatusRegister, MciError> {
        let mut status = CardStatusRegister::default();
        // TODO maybe proper timeout
        for i in (0..200_000u32).rev() {
            if i == 0 {
                return Err(MciError::Impl(ImplError::TimedOut));
            }
            self.card
                .bus
                .send_command(SDMMC_MCI_CMD13_SEND_STATUS.into(), (self.card.rca as u32) << 16)?;
            status = CardStatusRegister { val: self.card.bus.get_response()? };
            if status.ready_for_data() {
                break;
            }
        }
        Ok(status)
    }

    pub fn deselect(&mut self) -> Result<(), MciError> {
        self.card.bus.deselect_device(self.slot)
    }

    pub fn select(&mut self) -> Result<(), MciError> {
        self.card
            .bus
            .select_device(self.slot, self.card.clock, &self.card.bus_width, self.card.high_speed)
            .map_err(|_| MciError::CouldNotSelectDevice)
    }

    /// Select this instance's card slot and initialize the associated driver
    pub fn select_slot(&mut self) -> Result<(), MciError> {
        // Check card detection
        if !self.write_protected()? {
            // TODO proper error for pin check
            if self.card.state == State::Debounce {
                // TODO Timeout stop?
            }
            self.card.state = State::NoCard;
            return Err(MciError::NoCard);
        }

        if self.card.state == State::Debounce {
            if false {
                // TODO check if timed out
                return Err(MciError::Impl(ImplError::TimedOut));
            }
            self.card.state = State::Init;
            // Set 1-bit bus width and low clock for initialization
            self.card.clock = 400_000;
            self.card.bus_width = BusWidth::_1BIT;
            self.card.high_speed = false;
        }
        if self.card.state == State::Unusable {
            return Err(UnusableCard);
        }
        self.select()?;
        if self.card.state == State::Init {
            Ok(())
        } else {
            Ok(())
        } // TODO if it is still ongoing should return ongoing
    }

    pub fn init_read_blocks(
        &mut self,
        start: u32,
        num_blocks: u16,
    ) -> Result<Transaction, MciError> {
        self.select()?;
        // Wait for data status
        self.load_status()?;
        let cmd: u32 = if num_blocks > 1 {
            SDMMC_CMD18_READ_MULTIPLE_BLOCK.into()
        } else {
            SDMMC_CMD17_READ_SINGLE_BLOCK.into()
        };

        // SDSC Card (CCS=0) uses byte unit address,
        // SDHC and SDXC Cards (CCS=1) use block unit address (512 Bytes unit).
        let mut arg = start;
        if !self.card.card_type.high_capacity() {
            arg = start * SD_MMC_BLOCK_SIZE as u32;
        }
        self.card.bus.adtc_start(cmd, arg, SD_MMC_BLOCK_SIZE as u16, num_blocks, true)?;
        Ok(Transaction::new(num_blocks))
    }

    pub fn start_read(
        &mut self,
        transaction: &mut Transaction,
        destination: &mut [u8],
    ) -> Result<(), MciError> {
        if self.card.bus.read_blocks(destination).is_err() {
            transaction.remain = 0;
            return Err(MciError::ReadError);
        }
        transaction.remain -= (destination.len() / SD_MMC_BLOCK_SIZE) as u16;
        Ok(())
    }

    pub fn wait_end_of_read_blocks(
        &mut self,
        abort: bool,
        transaction: &mut Transaction,
    ) -> Result<(), MciError> {
        self.card.bus.wait_until_read_finished()?;
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
        if self.card.bus.adtc_stop(SDMMC_CMD12_STOP_TRANSMISSION.into(), 0).is_err() {
            self.card.bus.adtc_stop(SDMMC_CMD12_STOP_TRANSMISSION.into(), 0)?;
            // TODO proper error
        }
        Ok(())
    }

    pub fn init_write_blocks(
        &mut self,
        start: u32,
        num_blocks: u16,
    ) -> Result<Transaction, MciError> {
        self.select()?;
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
        if !self.card.card_type.high_capacity() {
            arg = start * SD_MMC_BLOCK_SIZE as u32;
        }

        self.card.bus.adtc_start(cmd, arg, SD_MMC_BLOCK_SIZE as u16, num_blocks, true)?; // TODO proper error

        let resp = CardStatusRegister { val: self.card.bus.get_response()? };
        if resp.write_protect_violation() {
            return Err(MciError::WriteProtected);
        }

        Ok(Transaction { total: num_blocks, remain: num_blocks })
    }

    pub fn start_write_blocks(
        &mut self,
        transaction: &mut Transaction,
        blocks: &[u8],
    ) -> Result<(), MciError> {
        if self.card.bus.write_blocks(blocks).is_err() {
            transaction.remain = 0;
            return Err(MciError::WriteError); // TODO proper error
        }
        transaction.remain -= (blocks.len() / SD_MMC_BLOCK_SIZE) as u16;
        Ok(())
    }

    pub fn wait_end_of_write_blocks(
        &mut self,
        abort: bool,
        transaction: &mut Transaction,
    ) -> Result<(), MciError> {
        self.card.bus.wait_until_write_finished()?;
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
        self.card.bus.adtc_stop(SDMMC_CMD12_STOP_TRANSMISSION.into(), 0)?;
        Ok(())
    }
}
