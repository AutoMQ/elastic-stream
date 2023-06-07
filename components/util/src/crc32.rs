use std::convert::AsRef;

use crc32fast::Hasher;

pub fn crc32<T>(data: T) -> u32
where
    T: AsRef<[u8]>,
{
    crc32fast::hash(data.as_ref())
}

pub fn crc32_vectored<U, V>(io_vec: V) -> u32
where
    V: Iterator<Item = U>,
    U: AsRef<[u8]>,
{
    let mut hasher = Hasher::new();
    for item in io_vec {
        hasher.update(item.as_ref());
    }
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_crc32_variant() {
        let payload = b"foo bar baz";
        let checksum = crc32fast::hash(payload);
        let hasher = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
        let iso_checksum = hasher.checksum(payload);
        assert_eq!(checksum, iso_checksum);
    }

    #[test]
    fn test_crc32_as_ref() {
        let payload = String::from("abcdef");
        let checksum = super::crc32(payload);
        assert_eq!(1267612143, checksum);
    }

    #[test]
    fn test_crc32_vectored() {
        let payloads = ["abc", "def"];
        let checksum = super::crc32_vectored(payloads.iter());
        assert_eq!(checksum, super::crc32(b"abcdef"));
        assert_eq!(1267612143, checksum);
    }
}
