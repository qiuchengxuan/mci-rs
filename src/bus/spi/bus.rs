use embedded_error::mci::{CommandOrDataError, MciError};
use embedded_hal::blocking::spi;
use embedded_hal::digital::v2::OutputPin;

use crate::bus::{Adtc, Bus};
use crate::command_arguments::mmc::BusWidth;

pub struct SpiBus<SPI, CS> {
    pub(crate) spi: SPI,
    cs: CS,
    pub(crate) last_response: u32,
    pub(crate) block_size: usize,
    pub(crate) num_blocks: usize,
    pub(crate) position: usize,
}

impl<SPI, CS, E, OE> SpiBus<SPI, CS>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    CS: OutputPin<Error = OE>,
{
    pub fn new(spi: SPI, cs: CS) -> Self {
        Self { spi, cs, last_response: 0, block_size: 0, num_blocks: 0, position: 0 }
    }

    pub(crate) fn write_byte(&mut self, value: u8) -> Result<(), MciError> {
        self.spi.write(&[value]).map_err(|_| MciError::WriteError)
    }

    pub(crate) fn read_byte(&mut self) -> Result<u8, MciError> {
        let mut retval = 0xFF;
        self.spi.transfer(core::slice::from_mut(&mut retval)).map_err(|_| MciError::WriteError)?;
        Ok(retval)
    }

    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), MciError> {
        self.spi.write(bytes).map_err(|_| MciError::WriteError)
    }

    pub(crate) fn read_bytes<'a>(&mut self, bytes: &'a mut [u8]) -> Result<&'a [u8], MciError> {
        self.spi.transfer(bytes).map_err(|_| MciError::WriteError)?;
        Ok(bytes)
    }

    fn select(&mut self) -> Result<(), MciError> {
        self.cs.set_low().map_err(|_| MciError::CouldNotSelectDevice)
    }

    fn deselect(&mut self) -> Result<(), MciError> {
        self.cs.set_high().map_err(|_| MciError::CouldNotSelectDevice)
    }

    pub(crate) fn wait_busy(&mut self) -> Result<(), MciError> {
        // Delay before check busy
        self.read_byte()?;

        // Wait end of busy signal
        self.read_byte()?;

        let mut nec_timeout = 200_000;
        while self.read_byte()? != 0xFF && nec_timeout > 0 {
            nec_timeout -= 1;
        }
        if nec_timeout == 0 {
            return Err(MciError::DataError(CommandOrDataError::Timeout));
        }
        Ok(())
    }
}

impl<SPI, CS, E, OE> Bus for SpiBus<SPI, CS>
where
    SPI: spi::Transfer<u8, Error = E> + spi::Write<u8, Error = E>,
    CS: OutputPin<Error = OE>,
{
    fn init(&mut self) -> Result<(), MciError> {
        // Supply minimum of 74 clock cycles without CS asserted.
        self.send_clock()
    }

    fn deinit(&mut self) -> Result<(), MciError> {
        Ok(()) // NOP
    }

    fn send_clock(&mut self) -> Result<(), MciError> {
        self.deselect()?;
        // Send 80 cycles
        for _ in 0..10 {
            self.write_byte(0xFF)?; // 8 cycles
        }
        self.select()
    }

    fn select_device(
        &mut self,
        _slot: u8,
        _clock: u32,
        _bus_width: &BusWidth,
        _high_speed: bool,
    ) -> Result<(), MciError> {
        Ok(())
    }

    fn deselect_device(&mut self, _slot: u8) -> Result<(), MciError> {
        Ok(())
    }

    fn send_command(&mut self, cmd: u32, arg: u32) -> Result<(), MciError> {
        self.adtc_start(cmd, arg, 0, 0, false)
    }

    fn get_response(&mut self) -> Result<u32, MciError> {
        Ok(self.last_response)
    }
}
