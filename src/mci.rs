use embedded_error::mci::MciError;

use crate::bus::{Adtc, Bus, Read, Write};
use crate::command_arguments::mmc::BusWidth;

// TODO keep + get current selected slot
pub trait Mci: Bus + Read + Write + Adtc {
    /// Get the maximum bus width for a device
    fn get_bus_width(&mut self, slot: u8) -> Result<BusWidth, MciError>;

    /// Whether the device is high speed capable
    fn is_high_speed_capable(&mut self) -> Result<bool, MciError>;

    /// Get 128 bits response of last command
    fn get_response128(&mut self) -> Result<[u32; 4], MciError>;
}
