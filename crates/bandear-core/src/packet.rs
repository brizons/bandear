//! The PMON audio datagram: the twenty bytes every audio packet opens with, and the interleaved
//! PCM16 samples behind them.
//!
//! Layout and field order are the server's (`encode_audio_packet` in `apps/server/src/protocol.rs`):
//! the magic `PMON`, a version, the channel count, the sample rate (4 bytes, little endian), the
//! stream's block counter (8 bytes, little endian) and how many samples follow (2 bytes, little
//! endian), then that many PCM16 samples, little endian. There is no timestamp: `sequence` is the
//! engine's block counter and the only ordering the client gets.
//!
//! Ported from the iOS client's `AudioDatagram.swift`. Where the two clients refused different
//! packets, this decoder refuses everything either of them refused: a header with no channels, a
//! sample count that is not a whole number of frames (iOS), and a sample rate of 0 or above
//! `i32::MAX` (Android, which reads the rate as a signed `Int` and refuses anything not positive).

use std::fmt;

/// The twenty-byte header of an audio datagram.
///
/// Only [`AudioDatagramHeader::decode`] makes one, so a header always has at least one channel and
/// a sample count that is a whole number of frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioDatagramHeader {
    channels: u8,
    sample_rate: u32,
    sequence: u64,
    sample_count: usize,
}

impl AudioDatagramHeader {
    /// `4 + 1 + 1 + 4 + 8 + 2`, matching `HEADER_SIZE` in `apps/server/src/protocol.rs`.
    pub const BYTE_COUNT: usize = 20;

    /// The four bytes every audio datagram starts with.
    pub const MAGIC: [u8; 4] = *b"PMON";

    /// The protocol version this decoder was written against.
    pub const SUPPORTED_VERSION: u8 = 1;

    /// The highest sample rate a header may declare. Android reads the four rate bytes as a signed
    /// `Int` and refuses one that is not positive, so a rate with the top bit set was never
    /// playable there; refusing it here keeps both phones on the same packets.
    pub const MAXIMUM_SAMPLE_RATE: u32 = i32::MAX as u32;

    /// Reads the header at the start of `bytes`; what follows the twenty header bytes is not read.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < Self::BYTE_COUNT {
            return Err(DecodeError::HeaderTooShort {
                byte_count: bytes.len(),
            });
        }

        let magic: [u8; 4] = read_array(bytes, 0);
        if magic != Self::MAGIC {
            return Err(DecodeError::UnknownMagic(magic));
        }

        let version = bytes[4];
        if version != Self::SUPPORTED_VERSION {
            return Err(DecodeError::UnsupportedVersion(version));
        }

        let channels = bytes[5];
        if channels == 0 {
            return Err(DecodeError::ZeroChannels);
        }

        let sample_rate = u32::from_le_bytes(read_array(bytes, 6));
        let sequence = u64::from_le_bytes(read_array(bytes, 10));
        let sample_count = usize::from(u16::from_le_bytes(read_array(bytes, 18)));

        if sample_rate == 0 || sample_rate > Self::MAXIMUM_SAMPLE_RATE {
            return Err(DecodeError::InvalidSampleRate(sample_rate));
        }

        if sample_count % usize::from(channels) != 0 {
            return Err(DecodeError::SampleCountNotDivisibleByChannels {
                sample_count,
                channels,
            });
        }

        Ok(Self {
            channels,
            sample_rate,
            sequence,
            sample_count,
        })
    }

    /// Channels per frame, at least one.
    pub fn channels(&self) -> u8 {
        self.channels
    }

    /// Frames per second.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The engine's block counter for this packet.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Interleaved samples the payload carries, a whole number of frames.
    pub fn sample_count(&self) -> usize {
        self.sample_count
    }

    /// Samples per channel. Safe to divide: a header with no channels never decodes.
    pub fn frame_count(&self) -> usize {
        self.sample_count / usize::from(self.channels)
    }

    /// How many payload bytes `sample_count` PCM16 samples take.
    pub fn payload_byte_count(&self) -> usize {
        self.sample_count * 2
    }

    /// The payload behind this header in `bytes`, refused unless it is exactly the declared
    /// samples long.
    fn payload<'bytes>(&self, bytes: &'bytes [u8]) -> Result<&'bytes [u8], DecodeError> {
        let payload = &bytes[Self::BYTE_COUNT..];
        if payload.len() != self.payload_byte_count() {
            return Err(DecodeError::PayloadLengthMismatch {
                declared_sample_count: self.sample_count,
                payload_byte_count: payload.len(),
            });
        }
        Ok(payload)
    }
}

/// The `N` bytes of `bytes` from `offset` on. The caller has checked the length.
fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> [u8; N] {
    let mut array = [0; N];
    array.copy_from_slice(&bytes[offset..offset + N]);
    array
}

/// The PCM16 samples of a payload already checked to be whole samples, in order.
fn payload_samples(payload: &[u8]) -> impl Iterator<Item = i16> + '_ {
    payload
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
}

/// A whole audio datagram: its header and the interleaved PCM16 samples behind it.
///
/// [`AudioDatagram::decode`] is the convenient shape, and it allocates the sample vector. The
/// receive path uses [`AudioDatagram::decode_into`] instead, which copies the payload straight into
/// the caller's own buffer and allocates nothing per packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDatagram {
    header: AudioDatagramHeader,
    samples: Vec<i16>,
}

impl AudioDatagram {
    /// Decodes a whole datagram: the header, then exactly the samples it declares.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let header = AudioDatagramHeader::decode(bytes)?;
        let samples = payload_samples(header.payload(bytes)?).collect();
        Ok(Self { header, samples })
    }

    /// Decodes a whole datagram into the caller's buffer and answers its header: the samples land
    /// in `samples[..header.sample_count()]`, and the rest of `samples` is left as it was.
    ///
    /// Refuses exactly what [`AudioDatagram::decode`] refuses, plus a buffer too small for the
    /// declared samples, in which case `samples` is left untouched. Allocates nothing.
    pub fn decode_into(
        bytes: &[u8],
        samples: &mut [i16],
    ) -> Result<AudioDatagramHeader, DecodeError> {
        let header = AudioDatagramHeader::decode(bytes)?;
        let payload = header.payload(bytes)?;
        let Some(destination) = samples.get_mut(..header.sample_count) else {
            return Err(DecodeError::SampleBufferTooSmall {
                sample_count: header.sample_count,
                capacity: samples.len(),
            });
        };
        for (sample, decoded) in destination.iter_mut().zip(payload_samples(payload)) {
            *sample = decoded;
        }
        Ok(header)
    }

    /// The datagram's header.
    pub fn header(&self) -> &AudioDatagramHeader {
        &self.header
    }

    /// The interleaved PCM16 samples, `header().sample_count()` of them.
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }
}

/// Why a datagram was refused. Each case names what disagreed, so a client can log the reason
/// instead of a bare count of dropped packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Fewer bytes arrived than a header needs.
    HeaderTooShort { byte_count: usize },
    /// The first four bytes were not `PMON`; something else is on the port.
    UnknownMagic([u8; 4]),
    /// A protocol version this client was not written against.
    UnsupportedVersion(u8),
    /// A channel count of zero, which would make the frame count meaningless.
    ZeroChannels,
    /// A sample rate of 0, or above [`AudioDatagramHeader::MAXIMUM_SAMPLE_RATE`].
    InvalidSampleRate(u32),
    /// The payload is not exactly `declared_sample_count` PCM16 samples long.
    PayloadLengthMismatch {
        declared_sample_count: usize,
        payload_byte_count: usize,
    },
    /// The sample count leaves a frame half finished, so the stream would go out of phase.
    SampleCountNotDivisibleByChannels { sample_count: usize, channels: u8 },
    /// The caller's buffer holds fewer samples than the datagram declares
    /// ([`AudioDatagram::decode_into`] only).
    SampleBufferTooSmall {
        sample_count: usize,
        capacity: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderTooShort { byte_count } => write!(
                formatter,
                "datagram of {byte_count} bytes is shorter than the {}-byte PMON header",
                AudioDatagramHeader::BYTE_COUNT
            ),
            Self::UnknownMagic(magic) => {
                write!(formatter, "datagram does not start with PMON: {magic:02x?}")
            }
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "PMON version {version} is not supported (expected {})",
                AudioDatagramHeader::SUPPORTED_VERSION
            ),
            Self::ZeroChannels => write!(formatter, "PMON header declares zero channels"),
            Self::InvalidSampleRate(sample_rate) => write!(
                formatter,
                "PMON sample rate {sample_rate} Hz is outside 1 to {}",
                AudioDatagramHeader::MAXIMUM_SAMPLE_RATE
            ),
            Self::PayloadLengthMismatch {
                declared_sample_count,
                payload_byte_count,
            } => write!(
                formatter,
                "PMON payload of {payload_byte_count} bytes is not the {declared_sample_count} \
                 declared PCM16 samples"
            ),
            Self::SampleCountNotDivisibleByChannels {
                sample_count,
                channels,
            } => write!(
                formatter,
                "PMON sample count {sample_count} is not a whole number of {channels}-channel frames"
            ),
            Self::SampleBufferTooSmall {
                sample_count,
                capacity,
            } => write!(
                formatter,
                "{sample_count} PMON samples do not fit a buffer of {capacity}"
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The packet the server's own test pins down, byte for byte: two stereo samples at 48 kHz,
    /// sequence 9, carrying -1 and 0x1234 (`serializes_documented_header_and_pcm_payload` in
    /// `apps/server/src/protocol.rs`).
    const GOLDEN_PACKET: [u8; 24] = [
        0x50, 0x4D, 0x4F, 0x4E, // "PMON"
        1,    // version
        2,    // channels
        0x80, 0xBB, 0x00, 0x00, // sample rate 48000, little endian
        9, 0, 0, 0, 0, 0, 0, 0, // sequence 9, little endian
        2, 0, // sample count 2, little endian
        0xFF, 0xFF, // -1
        0x34, 0x12, // 4660
    ];

    /// The fields of a packet built by [`packet`], defaulting to a valid stereo 48 kHz header.
    struct Packet {
        version: u8,
        channels: u8,
        sample_rate: u32,
        sequence: u64,
        sample_count: u16,
        payload_byte_count: Option<usize>,
    }

    impl Packet {
        /// A valid packet with `sample_count` stereo samples of silence, for the cases where only
        /// the header fields matter.
        fn new(sample_count: u16) -> Self {
            Self {
                version: 1,
                channels: 2,
                sample_rate: 48_000,
                sequence: 0,
                sample_count,
                payload_byte_count: None,
            }
        }

        fn bytes(&self) -> Vec<u8> {
            let mut bytes = b"PMON".to_vec();
            bytes.push(self.version);
            bytes.push(self.channels);
            bytes.extend_from_slice(&self.sample_rate.to_le_bytes());
            bytes.extend_from_slice(&self.sequence.to_le_bytes());
            bytes.extend_from_slice(&self.sample_count.to_le_bytes());
            let payload = self
                .payload_byte_count
                .unwrap_or(usize::from(self.sample_count) * 2);
            bytes.resize(bytes.len() + payload, 0);
            bytes
        }
    }

    fn packet(sample_count: u16) -> Vec<u8> {
        Packet::new(sample_count).bytes()
    }

    #[test]
    fn header_byte_count() {
        assert_eq!(AudioDatagramHeader::BYTE_COUNT, 20);
    }

    #[test]
    fn decodes_golden_packet() {
        let datagram = AudioDatagram::decode(&GOLDEN_PACKET).unwrap();

        assert_eq!(datagram.header().channels(), 2);
        assert_eq!(datagram.header().sample_rate(), 48_000);
        assert_eq!(datagram.header().sequence(), 9);
        assert_eq!(datagram.header().sample_count(), 2);
        assert_eq!(datagram.samples(), [-1, 4660]);
    }

    #[test]
    fn golden_frame_count() {
        assert_eq!(
            AudioDatagram::decode(&GOLDEN_PACKET)
                .unwrap()
                .header()
                .frame_count(),
            1
        );
    }

    #[test]
    fn reads_wide_sequence() {
        let header = AudioDatagramHeader::decode(
            &Packet {
                sequence: 0x0102_0304_0506_0708,
                ..Packet::new(0)
            }
            .bytes(),
        )
        .unwrap();

        assert_eq!(header.sequence(), 0x0102_0304_0506_0708);
    }

    #[test]
    fn reads_sample_rate() {
        let header = AudioDatagramHeader::decode(
            &Packet {
                sample_rate: 44_100,
                ..Packet::new(0)
            }
            .bytes(),
        )
        .unwrap();

        assert_eq!(header.sample_rate(), 44_100);
    }

    #[test]
    fn accepts_empty_payload() {
        let datagram = AudioDatagram::decode(&packet(0)).unwrap();

        assert!(datagram.samples().is_empty());
        assert_eq!(datagram.header().frame_count(), 0);
    }

    #[test]
    fn decodes_every_declared_sample() {
        let datagram = AudioDatagram::decode(&packet(480)).unwrap();

        assert_eq!(datagram.samples().len(), 480);
        assert_eq!(datagram.header().frame_count(), 240);
    }

    #[test]
    fn refuses_short_header() {
        for byte_count in [0, 1, 19] {
            let bytes = &GOLDEN_PACKET[..byte_count];

            assert_eq!(
                AudioDatagramHeader::decode(bytes),
                Err(DecodeError::HeaderTooShort { byte_count }),
                "{byte_count} bytes"
            );
        }
    }

    #[test]
    fn refuses_unknown_magic() {
        let mut bytes = GOLDEN_PACKET;
        bytes[0] = 0x51; // "Q"

        assert_eq!(
            AudioDatagramHeader::decode(&bytes),
            Err(DecodeError::UnknownMagic([0x51, 0x4D, 0x4F, 0x4E]))
        );
    }

    #[test]
    fn refuses_unsupported_version() {
        for version in [0, 2, 255] {
            assert_eq!(
                AudioDatagramHeader::decode(
                    &Packet {
                        version,
                        ..Packet::new(0)
                    }
                    .bytes()
                ),
                Err(DecodeError::UnsupportedVersion(version))
            );
        }
    }

    #[test]
    fn refuses_zero_channels() {
        assert_eq!(
            AudioDatagramHeader::decode(
                &Packet {
                    channels: 0,
                    ..Packet::new(0)
                }
                .bytes()
            ),
            Err(DecodeError::ZeroChannels)
        );
    }

    #[test]
    fn refuses_truncated_payload() {
        let bytes = Packet {
            payload_byte_count: Some(6),
            ..Packet::new(4)
        }
        .bytes();

        assert_eq!(
            AudioDatagram::decode(&bytes),
            Err(DecodeError::PayloadLengthMismatch {
                declared_sample_count: 4,
                payload_byte_count: 6
            })
        );
    }

    #[test]
    fn refuses_overlong_payload() {
        let bytes = Packet {
            payload_byte_count: Some(10),
            ..Packet::new(2)
        }
        .bytes();

        assert_eq!(
            AudioDatagram::decode(&bytes),
            Err(DecodeError::PayloadLengthMismatch {
                declared_sample_count: 2,
                payload_byte_count: 10
            })
        );
    }

    #[test]
    fn refuses_odd_sample_count_on_stereo() {
        assert_eq!(
            AudioDatagramHeader::decode(&packet(3)),
            Err(DecodeError::SampleCountNotDivisibleByChannels {
                sample_count: 3,
                channels: 2
            })
        );
    }

    #[test]
    fn magic_is_checked_first() {
        let mut bytes = Packet {
            version: 9,
            ..Packet::new(0)
        }
        .bytes();
        bytes[0] = 0x51;

        assert_eq!(
            AudioDatagramHeader::decode(&bytes),
            Err(DecodeError::UnknownMagic([0x51, 0x4D, 0x4F, 0x4E]))
        );
    }

    #[test]
    fn decodes_from_offset_slice() {
        let mut padded = vec![0xAA, 0xBB, 0xCC];
        padded.extend_from_slice(&GOLDEN_PACKET);
        let slice = &padded[3..];

        let datagram = AudioDatagram::decode(slice).unwrap();

        assert_eq!(datagram.header().sequence(), 9);
        assert_eq!(datagram.samples(), [-1, 4660]);
    }

    // Beyond the iOS client: Android's refusals, decided in the feature log (L3, L7).

    /// Android refuses a sample rate that is not positive (`PmonPacket.kt:46`).
    #[test]
    fn refuses_a_sample_rate_of_zero() {
        assert_eq!(
            AudioDatagramHeader::decode(
                &Packet {
                    sample_rate: 0,
                    ..Packet::new(0)
                }
                .bytes()
            ),
            Err(DecodeError::InvalidSampleRate(0))
        );
    }

    /// Android reads the rate as a signed `Int` (`PmonPacket.kt:39`), so one with the top bit set
    /// is negative there and refused; `i32::MAX` itself is still a rate.
    #[test]
    fn refuses_a_sample_rate_above_i32_max() {
        for sample_rate in [i32::MAX as u32 + 1, u32::MAX] {
            assert_eq!(
                AudioDatagramHeader::decode(
                    &Packet {
                        sample_rate,
                        ..Packet::new(0)
                    }
                    .bytes()
                ),
                Err(DecodeError::InvalidSampleRate(sample_rate))
            );
        }

        let highest = AudioDatagramHeader::decode(
            &Packet {
                sample_rate: i32::MAX as u32,
                ..Packet::new(0)
            }
            .bytes(),
        )
        .unwrap();
        assert_eq!(
            highest.sample_rate(),
            AudioDatagramHeader::MAXIMUM_SAMPLE_RATE
        );
    }

    // The receive path's shape: the payload copied into the caller's own buffer.

    #[test]
    fn decode_into_copies_the_samples_into_the_callers_buffer() {
        let mut samples = [7_i16; 4];

        let header = AudioDatagram::decode_into(&GOLDEN_PACKET, &mut samples).unwrap();

        assert_eq!(header.sequence(), 9);
        assert_eq!(header.sample_count(), 2);
        assert_eq!(
            samples,
            [-1, 4660, 7, 7],
            "only the declared samples change"
        );
    }

    #[test]
    fn decode_into_refuses_what_decode_refuses() {
        let mut samples = [7_i16; 8];
        let truncated = Packet {
            payload_byte_count: Some(6),
            ..Packet::new(4)
        }
        .bytes();

        assert_eq!(
            AudioDatagram::decode_into(&truncated, &mut samples),
            Err(DecodeError::PayloadLengthMismatch {
                declared_sample_count: 4,
                payload_byte_count: 6
            })
        );
        assert_eq!(
            AudioDatagram::decode_into(&GOLDEN_PACKET[..19], &mut samples),
            Err(DecodeError::HeaderTooShort { byte_count: 19 })
        );
        assert_eq!(samples, [7; 8]);
    }

    #[test]
    fn decode_into_refuses_a_buffer_too_small_and_leaves_it_alone() {
        let mut samples = [7_i16; 1];

        assert_eq!(
            AudioDatagram::decode_into(&GOLDEN_PACKET, &mut samples),
            Err(DecodeError::SampleBufferTooSmall {
                sample_count: 2,
                capacity: 1
            })
        );
        assert_eq!(samples, [7]);
    }

    #[test]
    fn every_refusal_names_what_disagreed() {
        let cases = [
            (DecodeError::HeaderTooShort { byte_count: 19 }, "19"),
            (DecodeError::UnknownMagic(*b"QMON"), "51"),
            (DecodeError::UnsupportedVersion(9), "9"),
            (DecodeError::ZeroChannels, "channels"),
            (DecodeError::InvalidSampleRate(0), "0"),
            (
                DecodeError::PayloadLengthMismatch {
                    declared_sample_count: 4,
                    payload_byte_count: 6,
                },
                "6",
            ),
            (
                DecodeError::SampleCountNotDivisibleByChannels {
                    sample_count: 3,
                    channels: 2,
                },
                "3",
            ),
            (
                DecodeError::SampleBufferTooSmall {
                    sample_count: 2,
                    capacity: 1,
                },
                "2",
            ),
        ];

        for (error, detail) in cases {
            let message = error.to_string();
            assert!(message.contains(detail), "{message:?} lacks {detail:?}");
        }
    }
}
