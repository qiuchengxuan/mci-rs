use embedded_error::mci::{CommandOrDataError, MciError};
use embedded_hal::blocking::spi;
use embedded_hal::digital::v2::OutputPin;

use crate::bus::Adtc;
use crate::command_arguments::mci_command::MciCommand;

use super::bus::SpiBus;
use super::response::{BitField, R1Response, R1ResponseField};

pub fn crc7(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &b in data.iter() {
        for i in 0..8 {
            crc <<= 1;
            if (((b << i) & 0x80) ^ (crc & 0x80)) != 0 {
                crc ^= 0x09;
            }
        }
    }
    (crc << 1) | 1
}

impl<SPI, CS, E, OE> Adtc for SpiBus<SPI, CS>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    CS: OutputPin<Error = OE>,
{
    fn adtc_start(
        &mut self,
        command: u32,
        argument: u32,
        block_size: u16,
        num_blocks: u16,
        _access_in_blocks: bool,
    ) -> Result<(), MciError> {
        // 8 cycles to respect Ncs timing
        // NOTE: This byte does not include start bit "0", thus it is ignored by card.
        self.write_byte(0xFF)?;

        let mci_command: MciCommand = command.into();
        let mut buf = [0u8; 9];
        buf[3] = 0x40 | mci_command.get_index();
        let array: &mut [u32; 2] = unsafe { core::mem::transmute(&mut buf) };
        array[1] = u32::to_be(argument);
        buf[8] = crc7(&buf[3..=8]);
        self.write_bytes(&buf[3..])?;

        self.read_byte()?; // Ignore first byte because Ncr min. = 8 clock cylces
        let mut r1: R1Response = self.read_byte()?.into();
        let mut ncr_timeout = 7;
        while r1.has(R1ResponseField::Error) && ncr_timeout > 0 {
            r1 = self.read_byte()?.into();
            ncr_timeout -= 1;
        }
        if ncr_timeout == 0 {
            return Err(MciError::CommandError(CommandOrDataError::Timeout));
        }
        self.last_response = r1.0 as u32;

        r1.no(R1ResponseField::CommandCRC)
            .ok_or(MciError::CommandError(CommandOrDataError::Crc))?;
        r1.no(R1ResponseField::IllegalCommand)
            .ok_or(MciError::CommandError(CommandOrDataError::Index))?;
        r1.no(R1ResponseField::Idle).ok_or(MciError::WriteError)?;
        if mci_command.card_may_send_busy() {
            self.wait_busy()?;
        }
        if mci_command.have_8bit_response() {
            self.last_response = u32::from_le(self.read_byte()? as u32);
        }
        if mci_command.have_32bit_response() {
            let array: &mut [u8; 4] = unsafe { core::mem::transmute(&mut self.last_response) };
            self.read_bytes(array)?;
            self.last_response = u32::from_be(self.last_response);
        }
        self.block_size = block_size as usize;
        self.num_blocks = num_blocks as usize;
        self.position = 0;
        Ok(())
    }

    fn adtc_stop(&self, _command: u32, _argument: u32) -> Result<(), MciError> {
        // Nop
        Ok(())
    }
}
