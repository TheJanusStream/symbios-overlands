//! A bounded walk through an Ogg Vorbis stream's comment and setup headers,
//! ahead of lewton's own read of them (#1560). lewton 0.10, the Vorbis
//! decoder under Bevy's, sizes allocations from header fields before it
//! checks them against the packet they came in: the vendor string, the
//! comment count and each comment (up to 4 GiB apiece), and each setup
//! codebook's code lengths, Huffman tree, lookup values and vector table (up
//! to `entries x dimensions`, 1.1e12). A setup header a few dozen bytes long
//! can claim any of them - no cap on a clip's length catches that - and an
//! allocation the web page cannot grow to aborts the client. Decoding is
//! sized by the headers as well: a codebook's dimensions set how much each
//! audio packet allocates and loops over, and a type 2 residue is sized in
//! 16-bit arithmetic that wraps for many channels at long blocks. And
//! lewton reads headers again in the middle of a clip, wherever a chained
//! stream begins.
//!
//! So the very packets lewton will parse are walked here first without
//! allocating - the first packet's stream and its next two, then every
//! chained stream's three as playback meets them - and a clip whose headers
//! ask for more than their bytes can hold, or than any real encoder writes,
//! is refused.

use std::io::Cursor;

use ogg::{Packet, PacketReader};

/// The most codebook entries a stream's codebooks may declare between them
/// (lewton keeps a code length a byte apiece, before reading any). The 35
/// Vorbis clips of the freedesktop sound theme declare 3,216 to 11,813.
const MAX_CODEBOOK_ENTRIES: u64 = 1 << 20;
/// The most code-word bits a stream's codebooks may total: lewton builds a
/// Huffman tree node, some forty bytes, for each bit of each code word, so
/// this caps the trees near 48 MB. The freedesktop clips total 32,975 to
/// 48,142.
const MAX_CODEWORD_BITS: u64 = 1 << 20;
/// The most values a stream's lookup codebooks may unpack their vector
/// tables to, four bytes apiece. The freedesktop clips unpack 7,155 to
/// 61,254.
const MAX_VECTOR_VALUES: u64 = 1 << 22;
/// The most dimensions a codebook may have: each audio packet's residue
/// decode allocates and loops over its classbook's for every channel, and
/// reads that many values per vector. The freedesktop clips' 1,425
/// codebooks have 1, 2, 4 or 8.
const MAX_CODEBOOK_DIMENSIONS: u64 = 64;
/// The longest code word Vorbis allows (Vorbis I, 3.2.1). An ordered
/// codebook's lengths count up a run at a time, in a counter lewton keeps
/// in a byte.
const MAX_CODE_LENGTH: u64 = 32;
/// The 24 bits every codebook opens with (Vorbis I, 3.2.1).
const CODEBOOK_SYNC: u32 = 0x56_4342;

type Reader<'a> = PacketReader<Cursor<&'a [u8]>>;

/// Whether lewton can read and decode the Ogg Vorbis streams in `bytes`
/// without allocating more than their bytes pay for, or than any real
/// stream asks. Bytes holding no Ogg stream, or whose headers lewton refuses
/// before it allocates, pass: they are no danger, and the decoder refuses
/// them in its turn.
pub(super) fn headers_bounded(bytes: &[u8]) -> bool {
    let mut reader = PacketReader::new(Cursor::new(bytes));
    // As lewton's `read_headers`: the first packet names the stream, and
    // that stream's next two packets are its comment and setup headers.
    let Some(ident) = read(&mut reader) else {
        return true;
    };
    let mut serial = ident.stream_serial();
    if !ident_bounded(&ident.data) {
        return false;
    }
    let Some(comment) = next_in(&mut reader, serial) else {
        return true;
    };
    if !comment_bounded(&comment.data) {
        return false;
    }
    let Some(setup) = next_in(&mut reader, serial) else {
        return true;
    };
    if !setup_bounded(&setup.data) {
        return false;
    }
    reader.delete_unread_packets();
    // As lewton's `read_next_audio_packet`, through the rest of the clip as
    // playback reads it: a packet opening another stream chains it, its
    // comment and setup headers are the next two packets whichever stream
    // they belong to, and lewton then decodes one packet to prime itself
    // and hands back the one after.
    loop {
        let Some(packet) = read(&mut reader) else {
            return true;
        };
        if packet.stream_serial() == serial || !packet.first_in_stream() {
            continue;
        }
        if !ident_bounded(&packet.data) {
            return false;
        }
        let Some(comment) = read(&mut reader) else {
            return true;
        };
        if !comment_bounded(&comment.data) {
            return false;
        }
        let Some(setup) = read(&mut reader) else {
            return true;
        };
        if !setup_bounded(&setup.data) {
            return false;
        }
        serial = setup.stream_serial();
        if read(&mut reader).is_none() || read(&mut reader).is_none() {
            return true;
        }
    }
}

/// The next packet - none at the end of the bytes or at a page the Ogg
/// layer cannot read, where lewton stops too.
fn read(reader: &mut Reader) -> Option<Packet> {
    reader.read_packet().ok().flatten()
}

/// The next packet of the stream `serial`, skipping any other's.
fn next_in(reader: &mut Reader, serial: u32) -> Option<Packet> {
    loop {
        let packet = read(reader)?;
        if packet.stream_serial() == serial {
            return Some(packet);
        }
    }
}

/// Whether the stream an identification header opens decodes within
/// lewton's 16-bit arithmetic: a type 2 residue is sized as a block's
/// samples times the channels in a `u16`, which wraps past 65,535 and
/// leaves the decode indexing past the vector it sized. A packet too short
/// to say is no identification header lewton reads.
fn ident_bounded(packet: &[u8]) -> bool {
    let Some(header) = packet.strip_prefix(b"\x01vorbis") else {
        return true;
    };
    let (Some(&channels), Some(&blocksizes)) = (header.get(4), header.get(21)) else {
        return true;
    };
    // The long block's size is the high four bits' power of two.
    u32::from(channels) << (blocksizes >> 4) <= u32::from(u16::MAX)
}

/// Whether lewton's read of the setup header `packet` stays within bounds.
fn setup_bounded(packet: &[u8]) -> bool {
    !matches!(setup_budget(packet), Err(Stop::Unbounded))
}

/// Whether lewton's read of a comment header (`read_header_comment`) asks
/// for no more than the packet holds: a vendor string and comments no longer
/// than what is left of it, and no more comments than length fields fit. A
/// packet that is no comment header stops lewton before it allocates.
fn comment_bounded(packet: &[u8]) -> bool {
    let Some(mut rest) = packet.strip_prefix(b"\x03vorbis") else {
        return true;
    };
    let Some(vendor) = take_len(&mut rest) else {
        return true;
    };
    let Some(after_vendor) = rest.get(vendor..) else {
        return false;
    };
    rest = after_vendor;
    let Some(count) = take_len(&mut rest) else {
        return true;
    };
    if count > rest.len() / 4 {
        return false;
    }
    for _ in 0..count {
        let Some(len) = take_len(&mut rest) else {
            return true;
        };
        let Some(after) = rest.get(len..) else {
            return false;
        };
        rest = after;
    }
    true
}

/// A little-endian `u32` length field off the front of `rest`.
fn take_len(rest: &mut &[u8]) -> Option<usize> {
    let (field, tail) = rest.split_first_chunk::<4>()?;
    *rest = tail;
    Some(u32::from_le_bytes(*field) as usize)
}

/// Why a walk through a setup header stops before its codebooks end.
#[derive(Debug, PartialEq)]
pub(super) enum Stop {
    /// Where lewton stops too - a bad sync, a lookup type past the spec, the
    /// end of the packet - having allocated nothing the bytes do not pay for.
    Unreadable,
    /// Where lewton would allocate more than the bytes pay for, or than any
    /// real stream asks.
    Unbounded,
}

/// What a setup header's codebooks ask lewton to allocate, all together.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Budget {
    /// Codebook entries declared.
    pub(super) entries: u64,
    /// Code-word bits, a Huffman tree node apiece.
    pub(super) codeword_bits: u64,
    /// Vector-table values unpacked.
    pub(super) vector_values: u64,
}

impl Budget {
    fn spend(total: &mut u64, amount: u64, cap: u64) -> Result<(), Stop> {
        *total = total.saturating_add(amount);
        if *total > cap {
            return Err(Stop::Unbounded);
        }
        Ok(())
    }
}

/// What the codebooks of the setup header `packet` ask lewton
/// (`read_header_setup`, `read_codebook`) to allocate - the rest of the
/// header is counts a few bits wide - or where the walk stops. A packet
/// that is no setup header stops lewton before it allocates, so it costs
/// nothing.
pub(super) fn setup_budget(packet: &[u8]) -> Result<Budget, Stop> {
    let mut budget = Budget::default();
    let Some(body) = packet.strip_prefix(b"\x05vorbis") else {
        return Ok(budget);
    };
    let mut bits = Bits { bytes: body, at: 0 };
    let count = bits.read(8)? + 1;
    for _ in 0..count {
        codebook(&mut bits, &mut budget)?;
    }
    Ok(budget)
}

/// Walk one codebook (Vorbis I, 3.2.1) as lewton reads it, charging
/// `budget` for each allocation before lewton would make it.
fn codebook(bits: &mut Bits, budget: &mut Budget) -> Result<(), Stop> {
    if bits.read(24)? != CODEBOOK_SYNC {
        return Err(Stop::Unreadable);
    }
    let dimensions = u64::from(bits.read(16)?);
    let entries = bits.read(24)?;
    let ordered = bits.read(1)? == 1;
    if dimensions > MAX_CODEBOOK_DIMENSIONS {
        return Err(Stop::Unbounded);
    }
    Budget::spend(&mut budget.entries, entries.into(), MAX_CODEBOOK_ENTRIES)?;
    if ordered {
        // Runs of entries, each a bit longer than the last - and a run can
        // overshoot its codebook's end before lewton notices.
        let mut length = u64::from(bits.read(5)?) + 1;
        let mut current = 0u32;
        while current < entries {
            if length > MAX_CODE_LENGTH {
                return Err(Stop::Unbounded);
            }
            let run = bits.read(ilog(entries - current))?;
            Budget::spend(
                &mut budget.codeword_bits,
                u64::from(run) * length,
                MAX_CODEWORD_BITS,
            )?;
            current = current.saturating_add(run);
            if current > entries {
                return Err(Stop::Unreadable);
            }
            length += 1;
        }
    } else {
        let sparse = bits.read(1)? == 1;
        for _ in 0..entries {
            if !sparse || bits.read(1)? == 1 {
                let length = u64::from(bits.read(5)?) + 1;
                Budget::spend(&mut budget.codeword_bits, length, MAX_CODEWORD_BITS)?;
            }
        }
    }
    let lookup = bits.read(4)?;
    if lookup > 2 {
        return Err(Stop::Unreadable);
    }
    if lookup > 0 {
        bits.skip(64)?; // minimum and delta values
        let value_bits = u64::from(bits.read(4)?) + 1;
        bits.skip(1)?; // sequence flag
        let values = if lookup == 1 {
            u64::from(lookup1_values(entries, dimensions))
        } else {
            u64::from(entries) * dimensions
        };
        // lewton reserves every value before it reads one: they must all be
        // in the packet.
        let needed = values.saturating_mul(value_bits);
        if needed > bits.remaining() {
            return Err(Stop::Unbounded);
        }
        Budget::spend(
            &mut budget.vector_values,
            u64::from(entries) * dimensions,
            MAX_VECTOR_VALUES,
        )?;
        bits.skip(needed)?;
    }
    Ok(())
}

/// The bits needed to write `x` (Vorbis I, 9.2.1) - as lewton's `ilog`.
fn ilog(x: u32) -> u32 {
    32 - x.leading_zeros()
}

/// The greatest `r` with `r^dimensions <= entries` (Vorbis I, 9.2.3): the
/// values a lookup type 1 codebook holds. With no dimensions every `r`
/// qualifies, and lewton answers `u32::MAX`, as here.
fn lookup1_values(entries: u32, dimensions: u64) -> u32 {
    if dimensions == 0 {
        return if entries == 0 { 0 } else { u32::MAX };
    }
    let fits = |r: u64| {
        let mut power = 1u64;
        // Past 64 doublings any base over 1 has outgrown a 24-bit count.
        for _ in 0..dimensions.min(64) {
            power = power.saturating_mul(r);
            if power > u64::from(entries) {
                return false;
            }
        }
        true
    };
    // 0 fits and entries + 1 does not, with any dimensions.
    let (mut lo, mut hi) = (0u64, u64::from(entries) + 1);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo as u32
}

/// Vorbis' bit packing (Vorbis I, 2.1.4): each byte fills from its least
/// significant bit, and a field's first bit is its least significant.
struct Bits<'a> {
    bytes: &'a [u8],
    /// The next bit to read.
    at: u64,
}

impl Bits<'_> {
    fn remaining(&self) -> u64 {
        (self.bytes.len() as u64 * 8).saturating_sub(self.at)
    }

    /// The next `n` (at most 32) bits as a number.
    fn read(&mut self, n: u32) -> Result<u32, Stop> {
        if u64::from(n) > self.remaining() {
            return Err(Stop::Unreadable);
        }
        let mut value = 0u32;
        for i in 0..n {
            let byte = self.bytes[(self.at / 8) as usize];
            value |= u32::from((byte >> (self.at % 8)) & 1) << i;
            self.at += 1;
        }
        Ok(value)
    }

    fn skip(&mut self, n: u64) -> Result<(), Stop> {
        if n > self.remaining() {
            return Err(Stop::Unreadable);
        }
        self.at += n;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
