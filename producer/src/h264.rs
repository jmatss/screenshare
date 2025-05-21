//! Copied from:
//! https://github.com/webrtc-rs/media/blob/de5e9db27d7aa5dd08eb576ce20de4ea2d26b8fd/src/io/h264_reader/mod.rs
//!
//! This copy of the H264Reader reads bytes asynchronous instead of synchronous.
//!
//! TODO: Fix error handling.
use bytes::{BufMut, Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt};
use webrtc::media::io::h264_reader::NalUnitType;

/// Since the errors used in the original `H264Reader` is private, need to create
/// new errors with the same "message".
type H264Result<T> = std::result::Result<T, H264Error>;

pub enum H264Error {
    ErrIoEOF(String),
    ErrDataIsNotH264Stream(String),
}

impl From<H264Error> for webrtc::Error {
    fn from(err: H264Error) -> Self {
        match err {
            H264Error::ErrIoEOF(s) => webrtc::Error::new(format!("H264Error::ErrIoEOF({})", s)),
            H264Error::ErrDataIsNotH264Stream(s) => {
                webrtc::Error::new(format!("H264Error::ErrDataIsNotH264Stream({})", s))
            }
        }
    }
}

/// NAL H.264 Network Abstraction Layer
#[allow(clippy::upper_case_acronyms)]
pub struct NAL {
    /// NAL header
    pub forbidden_zero_bit: bool,
    pub ref_idc: u8,
    pub unit_type: NalUnitType,

    /// header byte + rbsp
    pub data: BytesMut,
}

impl NAL {
    fn new(data: BytesMut) -> Self {
        NAL {
            forbidden_zero_bit: false,
            ref_idc: 0,
            unit_type: NalUnitType::Unspecified,
            data,
        }
    }

    fn parse_header(&mut self) {
        let first_byte = self.data[0];
        self.forbidden_zero_bit = ((first_byte & 0x80) >> 7) == 1; // 0x80 = 0b10000000
        self.ref_idc = (first_byte & 0x60) >> 5; // 0x60 = 0b01100000
        self.unit_type = NalUnitType::from(first_byte & 0x1F); // 0x1F = 0b00011111
    }
}

const NAL_PREFIX_3BYTES: [u8; 3] = [0, 0, 1];
const NAL_PREFIX_4BYTES: [u8; 4] = [0, 0, 0, 1];

/// H264Reader reads data from stream and constructs h264 nal units
pub struct H264Reader<R: AsyncRead + Unpin> {
    reader: R,
    nal_buffer: BytesMut,
    count_of_consecutive_zero_bytes: usize,
    nal_prefix_parsed: bool,
    read_buffer: Vec<u8>,
}

impl<R: AsyncRead + Unpin> H264Reader<R> {
    /// new creates new H264Reader
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            nal_buffer: BytesMut::new(),
            count_of_consecutive_zero_bytes: 0,
            nal_prefix_parsed: false,
            read_buffer: vec![],
        }
    }

    async fn read(&mut self, num_to_read: usize) -> Bytes {
        let mut buf = vec![0u8; 4096];
        while self.read_buffer.len() < num_to_read {
            let n = match self.reader.read(&mut buf).await {
                Ok(n) => {
                    if n == 0 {
                        break;
                    }
                    n
                }
                Err(_) => break,
            };

            self.read_buffer.extend_from_slice(&buf[0..n]);
        }

        let num_should_read = if num_to_read <= self.read_buffer.len() {
            num_to_read
        } else {
            self.read_buffer.len()
        };

        Bytes::from(
            self.read_buffer
                .drain(..num_should_read)
                .collect::<Vec<u8>>(),
        )
    }

    // TODO: Fix error handling.
    async fn bit_stream_starts_with_h264prefix(&mut self) -> H264Result<usize> {
        let prefix_buffer = self.read(4).await;

        let n = prefix_buffer.len();
        if n == 0 {
            return Err(H264Error::ErrIoEOF("n == 0 when reading prefix".into()));
        }

        if n < 3 {
            return Err(H264Error::ErrDataIsNotH264Stream("n < 3".into()));
        }

        let nal_prefix3bytes_found = NAL_PREFIX_3BYTES[..] == prefix_buffer[..3];
        if n == 3 {
            if nal_prefix3bytes_found {
                return Err(H264Error::ErrDataIsNotH264Stream(
                    "n == 3 && nal_prefix3bytes_found".into(),
                ));
            }
            return Err(H264Error::ErrDataIsNotH264Stream(
                "n == 3 && !nal_prefix3bytes_found".into(),
            ));
        }

        // n == 4
        if nal_prefix3bytes_found {
            self.nal_buffer.put_u8(prefix_buffer[3]);
            return Ok(3);
        }

        let nal_prefix4bytes_found = NAL_PREFIX_4BYTES[..] == prefix_buffer;
        if nal_prefix4bytes_found {
            Ok(4)
        } else {
            Err(H264Error::ErrDataIsNotH264Stream(
                "no nal prefix found".into(),
            ))
        }
    }

    /// next_nal reads from stream and returns then next NAL,
    /// and an error if there is incomplete frame data.
    /// Returns all nil values when no more NALs are available.
    pub async fn next_nal(&mut self) -> H264Result<NAL> {
        if !self.nal_prefix_parsed {
            self.bit_stream_starts_with_h264prefix().await?;

            self.nal_prefix_parsed = true;
        }

        loop {
            let buffer = self.read(1).await;
            let n = buffer.len();

            if n != 1 {
                break;
            }
            let read_byte = buffer[0];
            let nal_found = self.process_byte(read_byte);
            if nal_found {
                let nal_unit_type = NalUnitType::from(self.nal_buffer[0] & 0x1F);
                if nal_unit_type == NalUnitType::SEI {
                    self.nal_buffer.clear();
                    continue;
                } else {
                    break;
                }
            }

            self.nal_buffer.put_u8(read_byte);
        }

        if self.nal_buffer.is_empty() {
            return Err(H264Error::ErrIoEOF("self.nal_buffer.is_empty()".into()));
        }

        let mut nal = NAL::new(self.nal_buffer.split());
        nal.parse_header();

        Ok(nal)
    }

    fn process_byte(&mut self, read_byte: u8) -> bool {
        let mut nal_found = false;

        match read_byte {
            0 => {
                self.count_of_consecutive_zero_bytes += 1;
            }
            1 => {
                if self.count_of_consecutive_zero_bytes >= 2 {
                    let count_of_consecutive_zero_bytes_in_prefix =
                        if self.count_of_consecutive_zero_bytes > 2 {
                            3
                        } else {
                            2
                        };
                    let nal_unit_length =
                        self.nal_buffer.len() - count_of_consecutive_zero_bytes_in_prefix;
                    if nal_unit_length > 0 {
                        let _ = self.nal_buffer.split_off(nal_unit_length);
                        nal_found = true;
                    }
                }
                self.count_of_consecutive_zero_bytes = 0;
            }
            _ => {
                self.count_of_consecutive_zero_bytes = 0;
            }
        }

        nal_found
    }
}
