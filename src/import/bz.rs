//! Reads bzip2 on every core.
//!
//! The dumps lyrid is built from are bzip2: Wikidata's 103 GB, the MusicBrainz
//! export, the Wikipedia index. bzip2 decompresses at a few megabytes a second
//! per core, and a reader on one core spends most of a pass waiting for it:
//! measured on the Wikidata dump, 26.6 s of decompression against 9.5 s of
//! parsing the same lines.
//!
//! bzip2 compresses its input in **blocks** of up to 900 kB, each on its own:
//! the run-length coding, the Huffman tables and the Burrows-Wheeler transform
//! all start over at every block, and each block carries the CRC of what it
//! holds. So blocks can be decompressed in any order, on any core, and put back
//! in order afterwards.
//!
//! What makes this more than a loop: a block is found by its 48-bit signature
//! (the digits of pi, `0x314159265359`), and the signature is not on a byte
//! boundary -- blocks are packed bit after bit. Splitting on *streams* would be
//! simpler, and a multistream file like Wikipedia's articles would allow it,
//! but the Wikidata dump is a single stream: only its blocks divide it.
//!
//! The parts:
//!
//! - a **splitter** reads the compressed bytes in order, finds each signature
//!   at whatever bit it sits, and cuts the input into segments, one per block;
//! - **workers** turn each segment into a one-block bzip2 stream of its own --
//!   a header, the block shifted onto a byte boundary, an end-of-stream marker
//!   carrying the block's CRC -- and let libbzip2 decompress it. The format's
//!   reference decoder does the hard part and checks the CRC;
//! - the **reader** puts the blocks back in order and checks each stream's
//!   combined CRC against the one written at its end, so a block lost or read
//!   twice is an error rather than a silent gap in a canon.
//!
//! A signature can also occur by chance inside a block's data, once in 2^48
//! bit positions. A segment cut at such a false signature fails its CRC; the
//! reader joins it to the segment after it and decodes the two as one.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, Read};
use std::num::NonZero;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use bzip2::read::BzDecoder;

/// The signature that opens every block: the digits of pi.
const BLOCK_MAGIC: u64 = 0x3141_5926_5359;

/// The signature that closes a stream: the digits of the square root of pi.
const END_MAGIC: u64 = 0x1772_4538_5090;

/// How much compressed input the splitter asks for at a time.
const READ_SIZE: usize = 1 << 20;

/// A block compresses to its 900 kB and a little more at worst. A segment far
/// past that is input that is not bzip2, and following it would hold the whole
/// file in memory before saying so.
const MAX_SEGMENT: u64 = 64 << 20;

/// How many segments one block may have been cut into before it is called
/// corrupt. A single false signature is already a 2^-48 event.
const MAX_MEND: usize = 8;

/// For each byte value, the shifts at which a signature starting in the byte
/// before would put that value here: bits 0-7 for the block signature, 8-15
/// for the end-of-stream one. Most bytes are no signature's second byte, which
/// is what lets the splitter skip them without reading 48 bits at every bit.
const SECOND_BYTES: [u16; 256] = second_bytes();

const fn second_bytes() -> [u16; 256] {
    let mut table = [0u16; 256];
    let mut shift = 0;
    while shift < 8 {
        // A signature at bit `shift` of byte i covers bits 8 - shift .. 16 - shift
        // of itself with byte i + 1.
        table[((BLOCK_MAGIC >> (32 + shift)) & 0xFF) as usize] |= 1 << shift;
        table[((END_MAGIC >> (32 + shift)) & 0xFF) as usize] |= 1 << (8 + shift);
        shift += 1;
    }
    table
}

/// Opens a bzip2 file for reading on every core.
pub fn open(path: &Path) -> Result<ParallelDecoder> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    Ok(ParallelDecoder::new(file))
}

/// `bits` bits (at most 57) starting `at` bits into `bytes`, most significant
/// first, as bzip2 packs them. Bits past the end read as zero.
fn bits_at(bytes: &[u8], at: u64, bits: u32) -> u64 {
    debug_assert!((1..=57).contains(&bits));
    let mut window = [0u8; 8];
    if let Some(rest) = usize::try_from(at / 8).ok().and_then(|first| bytes.get(first..)) {
        let len = rest.len().min(8);
        window[..len].copy_from_slice(&rest[..len]);
    }
    (u64::from_be_bytes(window) << (at % 8)) >> (64 - bits)
}

/// Packs bits most significant first, the way bzip2 lays them out.
struct BitWriter {
    bytes: Vec<u8>,
    /// Bits not yet making a whole byte, in the low `filled` bits.
    pending: u64,
    filled: u32,
}

impl BitWriter {
    fn with_capacity(bytes: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(bytes),
            pending: 0,
            filled: 0,
        }
    }

    /// Appends the low `bits` bits of `value`, at most 56 at a time.
    fn push(&mut self, value: u64, bits: u32) {
        debug_assert!(bits <= 56);
        if bits == 0 {
            return;
        }
        self.pending = (self.pending << bits) | (value & ((1 << bits) - 1));
        self.filled += bits;
        while self.filled >= 8 {
            self.filled -= 8;
            self.bytes.push((self.pending >> self.filled).to_be_bytes()[7]);
        }
        self.pending &= (1 << self.filled) - 1;
    }

    /// Appends `len` bits of `src`, starting `skip` bits into it.
    fn push_run(&mut self, src: &[u8], skip: u8, len: u64) {
        let mut at = u64::from(skip);
        let end = at + len;
        while at < end {
            let take = u32::try_from((end - at).min(56)).unwrap_or(56);
            self.push(bits_at(src, at, take), take);
            at += u64::from(take);
        }
    }

    /// The bytes written, the last one padded with zero bits.
    fn finish(mut self) -> Vec<u8> {
        if self.filled > 0 {
            self.push(0, 8 - self.filled);
        }
        self.bytes
    }
}

/// One block as cut out of the input: the bits from its signature up to the
/// next signature.
#[derive(Clone)]
struct Segment {
    /// The bytes holding those bits. The first and the last may also hold bits
    /// of the neighbours.
    bytes: Vec<u8>,
    /// Where the block starts within the first byte.
    skip: u8,
    /// Its length in bits.
    len: u64,
    /// Where it starts in the input, in bits: for the message when it is bad.
    at: u64,
    /// The block-size digit of its stream's header, `b'1'..=b'9'`.
    level: u8,
}

impl Segment {
    /// The CRC the block declares for its contents: the 32 bits after its
    /// signature.
    fn crc(&self) -> u32 {
        u32::try_from(bits_at(&self.bytes, u64::from(self.skip) + 48, 32)).expect("32 bits fit a u32")
    }

    /// This segment followed by the one cut off it at a false signature. The
    /// two meet inside a shared byte, which the second one holds whole.
    fn join(mut self, next: &Segment) -> Segment {
        let end = u64::from(self.skip) + self.len;
        debug_assert_eq!(u64::from(next.skip), end % 8, "segments join only where one ends and the next begins");
        self.bytes.truncate(usize::try_from(end / 8).expect("a segment fits in memory"));
        self.bytes.extend_from_slice(&next.bytes);
        self.len += next.len;
        self
    }

    /// Decompresses the block, returning its bytes and its CRC.
    ///
    /// The block is wrapped into a stream of its own, so libbzip2 checks it
    /// the way it checks any stream: the block's CRC against its contents, and
    /// the stream's CRC -- here the block's own -- at the end.
    fn decode(&self) -> io::Result<(Vec<u8>, u32)> {
        let crc = self.crc();
        let mut stream = BitWriter::with_capacity(self.bytes.len() + 16);
        stream.push(u64::from(u32::from_be_bytes([b'B', b'Z', b'h', self.level])), 32);
        stream.push_run(&self.bytes, self.skip, self.len);
        stream.push(END_MAGIC, 48);
        stream.push(u64::from(crc), 32);
        let stream = stream.finish();

        let mut out = Vec::with_capacity(usize::from(self.level - b'0') * 100_000);
        BzDecoder::new(stream.as_slice()).read_to_end(&mut out)?;
        Ok((out, crc))
    }
}

/// What travels from the splitter, through the workers, to the reader, in the
/// order of the input.
enum Piece {
    /// A block to decompress. When the reader receives one, the worker could
    /// not decompress it alone.
    Block(Segment),
    /// A block decompressed, with its CRC.
    Data(Vec<u8>, u32),
    /// The end of a stream, with the combined CRC it declares.
    End(u32),
    /// The input stopped making sense here.
    Fault(String),
    /// The input is over.
    Finish,
}

/// Where the splitter is in the input.
#[derive(Clone, Copy)]
enum State {
    /// A stream header is expected at this byte -- or the end of the input.
    Header(u64),
    /// Inside a stream: the current block starts at bit `start`, and the next
    /// signature is looked for from byte `scan` on.
    Blocks {
        start: u64,
        scan: u64,
        level: u8,
    },
    Done,
}

/// Cuts compressed input into blocks and stream ends, in order.
struct Splitter<R> {
    source: R,
    /// Input not yet handed on: `buf[0]` is input byte `base`.
    buf: Vec<u8>,
    base: u64,
    eof: bool,
    state: State,
    /// A stream's end, found together with its last block.
    queued: Option<Piece>,
}

impl<R: Read> Splitter<R> {
    fn new(source: R) -> Self {
        Self {
            source,
            buf: Vec::with_capacity(2 * READ_SIZE),
            base: 0,
            eof: false,
            state: State::Header(0),
            queued: None,
        }
    }

    /// One past the last input byte read so far.
    fn end(&self) -> u64 {
        self.base + self.buf.len() as u64
    }

    fn index(&self, byte: u64) -> usize {
        usize::try_from(byte - self.base).expect("the buffer fits in memory")
    }

    fn bits(&self, at: u64, bits: u32) -> u64 {
        bits_at(&self.buf, at - self.base * 8, bits)
    }

    /// Reads until byte `byte` is in the buffer or the input ends.
    fn fill_to(&mut self, byte: u64) -> Result<(), String> {
        while self.end() < byte && !self.eof {
            self.compact();
            let len = self.buf.len();
            self.buf.resize(len + READ_SIZE, 0);
            match self.source.read(&mut self.buf[len..]) {
                Ok(0) => {
                    self.buf.truncate(len);
                    self.eof = true;
                }
                Ok(read) => self.buf.truncate(len + read),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => self.buf.truncate(len),
                Err(error) => {
                    self.buf.truncate(len);
                    return Err(format!("cannot read the compressed input: {error}"));
                }
            }
        }
        Ok(())
    }

    /// Drops the input already handed on, once it is most of the buffer:
    /// dropping it on every read would move the same bytes again and again.
    fn compact(&mut self) {
        let keep = match self.state {
            State::Header(at) => at,
            State::Blocks { start, .. } => start / 8,
            State::Done => self.end(),
        };
        let done = self.index(keep);
        if done > 0 && done >= self.buf.len() / 2 {
            self.buf.drain(..done);
            self.base = keep;
        }
    }

    /// The bits from `start` to `end` as a segment of their own.
    fn cut(&self, start: u64, end: u64, level: u8) -> Segment {
        Segment {
            bytes: self.buf[self.index(start / 8)..self.index(end.div_ceil(8))].to_vec(),
            skip: u8::try_from(start % 8).expect("a bit within a byte"),
            len: end - start,
            at: start,
            level,
        }
    }

    /// Whether a stream header begins at byte `at`, followed by a block or by
    /// the end of an empty stream. Needs ten bytes from `at` in the buffer.
    fn header_at(&self, at: u64) -> Option<u8> {
        let header = self.buf.get(self.index(at)..self.index(at) + 4)?;
        let level = header[3];
        let first = self.bits((at + 4) * 8, 48);
        (header[..3] == *b"BZh" && (b'1'..=b'9').contains(&level) && (first == BLOCK_MAGIC || first == END_MAGIC)).then_some(level)
    }

    /// Where the next stream would begin if an end-of-stream signature really
    /// sits at bit `end`, or `None` when what follows says it is a chance
    /// pattern inside a block. A real end is followed by its CRC, padding to
    /// the byte, and then either nothing or another stream.
    fn stream_ends_at(&mut self, end: u64) -> Result<Option<u64>, String> {
        let next = (end + 80).div_ceil(8);
        self.fill_to(next + 10)?;
        if self.eof && self.end() == next {
            return Ok(Some(next));
        }
        if self.end() >= next + 10 && self.header_at(next).is_some() {
            return Ok(Some(next));
        }
        Ok(None)
    }

    fn step(&mut self) -> Result<Option<Piece>, String> {
        loop {
            match self.state {
                State::Done => return Ok(None),
                State::Header(at) => {
                    self.fill_to(at + 10)?;
                    if self.eof && self.end() == at {
                        self.state = State::Done;
                        return Ok(None);
                    }
                    if self.end() < at + 10 {
                        return Err(format!("the input ends inside a bzip2 stream header at byte {at}"));
                    }
                    let Some(level) = self.header_at(at) else {
                        return Err(format!("no bzip2 stream at byte {at} of the input"));
                    };
                    let first = (at + 4) * 8;
                    if self.bits(first, 48) == END_MAGIC {
                        // An empty stream: no blocks, only its end.
                        self.fill_to(at + 14)?;
                        if self.end() < at + 14 {
                            return Err(format!("the input ends inside a bzip2 stream at byte {at}"));
                        }
                        let crc = u32::try_from(self.bits(first + 48, 32)).expect("32 bits fit a u32");
                        self.state = State::Header(at + 14);
                        return Ok(Some(Piece::End(crc)));
                    }
                    self.state = State::Blocks {
                        start: first,
                        scan: at + 4,
                        level,
                    };
                }
                State::Blocks { start, scan, level } => return self.next_block(start, scan, level).map(Some),
            }
        }
    }

    /// Finds where the block starting at bit `start` ends, looking from byte
    /// `scan` on, and cuts it off.
    fn next_block(&mut self, start: u64, mut scan: u64, level: u8) -> Result<Piece, String> {
        loop {
            // Every candidate needs eight bytes of input from its first one.
            if scan + 8 > self.end() && !self.eof {
                if scan - start / 8 > MAX_SEGMENT {
                    return Err(format!("no bzip2 block boundary within {MAX_SEGMENT} bytes of bit {start}: not a bzip2 stream"));
                }
                self.fill_to(scan + 8 + READ_SIZE as u64)?;
                continue;
            }
            if scan >= self.end() {
                return Err(format!("the input ends inside a bzip2 block that starts at bit {start}"));
            }

            // Skip, at memory speed, every byte that is no signature's second.
            let from = self.index(scan);
            let to = if self.eof { self.buf.len() } else { self.index(self.end() - 7) };
            let Some(hit) = self.buf[from..to].iter().position(|&byte| SECOND_BYTES[usize::from(byte)] != 0) else {
                scan = self.base + to as u64;
                continue;
            };
            // The signature would start one byte before its second.
            let candidate = (self.base + (from + hit) as u64).saturating_sub(1);
            let shifts = SECOND_BYTES[usize::from(self.buf[from + hit])];
            scan = self.base + (from + hit) as u64;

            for shift in 0..8u64 {
                let at = candidate * 8 + shift;
                if at <= start {
                    continue;
                }
                let block = shifts & (1 << shift) != 0;
                let end = shifts & (1 << (8 + shift)) != 0;
                if !block && !end {
                    continue;
                }
                let found = self.bits(at, 48);
                if block && found == BLOCK_MAGIC {
                    let segment = self.cut(start, at, level);
                    self.state = State::Blocks { start: at, scan, level };
                    return Ok(Piece::Block(segment));
                }
                if end && found == END_MAGIC {
                    let Some(next) = self.stream_ends_at(at)? else { continue };
                    let segment = self.cut(start, at, level);
                    let crc = u32::try_from(self.bits(at + 48, 32)).expect("32 bits fit a u32");
                    self.queued = Some(Piece::End(crc));
                    self.state = State::Header(next);
                    return Ok(Piece::Block(segment));
                }
            }
            scan += 1;
        }
    }
}

impl<R: Read> Iterator for Splitter<R> {
    type Item = Piece;

    fn next(&mut self) -> Option<Piece> {
        if let Some(piece) = self.queued.take() {
            return Some(piece);
        }
        match self.step() {
            Ok(piece) => piece,
            Err(message) => {
                self.state = State::Done;
                Some(Piece::Fault(message))
            }
        }
    }
}

/// A bzip2 reader that decompresses on every core and reads like any other.
///
/// It is a `BufRead` over the decompressed blocks themselves, so reading lines
/// out of it copies each byte once.
pub struct ParallelDecoder {
    results: Receiver<(u64, Piece)>,
    /// One token per piece in flight, handed back as each is consumed: the
    /// splitter waits for room rather than reading the file into memory ahead
    /// of a slow reader.
    tokens: Receiver<()>,
    /// Pieces that arrived before their turn.
    pending: BTreeMap<u64, Piece>,
    next: u64,
    block: Vec<u8>,
    pos: usize,
    /// The running CRC of the current stream, as bzip2 combines its blocks'.
    combined: u32,
    finished: bool,
    fault: Option<String>,
}

impl ParallelDecoder {
    pub fn new<R: Read + Send + 'static>(source: R) -> Self {
        Self::from_pieces(Splitter::new(source))
    }

    fn from_pieces<I: Iterator<Item = Piece> + Send + 'static>(pieces: I) -> Self {
        // One core reads lines out of this; the rest decompress.
        let workers = thread::available_parallelism().map_or(4, NonZero::get).saturating_sub(1).max(1);
        let window = workers * 4;
        let (token_tx, token_rx) = mpsc::sync_channel(window);
        let (job_tx, job_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();

        let jobs = Arc::new(Mutex::new(job_rx));
        for _ in 0..workers {
            let jobs = Arc::clone(&jobs);
            let results = result_tx.clone();
            thread::spawn(move || work(&jobs, &results));
        }
        thread::spawn(move || split(pieces, &token_tx, &job_tx));

        Self {
            results: result_rx,
            tokens: token_rx,
            pending: BTreeMap::new(),
            next: 0,
            block: Vec::new(),
            pos: 0,
            combined: 0,
            finished: false,
            fault: None,
        }
    }

    /// The piece with sequence number `seq`, waiting for the workers if needed.
    fn take(&mut self, seq: u64) -> io::Result<Piece> {
        loop {
            if let Some(piece) = self.pending.remove(&seq) {
                self.tokens.try_recv().ok();
                return Ok(piece);
            }
            let (at, piece) = self
                .results
                .recv()
                .map_err(|_| io::Error::other("the bzip2 workers stopped before the end of the input"))?;
            self.pending.insert(at, piece);
        }
    }

    /// Moves on to the next decompressed block; false at the end of the input.
    fn advance(&mut self) -> io::Result<bool> {
        loop {
            if let Some(message) = &self.fault {
                return Err(io::Error::new(io::ErrorKind::InvalidData, message.clone()));
            }
            if self.finished {
                return Ok(false);
            }
            let piece = self.take(self.next)?;
            self.next += 1;
            match piece {
                Piece::Data(bytes, crc) => {
                    self.accept(bytes, crc);
                    return Ok(true);
                }
                Piece::Block(segment) => match self.mend(segment) {
                    Ok((bytes, crc)) => {
                        self.accept(bytes, crc);
                        return Ok(true);
                    }
                    Err(message) => self.fault = Some(message),
                },
                Piece::End(declared) => {
                    let combined = std::mem::take(&mut self.combined);
                    if declared != combined {
                        self.fault = Some(format!(
                            "the blocks of a bzip2 stream do not add up to its CRC (declared {declared:08x}, read {combined:08x})"
                        ));
                    }
                }
                Piece::Fault(message) => self.fault = Some(message),
                Piece::Finish => self.finished = true,
            }
        }
    }

    fn accept(&mut self, bytes: Vec<u8>, crc: u32) {
        self.combined = self.combined.rotate_left(1) ^ crc;
        self.block = bytes;
        self.pos = 0;
    }

    /// Decodes a segment that would not decode alone, by joining the segments
    /// after it: a false signature inside a block cuts it in two, and the
    /// pieces are each other's missing half.
    fn mend(&mut self, mut segment: Segment) -> Result<(Vec<u8>, u32), String> {
        let corrupt = format!("a corrupt bzip2 block at byte {} of the input", segment.at / 8);
        for _ in 0..MAX_MEND {
            let Ok(Piece::Block(rest)) = self.take(self.next) else {
                return Err(corrupt);
            };
            self.next += 1;
            segment = segment.join(&rest);
            if let Ok(decoded) = segment.decode() {
                return Ok(decoded);
            }
        }
        Err(corrupt)
    }
}

/// The splitter's thread: numbers the pieces and hands them to the workers,
/// waiting for a token before each.
fn split(pieces: impl Iterator<Item = Piece>, tokens: &SyncSender<()>, jobs: &Sender<(u64, Piece)>) {
    for (seq, piece) in (0u64..).zip(pieces.chain(std::iter::once(Piece::Finish))) {
        if tokens.send(()).is_err() || jobs.send((seq, piece)).is_err() {
            // The reader was dropped: nobody wants the rest.
            return;
        }
    }
}

/// A worker's thread: decompresses blocks, passes everything else on.
fn work(jobs: &Mutex<Receiver<(u64, Piece)>>, results: &Sender<(u64, Piece)>) {
    loop {
        let job = match jobs.lock() {
            Ok(jobs) => jobs.recv(),
            Err(_) => return,
        };
        let Ok((seq, piece)) = job else { return };
        let piece = match piece {
            Piece::Block(segment) => match segment.decode() {
                Ok((bytes, crc)) => Piece::Data(bytes, crc),
                Err(_) => Piece::Block(segment),
            },
            other => other,
        };
        if results.send((seq, piece)).is_err() {
            return;
        }
    }
}

impl BufRead for ParallelDecoder {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        while self.pos >= self.block.len() {
            if !self.advance()? {
                return Ok(&[]);
            }
        }
        Ok(&self.block[self.pos..])
    }

    fn consume(&mut self, amount: usize) {
        self.pos = (self.pos + amount).min(self.block.len());
    }
}

impl Read for ParallelDecoder {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let len = available.len().min(buf.len());
        buf[..len].copy_from_slice(&available[..len]);
        self.consume(len);
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::Write;

    use bzip2::Compression;
    use bzip2::write::BzEncoder;

    use super::*;

    /// Text that compresses like the dumps do -- words, not noise -- and
    /// varies enough that blocks end at every kind of bit offset.
    fn text(len: usize, seed: u64) -> Vec<u8> {
        const WORDS: [&str; 12] = [
            "sky", "star", "genre", "Nirvana", "Q24826", "\"P434\"", "label", "{", "}", ",", "1991", "Aberdeen",
        ];
        let mut state = seed;
        let mut out = Vec::with_capacity(len + 16);
        while out.len() < len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            out.extend_from_slice(WORDS[usize::try_from(state % 12).unwrap()].as_bytes());
            out.push(if state.is_multiple_of(9) { b'\n' } else { b' ' });
            // A run long enough for bzip2's run-length coding now and then.
            if state.is_multiple_of(97) {
                out.extend(std::iter::repeat_n(b'a', usize::try_from(state % 300).unwrap()));
            }
        }
        out.truncate(len);
        out
    }

    fn compress(data: &[u8], level: u32) -> Vec<u8> {
        let mut encoder = BzEncoder::new(Vec::new(), Compression::new(level));
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn read_all(mut reader: impl Read) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        reader.read_to_end(&mut out)?;
        Ok(out)
    }

    fn pieces(compressed: &[u8]) -> Vec<Piece> {
        Splitter::new(io::Cursor::new(compressed.to_vec())).collect()
    }

    #[test]
    fn reads_what_a_many_block_stream_holds() {
        let data = text(3_000_000, 7);
        let compressed = compress(&data, 1);
        assert_eq!(read_all(ParallelDecoder::new(io::Cursor::new(compressed))).unwrap(), data);
    }

    #[test]
    fn finds_blocks_at_every_bit_offset() {
        // The point of the splitter is signatures off the byte boundary. With
        // only byte-aligned blocks a reader that ignored the shift would pass.
        let pieces = pieces(&compress(&text(6_000_000, 11), 1));
        let skips: HashSet<u8> = pieces
            .iter()
            .filter_map(|piece| match piece {
                Piece::Block(segment) => Some(segment.skip),
                _ => None,
            })
            .collect();
        assert!(skips.len() >= 5, "blocks started at only {} bit offsets: {skips:?}", skips.len());
    }

    #[test]
    fn reads_concatenated_streams_of_any_level_and_an_empty_one() {
        let (first, second) = (text(700_000, 1), text(1_300_000, 2));
        let mut compressed = compress(&first, 1);
        compressed.extend(compress(b"", 9));
        compressed.extend(compress(&second, 9));

        let mut expected = first;
        expected.extend(second);
        assert_eq!(read_all(ParallelDecoder::new(io::Cursor::new(compressed))).unwrap(), expected);
    }

    #[test]
    fn reads_nothing_from_nothing() {
        assert_eq!(read_all(ParallelDecoder::new(io::Cursor::new(Vec::new()))).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn mends_a_block_cut_at_a_false_signature() {
        // A signature by chance inside a block's data is a 2^-48 event, so the
        // test makes one: it cuts a real block in two where no block ends.
        let data = text(1_500_000, 3);
        let mut cut = Vec::new();
        for piece in pieces(&compress(&data, 1)) {
            match piece {
                Piece::Block(segment) if cut.len() == 4 => {
                    let half = segment.len / 2 + 3;
                    let at = u64::from(segment.skip) + half;
                    let first_bytes = usize::try_from(at.div_ceil(8)).unwrap();
                    let second_from = usize::try_from(at / 8).unwrap();
                    cut.push(Piece::Block(Segment {
                        bytes: segment.bytes[..first_bytes].to_vec(),
                        len: half,
                        ..segment.clone()
                    }));
                    cut.push(Piece::Block(Segment {
                        bytes: segment.bytes[second_from..].to_vec(),
                        skip: u8::try_from(at % 8).unwrap(),
                        len: segment.len - half,
                        at: segment.at + half,
                        level: segment.level,
                    }));
                }
                other => cut.push(other),
            }
        }
        assert_eq!(read_all(ParallelDecoder::from_pieces(cut.into_iter())).unwrap(), data);
    }

    #[test]
    fn a_lost_block_breaks_the_streams_crc() {
        // Every block on its own is fine; only the stream's CRC knows one is
        // missing. Without that check this reads as a shorter, valid file.
        let mut lost = pieces(&compress(&text(1_500_000, 5), 1));
        lost.remove(3);
        let error = read_all(ParallelDecoder::from_pieces(lost.into_iter())).unwrap_err();
        assert!(error.to_string().contains("CRC"), "{error}");
    }

    #[test]
    fn a_corrupt_byte_is_an_error_and_not_a_gap() {
        let data = text(1_500_000, 9);
        let mut compressed = compress(&data, 1);
        let middle = compressed.len() / 2;
        compressed[middle] ^= 0x10;
        assert!(read_all(ParallelDecoder::new(io::Cursor::new(compressed))).is_err());
    }

    #[test]
    fn a_truncated_stream_is_an_error() {
        let compressed = compress(&text(1_500_000, 13), 1);
        let cut = compressed[..compressed.len() * 2 / 3].to_vec();
        assert!(read_all(ParallelDecoder::new(io::Cursor::new(cut))).is_err());
    }

    #[test]
    fn input_that_is_not_bzip2_is_an_error() {
        let error = read_all(ParallelDecoder::new(io::Cursor::new(b"{\"type\":\"item\"}".repeat(100)))).unwrap_err();
        assert!(error.to_string().contains("no bzip2 stream"), "{error}");
    }

    #[test]
    fn bytes_after_the_last_stream_are_an_error() {
        let mut compressed = compress(&text(200_000, 17), 9);
        compressed.extend_from_slice(b"trailing bytes that are no stream");
        assert!(read_all(ParallelDecoder::new(io::Cursor::new(compressed))).is_err());
    }

    #[test]
    #[ignore = "reads a real dump named by LYRID_BZ2_SAMPLE"]
    fn agrees_with_libbzip2_on_a_real_dump() {
        // A dump is the real test: gigabytes from somebody else's encoder.
        // LYRID_BZ2_SAMPLE=path cargo test --release agrees_with -- --ignored
        let path = std::path::PathBuf::from(std::env::var_os("LYRID_BZ2_SAMPLE").expect("LYRID_BZ2_SAMPLE names the dump to compare"));
        let (mut serial, mut parallel) = (
            bzip2::read::MultiBzDecoder::new(io::BufReader::new(File::open(&path).unwrap())),
            open(&path).unwrap(),
        );
        let (mut a, mut b) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
        let mut total = 0u64;
        loop {
            let read = serial.read(&mut a).unwrap();
            if read == 0 {
                assert_eq!(parallel.read(&mut b).unwrap(), 0, "the parallel reader read on past the end at byte {total}");
                break;
            }
            parallel.read_exact(&mut b[..read]).unwrap();
            assert!(a[..read] == b[..read], "the readers part within bytes {total}..{}", total + read as u64);
            total += read as u64;
        }
        eprintln!("{total} bytes agree");
    }
}
