use super::*;

use ogg::{PacketWriteEndInfo, PacketWriter};

const STREAM: u32 = 0x5EED;

/// A Vorbis identification header lewton accepts: mono, 44.1 kHz, blocks of
/// 256 and 2048 samples.
fn ident() -> Vec<u8> {
    ident_of(1, 11)
}

/// An identification header of `channels` at 44.1 kHz, with short blocks of
/// 256 samples and long ones of `2^long`.
fn ident_of(channels: u8, long: u8) -> Vec<u8> {
    let mut packet = b"\x01vorbis".to_vec();
    packet.extend_from_slice(&0u32.to_le_bytes()); // version
    packet.push(channels);
    packet.extend_from_slice(&44_100u32.to_le_bytes());
    packet.extend_from_slice(&[0; 12]); // bitrates
    packet.push(long << 4 | 8); // blocksizes
    packet.push(1); // framing
    packet
}

/// A comment header with this vendor-length field, comment count and
/// comment-length fields, holding no strings at all.
fn comment(vendor_len: u32, count: u32, lens: &[u32]) -> Vec<u8> {
    let mut packet = b"\x03vorbis".to_vec();
    packet.extend_from_slice(&vendor_len.to_le_bytes());
    packet.extend_from_slice(&count.to_le_bytes());
    for len in lens {
        packet.extend_from_slice(&len.to_le_bytes());
    }
    packet
}

/// Vorbis' bit packing, for writing setup headers by hand.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    at: u64,
}

impl BitWriter {
    fn put(&mut self, value: u64, n: u32) -> &mut Self {
        for i in 0..n {
            if self.at.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let bit = ((value >> i) & 1) as u8;
            *self.bytes.last_mut().expect("a byte") |= bit << (self.at % 8);
            self.at += 1;
        }
        self
    }
}

/// A setup header opening with these codebook bits, `count` codebooks.
fn setup(count: u8, books: &BitWriter) -> Vec<u8> {
    let mut packet = b"\x05vorbis".to_vec();
    packet.push(count - 1);
    packet.extend_from_slice(&books.bytes);
    packet
}

/// A codebook's opening: sync, dimensions, entries, the ordered flag.
fn open_book(w: &mut BitWriter, dimensions: u64, entries: u64, ordered: bool) {
    w.put(u64::from(CODEBOOK_SYNC), 24)
        .put(dimensions, 16)
        .put(entries, 24)
        .put(u64::from(ordered), 1);
}

/// One codebook of two entries, each code a bit long, with no lookup.
fn small_book() -> BitWriter {
    let mut w = BitWriter::default();
    open_book(&mut w, 1, 2, false);
    w.put(0, 1) // not sparse
        .put(0, 5) // length 1
        .put(0, 5) // length 1
        .put(0, 4); // no lookup
    w
}

/// An Ogg stream of `packets`, each `(serial, bytes)`, a page apiece.
fn stream(packets: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut writer = PacketWriter::new(&mut out);
    for (i, (serial, packet)) in packets.iter().enumerate() {
        let end = if i + 1 == packets.len() {
            PacketWriteEndInfo::EndStream
        } else {
            PacketWriteEndInfo::EndPage
        };
        writer
            .write_packet(packet.clone().into_boxed_slice(), *serial, end, 0)
            .expect("written");
    }
    drop(writer);
    out
}

/// The three headers of one stream.
fn headers(comment: Vec<u8>, setup: Vec<u8>) -> Vec<u8> {
    stream(&[(STREAM, ident()), (STREAM, comment), (STREAM, setup)])
}

#[test]
fn the_crafted_identification_headers_are_ones_lewton_reads() {
    // Every hostile stream below gets past them to the headers that allocate.
    assert!(lewton::header::read_header_ident(&ident()).is_ok());
    assert!(lewton::header::read_header_ident(&ident_of(8, 13)).is_ok());
}

#[test]
fn lookup1_values_matches_lewtons_own_vectors() {
    for (entries, dimensions, values) in [
        (1025, 10, 2),
        (1024, 10, 2),
        (1023, 10, 1),
        (3126, 5, 5),
        (3125, 5, 5),
        (3124, 5, 4),
        (1, 1, 1),
        (0, 15, 0),
        (0, 0, 0),
        (1, 0, u32::MAX),
        (400, 0, u32::MAX),
        (4096, 65_535, 1),
    ] {
        assert_eq!(
            lookup1_values(entries, dimensions),
            values,
            "lookup1_values({entries}, {dimensions})"
        );
    }
}

#[test]
fn bits_fill_each_byte_from_its_least_significant() {
    let mut bits = Bits {
        bytes: &[0b1010_0001, 0xFF],
        at: 0,
    };
    assert_eq!(bits.read(1), Ok(1));
    assert_eq!(bits.read(4), Ok(0));
    assert_eq!(bits.read(3), Ok(0b101));
    assert_eq!(bits.read(9), Err(Stop::Unreadable), "one byte left");
    assert_eq!(bits.read(8), Ok(0xFF));
}

#[test]
fn a_small_codebook_costs_what_it_declares() {
    assert_eq!(
        setup_budget(&setup(1, &small_book())),
        Ok(Budget {
            entries: 2,
            codeword_bits: 2,
            vector_values: 0,
        })
    );
    assert!(headers_bounded(&headers(
        comment(0, 0, &[]),
        setup(1, &small_book())
    )));
}

#[test]
fn bytes_that_hold_no_ogg_stream_pass_the_walk() {
    // No danger to lewton, and the decoder refuses them in its turn.
    assert!(headers_bounded(b"<!doctype html><p>Not found</p>"));
    assert!(headers_bounded(&[]));
}

/// A vendor string of 4 GiB, from a packet a few bytes long: lewton zeroes
/// the buffer before it reads a byte into it.
#[test]
fn a_comment_header_claiming_more_than_its_packet_is_refused() {
    let hostile = headers(comment(0xFFFF_FFF0, 0, &[]), setup(1, &small_book()));
    assert!(!headers_bounded(&hostile));
    assert_eq!(
        crate::world_builder::audio_probe::playable_clip(hostile).err(),
        Some(crate::world_builder::asset_failure::AssetFetchError::Undecodable)
    );
    let long_comment = headers(comment(0, 1, &[0xFFFF_FFF0]), setup(1, &small_book()));
    assert!(!headers_bounded(&long_comment));
}

/// A billion comments: lewton reserves room for every one up front.
#[test]
fn a_comment_header_claiming_more_comments_than_fit_is_refused() {
    let hostile = headers(comment(0, 1 << 30, &[]), setup(1, &small_book()));
    assert!(!headers_bounded(&hostile));
}

/// A setup header under 30 bytes: one ordered codebook of `entries` in
/// `dimensions`, a single run of one-bit codes, and lookup type 1 - whose
/// values, here a single one, lewton unpacks to `entries x dimensions`.
fn vector_book(dimensions: u64, entries: u32) -> Vec<u8> {
    let mut w = BitWriter::default();
    open_book(&mut w, dimensions, entries.into(), true);
    w.put(0, 5) // first length 1
        .put(entries.into(), ilog(entries)) // one run, every entry
        .put(1, 4) // lookup type 1
        .put(0, 32) // minimum
        .put(0, 32) // delta
        .put(0, 4) // values a bit wide
        .put(0, 1) // sequence flag
        .put(0, 1); // the single value
    setup(1, &w)
}

/// The critic's case (#1560): 4,096 entries in 65,535 dimensions unpack to
/// 268 million values, a gigabyte lewton allocates before it decodes a
/// sample - and every audio packet would loop over those dimensions.
#[test]
fn a_codebook_of_too_many_dimensions_is_refused() {
    let packet = vector_book(65_535, 4096);
    assert!(packet.len() < 30, "{} bytes", packet.len());
    assert_eq!(setup_budget(&packet), Err(Stop::Unbounded));
    assert!(!headers_bounded(&headers(comment(0, 0, &[]), packet)));
}

/// Within the dimensions, 131,072 entries in 64 unpack to 8 million
/// values from the same few bytes; 1,024 in 8 do not.
#[test]
fn a_tiny_codebook_claiming_a_vast_vector_table_is_refused() {
    assert_eq!(
        setup_budget(&vector_book(64, 1 << 17)),
        Err(Stop::Unbounded)
    );
    assert_eq!(
        setup_budget(&vector_book(8, 1 << 10)),
        Ok(Budget {
            entries: 1 << 10,
            codeword_bits: 1 << 10,
            vector_values: 1 << 13,
        })
    );
}

/// A lookup codebook of no dimensions: lewton reckons `u32::MAX` values and
/// reserves 16 GiB for them before reading the first.
#[test]
fn lookup_values_that_are_not_in_the_packet_are_refused() {
    let mut w = BitWriter::default();
    open_book(&mut w, 0, 1, false);
    w.put(0, 1) // not sparse
        .put(0, 5) // length 1
        .put(1, 4) // lookup type 1
        .put(0, 64) // minimum and delta
        .put(0, 4) // values a bit wide
        .put(0, 1); // sequence flag
    assert_eq!(setup_budget(&setup(1, &w)), Err(Stop::Unbounded));
}

#[test]
fn codebooks_declaring_too_many_entries_are_refused() {
    let mut w = BitWriter::default();
    open_book(&mut w, 1, 0xFF_FFFF, true);
    assert_eq!(setup_budget(&setup(1, &w)), Err(Stop::Unbounded));
}

/// A million entries in one run of 32-bit codes: 32 million tree nodes.
#[test]
fn an_ordered_run_of_long_codes_past_the_cap_is_refused() {
    let mut w = BitWriter::default();
    open_book(&mut w, 1, 1 << 20, true);
    w.put(31, 5).put(1 << 20, ilog(1 << 20));
    assert_eq!(setup_budget(&setup(1, &w)), Err(Stop::Unbounded));
}

/// Runs of no entries still lengthen the codes: past 32 bits no code is
/// Vorbis, and past 255 lewton's length counter wraps.
#[test]
fn ordered_code_lengths_past_32_bits_are_refused() {
    let mut w = BitWriter::default();
    open_book(&mut w, 1, 64, true);
    w.put(31, 5); // first length 32
    w.put(0, ilog(64)); // a run of none: the next length is 33
    w.put(64, ilog(64));
    assert_eq!(setup_budget(&setup(1, &w)), Err(Stop::Unbounded));
}

/// Eight channels at blocks of 8,192 samples: lewton sizes a type 2
/// residue as 65,536 in a `u16`, which wraps to nothing.
#[test]
fn a_stream_too_wide_for_lewtons_arithmetic_is_refused() {
    let wide = stream(&[
        (STREAM, ident_of(8, 13)),
        (STREAM, comment(0, 0, &[])),
        (STREAM, setup(1, &small_book())),
    ]);
    assert!(!headers_bounded(&wide));
    let narrower = stream(&[
        (STREAM, ident_of(8, 12)),
        (STREAM, comment(0, 0, &[])),
        (STREAM, setup(1, &small_book())),
    ]);
    assert!(headers_bounded(&narrower), "8 x 4,096 fits");
}

/// lewton reads headers again wherever a chained stream begins, in the
/// middle of playback: a clip whose first stream is sound and whose second
/// asks for gigabytes is refused, and a sound chain is not.
#[test]
fn every_chained_streams_headers_are_walked() {
    let sound = [
        (STREAM, ident()),
        (STREAM, comment(0, 0, &[])),
        (STREAM, setup(1, &small_book())),
        (STREAM, vec![0; 4]), // audio
    ];
    let chain = |second_comment: Vec<u8>| {
        let mut packets = sound.to_vec();
        packets.extend([
            (STREAM + 1, ident()),
            (STREAM + 1, second_comment),
            (STREAM + 1, setup(1, &small_book())),
            (STREAM + 1, vec![0; 4]),
        ]);
        stream(&packets)
    };
    assert!(headers_bounded(&chain(comment(0, 0, &[]))));
    assert!(!headers_bounded(&chain(comment(0xFFFF_FFF0, 0, &[]))));
}

/// lewton follows the stream its first packet names and skips any other
/// stream's packets; so does the walk, whichever stream the hostile header
/// is in.
#[test]
fn the_walk_reads_the_packets_lewton_reads() {
    let hostile = comment(0xFFFF_FFF0, 0, &[]);
    let fine = comment(0, 0, &[]);
    let other = STREAM + 1;
    let skipped = stream(&[
        (STREAM, ident()),
        (other, hostile.clone()),
        (STREAM, fine.clone()),
        (STREAM, setup(1, &small_book())),
    ]);
    assert!(
        headers_bounded(&skipped),
        "another stream's packet is not read"
    );
    let followed = stream(&[
        (STREAM, ident()),
        (other, fine),
        (STREAM, hostile),
        (STREAM, setup(1, &small_book())),
    ]);
    assert!(
        !headers_bounded(&followed),
        "the stream's own comment is read"
    );
}
