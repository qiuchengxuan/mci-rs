pub trait Field: Into<u8> {}

pub trait BitField<F: Field>: Copy + Into<u8> {
    fn has(&self, field: F) -> bool {
        let value: u8 = (*self).into();
        value & (1 << field.into()) > 0
    }

    fn no(&self, field: F) -> Option<()> {
        return if !self.has(field) { Some(()) } else { None };
    }
}

#[derive(Copy, Clone, Debug)]
#[allow(dead_code)]
pub enum R1ResponseField {
    Idle = 0,
    EraseReset,
    IllegalCommand,
    CommandCRC,
    EraseSequence,
    Address,
    Parameter,
    Error,
}

impl Into<u8> for R1ResponseField {
    fn into(self) -> u8 {
        self as u8
    }
}

impl Field for R1ResponseField {}

#[derive(Copy, Clone, Debug)]
pub(crate) struct R1Response(pub u8);

impl From<u8> for R1Response {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl Into<u8> for R1Response {
    fn into(self) -> u8 {
        self.0 as u8
    }
}

impl BitField<R1ResponseField> for R1Response {}

pub const BLOCK_READ_DATA_TOKEN: u8 = 0xFE;

#[derive(Copy, Clone, Debug)]
#[allow(dead_code)]
pub enum ErrorTokenField {
    Error = 0,
    CCError,
    CardECCFailed,
    OutOfRange,
    CardIsLocked,
}

impl Into<u8> for ErrorTokenField {
    fn into(self) -> u8 {
        self as u8
    }
}

impl Field for ErrorTokenField {}

#[derive(Copy, Clone, Debug)]
pub(crate) struct ErrorToken(pub u8);

impl ErrorToken {
    pub fn try_from(value: u8) -> Option<Self> {
        return if value & 0xF0 == 0 { Some(Self(value)) } else { None };
    }
}

impl Into<u8> for ErrorToken {
    fn into(self) -> u8 {
        self.0 as u8
    }
}

impl BitField<ErrorTokenField> for ErrorToken {}

pub enum WriteToken {
    SingleWrite = 0xFE,
    MultiWrite = 0xFC,
    StopTransmit = 0xFD,
}

pub enum ResponseCode {
    Accepted,
    CRCError,
    WriteError,
}

#[derive(Copy, Clone, Debug)]
pub struct ReadToken(u8);

impl ReadToken {
    pub fn try_from(value: u8) -> Option<Self> {
        // 0bxxx0xxx1
        return if value & 0b10001 == 0b00001 { Some(Self(value)) } else { None };
    }

    pub fn response_code(self) -> Option<ResponseCode> {
        match self.0 >> 1 & 0b111 {
            0x2 => Some(ResponseCode::Accepted),
            0x5 => Some(ResponseCode::CRCError),
            0x6 => Some(ResponseCode::WriteError),
            _ => None,
        }
    }
}
