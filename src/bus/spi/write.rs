use embedded_error::mci::{CommandOrDataError, MciError};
use embedded_hal::blocking::spi;
use embedded_hal::digital::v2::OutputPin;

use crate::bus::Write;

use super::bus::SpiBus;
use super::response::{ReadToken, ResponseCode, WriteToken};

impl<SPI, CS, E, OE> SpiBus<SPI, CS>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    CS: OutputPin<Error = OE>,
{
    fn start_write_block(&mut self) -> Result<(), MciError> {
        self.write_byte(0xFF)?;
        let token =
            if self.num_blocks == 1 { WriteToken::SingleWrite } else { WriteToken::MultiWrite };
        self.write_byte(token as u8)
    }

    fn stop_write_block(&mut self) -> Result<(), MciError> {
        // CRC disabled in SPI mode
        self.write_bytes(&[0xFF, 0xFF])?;
        let token = ReadToken::try_from(self.read_byte()?).ok_or(MciError::ReadError)?;
        match token.response_code().ok_or(MciError::WriteError)? {
            ResponseCode::CRCError => Err(MciError::DataError(CommandOrDataError::Crc)),
            ResponseCode::WriteError => Err(MciError::WriteError),
            ResponseCode::Accepted => Ok(()),
        }
    }
    fn stop_write_multi_block(&mut self) -> Result<(), MciError> {
        if self.num_blocks <= 1 {
            return Ok(());
        }
        if self.num_blocks > self.position / self.block_size {
            return Ok(());
        }

        self.write_byte(0xFF)?; // wait 8 cycles
        self.write_byte(WriteToken::StopTransmit as u8)?;
        self.wait_busy()
    }
}

impl<SPI, CS, E, OE> Write for SpiBus<SPI, CS>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    CS: OutputPin<Error = OE>,
{
    fn write_word(&mut self, word: u32) -> Result<(), MciError> {
        if self.position % self.block_size == 0 {
            self.start_write_block()?;
        }
        let mut bytes = u32::to_be_bytes(word);
        self.read_bytes(&mut bytes)?;
        self.position += 4;
        if self.position % self.block_size == 0 {
            self.stop_write_block()?;
            self.wait_busy()?;
        }
        self.stop_write_multi_block()
    }

    fn write_blocks(&mut self, blocks: &[u8]) -> Result<(), MciError> {
        let num_blocks = blocks.len() / self.block_size as usize;
        for i in 0..num_blocks {
            self.start_write_block()?;
            let offset = i * self.block_size;
            self.write_bytes(&blocks[offset..offset + self.block_size])?;
            self.position += self.block_size;
            self.stop_write_block()?;
            // Delay to mci_wait_end_of_write_blocks to check busy
            if i < num_blocks - 1 {
                self.wait_busy()?;
            }
        }
        Ok(())
    }

    fn wait_until_write_finished(&mut self) -> Result<(), MciError> {
        self.wait_busy()?;
        self.stop_write_multi_block()
    }
}
