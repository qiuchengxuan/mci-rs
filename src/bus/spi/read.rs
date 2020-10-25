use embedded_error::mci::{CommandOrDataError, MciError};
use embedded_hal::blocking::spi;
use embedded_hal::digital::v2::OutputPin;

use crate::bus::Read;

use super::bus::SpiBus;
use super::response::{BitField, ErrorToken, ErrorTokenField, BLOCK_READ_DATA_TOKEN};

impl<SPI, CS, E, OE> SpiBus<SPI, CS>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    CS: OutputPin<Error = OE>,
{
    fn start_read_block(&mut self) -> Result<(), MciError> {
        let mut token = self.read_byte()?;
        /* Wait for start data token:
         * The read timeout is the Nac timing.
         * Nac must be computed trough CSD values, * or it is 100ms for SDHC / SDXC
         * Compute the maximum timeout:
         * Frequency maximum = 25MHz
         * 1 byte = 8 cycles
         * 100ms = 312500 x spi_read_buffer_wait() maximum
         */
        let mut counter = 500_000;
        while token != BLOCK_READ_DATA_TOKEN {
            if let Some(token) = ErrorToken::try_from(token) {
                token.no(ErrorTokenField::Error).ok_or(MciError::ReadError)?;
                token
                    .no(ErrorTokenField::CCError)
                    .ok_or(MciError::DataError(CommandOrDataError::Crc))?;
                token.no(ErrorTokenField::CardECCFailed).ok_or(MciError::UnusableCard)?;
            }
            counter -= 1;
            if counter == 0 {
                return Err(MciError::DataError(CommandOrDataError::Timeout));
            }
            token = self.read_byte()?;
        }
        Ok(())
    }

    fn stop_read_block(&mut self) -> Result<(), MciError> {
        let _crc = [self.read_byte()?, self.read_byte()?]; // not checked
        Ok(())
    }
}

impl<SPI, CS, E, OE> Read for SpiBus<SPI, CS>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    CS: OutputPin<Error = OE>,
{
    fn read_word(&mut self) -> Result<u32, MciError> {
        if self.position % self.block_size == 0 {
            self.start_read_block()?;
        }
        let mut bytes = [0xFF; 4];
        self.read_bytes(&mut bytes)?;
        self.position += 4;
        if self.position % self.block_size == 0 {
            self.stop_read_block()?;
        }
        Ok(u32::from_be_bytes(bytes))
    }

    fn read_blocks(&mut self, blocks: &mut [u8]) -> Result<(), MciError> {
        let num_blocks = blocks.len() / self.block_size;
        for i in 0..num_blocks {
            self.start_read_block()?;
            let offset = i * self.block_size;
            self.read_bytes(&mut blocks[offset..offset + self.block_size])?;
            self.position += self.block_size;
            self.stop_read_block()?;
        }
        Ok(())
    }

    fn wait_until_read_finished(&mut self) -> Result<(), MciError> {
        Ok(())
    }
}
