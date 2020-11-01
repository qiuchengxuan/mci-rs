pub mod spi;

use embedded_error::mci::MciError;

use crate::command_arguments::mmc::BusWidth;

pub const SD_MMC_BLOCK_SIZE: usize = 512;

pub trait Bus {
    /// Initialize MCI low level driver.
    fn init(&mut self) -> Result<(), MciError>;

    /// Deinitialize MCI low level driver.
    fn deinit(&mut self) -> Result<(), MciError>;

    /// Select a device and initialize it
    fn select_device(
        &mut self,
        slot: u8,
        clock: u32,
        bus_width: &BusWidth,
        high_speed: bool,
    ) -> Result<(), MciError>;

    /// Deselect device
    fn deselect_device(&mut self, slot: u8) -> Result<(), MciError>;

    /// Send 74 clock cycles on the line. Required after card plug and install
    fn send_clock(&mut self) -> Result<(), MciError>;

    fn send_command(&mut self, cmd: u32, arg: u32) -> Result<(), MciError>;

    /// Get 32 bits response of last command
    fn get_response(&mut self) -> Result<u32, MciError>;
}

pub trait Adtc {
    /// ADTC command start
    /// An ADTC (Addressed Data Transfer Commands) is used for R/W access
    ///
    /// # Arguments
    /// * `command`: 32bit command
    /// * `argument`: Argument of the command
    /// * `block_size`: 16bit block size
    /// * `block_amount`: Amount of blocks to transfer
    /// * `access_in_blocks`: If true - read_blocks/write_blocks must be used after this command
    ///                 Otherwise read_word/write_word must be used
    fn adtc_start(
        &mut self,
        command: u32,
        argument: u32,
        block_size: u16,
        block_amount: u16,
        access_in_blocks: bool,
    ) -> Result<(), MciError>;

    /// ADTC command stop
    /// Send a command to stop an ADTC
    /// # Arguments
    /// * `command`: 32bit command
    /// * `argument`: Argument of the command
    fn adtc_stop(&self, command: u32, argument: u32) -> Result<(), MciError>;
}

pub trait Read {
    /// Read a word on the wire
    fn read_word(&mut self) -> Result<u32, MciError>;

    /// Start a read block transfer on the line
    /// # Arguments
    ///  * `destination` Buffer to write to
    ///  * `number_of_blocks` Number of blocks to read
    fn read_blocks(&mut self, blocks: &mut [u8]) -> Result<(), MciError>;

    /// Wait until the end of reading the blocks
    fn wait_until_read_finished(&mut self) -> Result<(), MciError>;
}

pub trait Write {
    /// Write a word on the wire
    fn write_word(&mut self, val: u32) -> Result<(), MciError>;

    /// Start a write block transfer on the line
    /// # Arguments
    ///  * `data` - Data to write on the line
    fn write_blocks(&mut self, blocks: &[u8]) -> Result<(), MciError>;

    /// Wait until the end of writing blocks
    fn wait_until_write_finished(&mut self) -> Result<(), MciError>;
}

pub trait SpiBus: Bus + Adtc + Read + Write {}

pub trait SdMmcBus: Bus + Adtc + Read + Write {
    // TODO keep + get current selected slot
    /// Get the maximum bus width for a device
    fn get_bus_width(&mut self, slot: u8) -> Result<BusWidth, MciError>;

    /// Whether the device is high speed capable
    fn is_high_speed_capable(&mut self) -> Result<bool, MciError>;

    /// Get 128 bits response of last command
    fn get_response128(&mut self) -> Result<[u32; 4], MciError>;
}
