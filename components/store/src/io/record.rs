use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Length of record prefix: CRC(4B) + Size(3B) + Type(1B)
pub const RECORD_PREFIX_LENGTH: u64 = 4 + 3 + 1;

/// Type of the
///
/// +---------+-----------+-----------+--- ... ---+
/// |CRC (4B) | Size (3B) | Type (1B) | Payload   |
/// +---------+-----------+-----------+--- ... ---+
///
/// CRC = 32bit hash computed over the payload using CRC
/// Size = Length of the payload data
/// Type = Type of record
///        (ZeroType, FullType, FirstType, LastType, MiddleType )
///        The type is used to group a bunch of records together to represent
///        blocks that are larger than BlockSize
/// Payload = Byte stream as long as specified by the payload size
#[derive(Debug, TryFromPrimitive, IntoPrimitive, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum RecordType {
    /// Type `Zero` is used as the last record of a log segment file, aka, footer of the segment.
    /// A footer contains 0 or more `0`-filled bytes and timestamp of first and last records in unix timestamp.
    ///
    /// # Footer Example
    ///
    /// ...+---------+-----------+-----------+--- ... ---+-----+-----+
    ///    |CRC (4B) | Size (3B) | Type (1B) |      0s   |  T1 | T2  |
    /// ...+---------+-----------+-----------+--- ... ---+-----+-----+
    ///
    Zero = 0,

    /// Type `Full` is
    Full = 1,
    First = 2,
    Middle = 3,
    Last = 4,
}

impl RecordType {
    /// The first 3 bytes are to represent length
    /// The last byte is for record type.
    pub(crate) fn with_length(&self, len: u32) -> u32 {
        let l = len << 8;

        let t: u8 = match *self {
            RecordType::Zero => 0,
            RecordType::Full => 1,
            RecordType::First => 2,
            RecordType::Middle => 3,
            RecordType::Last => 4,
        };

        l | t as u32
    }

    #[cfg(test)]
    pub(crate) fn parse(val: u32) -> Result<(u32, Self), crate::error::StoreError> {
        let t = val & 0xFF;
        let t = RecordType::try_from(t as u8)
            .map_err(|_e| crate::error::StoreError::UnsupportedRecordType)?;
        Ok((val >> 8, t))
    }
}

#[cfg(test)]
mod tests {
    use crate::error::StoreError;

    use super::RecordType;

    #[test]
    fn test_with_length() -> Result<(), StoreError> {
        let len = 128;
        let val = RecordType::Full.with_length(len);
        assert_ne!(len, val); // Ensure bit-shift is indeed applied
        let (l, t) = RecordType::parse(val)?;
        assert_eq!(len, l);
        assert_eq!(RecordType::Full, t);
        Ok(())
    }
}
