use crate::codecs::{Finish, ReadState, State, WriteState};
use rustc_hash::FxHashMap;
use std::io::{self, Read, Write};

// The `.Z` format has no external crate dependency: it's the frozen Unix
// `compress` LZW format, implemented natively below. The stream is a
// 3-byte header followed by LZW codes packed LSB-first, starting at 9
// bits per code and growing to `max_bits`. Two quirks are load-bearing
// and mirrored exactly from the reference implementation (compress 4.2 /
// ncompress, which gzip's unlzw also follows):
//
// - Codes are written in groups of 8 (= `n_bits` bytes). Whenever the
//   code width changes or the table is cleared, the encoder pads the
//   partial group to its full `n_bits` bytes, and the decoder skips the
//   same padding.
// - After a clear, the decoder assigns a throwaway table entry to code
//   256 (the clear code itself), which keeps its entry counter in
//   lockstep with the encoder's.

const MAGIC: [u8; 2] = [0x1F, 0x9D];
/// Header flag bit marking a stream that reserves code 256 as the
/// "clear table" code. Everything written since 1985 sets it.
const BLOCK_MODE: u8 = 0x80;
const MAX_BITS_MASK: u8 = 0x1F;

/// All streams start at 9 bits per code.
const INIT_BITS: u32 = 9;
/// The widest code we write (`compress -b 16`, its default and maximum).
const MAX_BITS: u32 = 16;

/// In block mode, code 256 clears the string table.
const CLEAR: u32 = 256;
/// In block mode, the first available string code.
const FIRST: u32 = 257;

/// The widen threshold for a code width: once the next free entry
/// exceeds it, codes grow by one bit. At the final width it's a
/// sentinel one past the largest code, so it never triggers again.
/// Mirrors the reference implementation exactly, including the `-b 9`
/// oddity where the initial threshold is *not* the sentinel, making
/// such streams widen to 10-bit codes despite a 9-bit table.
fn widen_threshold(n_bits: u32, max_bits: u32) -> u32 {
    if n_bits == max_bits {
        1 << n_bits
    } else {
        (1 << n_bits) - 1
    }
}

fn corrupt(message: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid .Z stream: {message}"),
    )
}

/// A Unix `compress` (LZW) codec that wraps another stream, aka `.Z`.
/// The first read commits it to decompressing, while the first write
/// commits it to compressing.
///
/// The format has no compression levels. Writing produces a block-mode
/// stream with 16-bit codes (what `compress` produces by default), and
/// reading accepts any valid stream, including non-block-mode and
/// narrower code widths from `compress -b 9` through `-b 16`.
pub struct Z<T> {
    state: State<T>,
}

impl<T> Z<T> {
    /// Create a new codec.
    pub fn new(inner: T) -> Self {
        Z {
            state: State::Pending(inner),
        }
    }

    /// Consume the codec and return the wrapped stream. If the codec was
    /// compressing, the final code and any buffered bytes are written first.
    pub fn into_inner(self) -> io::Result<T> {
        self.state.into_inner()
    }
}

impl<T: Read + 'static> ReadState<T> for ZDecoder<T> {
    fn into_inner(self: Box<Self>) -> T {
        self.inner
    }
}

impl<T: Read + 'static> Read for Z<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.state
            .read(buf, |inner| Ok(Box::new(ZDecoder::new(inner))))
    }
}

impl<T: Write + 'static> WriteState<T> for ZEncoder<T> {
    fn finish(mut self: Box<Self>) -> io::Result<T> {
        self.do_finish()?;

        Ok(self.inner.take().unwrap())
    }
}

impl<T: Write + 'static> Write for Z<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.state
            .write(buf, |inner| Ok(Box::new(ZEncoder::new(inner))))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state.flush()
    }
}

impl<T: Finish + 'static> Finish for Z<T> {
    fn finish(&mut self) -> io::Result<()> {
        self.state
            .finish(|inner| Ok(Box::new(ZEncoder::new(inner))))
    }
}

/// Streaming `.Z` decompressor.
struct ZDecoder<T> {
    inner: T,

    // Raw input buffering
    in_buf: Vec<u8>,
    in_pos: usize,
    in_len: usize,
    in_eof: bool,

    // Bit reservoir, LSB-first
    bit_buf: u64,
    bit_len: u32,

    // Stream parameters, set once the header is parsed
    started: bool,
    block_mode: bool,
    max_bits: u32,

    // LZW state
    n_bits: u32,
    max_code: u32,
    free: u32,
    prefixes: Vec<u16>,
    suffixes: Vec<u8>,
    old_code: Option<u16>,
    last_char: u8,

    /// Bits consumed since the last width change or clear, used to skip
    /// the code-group padding.
    epoch_bits: u64,

    // Decoded bytes not yet handed to the caller
    pending: Vec<u8>,
    pending_pos: usize,

    done: bool,
    failed: Option<(io::ErrorKind, String)>,
}

impl<T: Read> ZDecoder<T> {
    fn new(inner: T) -> Self {
        ZDecoder {
            inner,
            in_buf: vec![0; 8192],
            in_pos: 0,
            in_len: 0,
            in_eof: false,
            bit_buf: 0,
            bit_len: 0,
            started: false,
            block_mode: false,
            max_bits: 0,
            n_bits: INIT_BITS,
            max_code: 0,
            free: 0,
            prefixes: vec![],
            suffixes: vec![],
            old_code: None,
            last_char: 0,
            epoch_bits: 0,
            pending: vec![],
            pending_pos: 0,
            done: false,
            failed: None,
        }
    }

    /// Top up the bit reservoir to at least `want` bits. Returns false
    /// if the input ends first, leaving any remainder in the reservoir.
    fn fill_bits(&mut self, want: u32) -> io::Result<bool> {
        while self.bit_len < want {
            if self.in_pos == self.in_len {
                if self.in_eof {
                    return Ok(false);
                }

                match self.inner.read(&mut self.in_buf) {
                    Ok(0) => {
                        self.in_eof = true;
                        return Ok(false);
                    }
                    Ok(len) => {
                        self.in_pos = 0;
                        self.in_len = len;
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }

            self.bit_buf |= (self.in_buf[self.in_pos] as u64) << self.bit_len;
            self.in_pos += 1;
            self.bit_len += 8;
        }

        Ok(true)
    }

    fn parse_header(&mut self) -> io::Result<()> {
        if !self.fill_bits(24)? {
            return Err(corrupt("missing or truncated header"));
        }

        let bytes = [
            self.bit_buf as u8,
            (self.bit_buf >> 8) as u8,
            (self.bit_buf >> 16) as u8,
        ];

        self.bit_buf >>= 24;
        self.bit_len -= 24;

        if bytes[0..2] != MAGIC {
            return Err(corrupt("bad magic number"));
        }

        self.block_mode = bytes[2] & BLOCK_MODE != 0;
        self.max_bits = (bytes[2] & MAX_BITS_MASK) as u32;

        if !(INIT_BITS..=MAX_BITS).contains(&self.max_bits) {
            return Err(corrupt(&format!(
                "unsupported maximum code width {}",
                self.max_bits
            )));
        }

        let table_size = 1 << self.max_bits;

        self.prefixes = vec![0; table_size];
        self.suffixes = vec![0; table_size];

        self.n_bits = INIT_BITS;
        self.max_code = (1 << INIT_BITS) - 1;
        self.free = if self.block_mode { FIRST } else { CLEAR };
        self.started = true;

        Ok(())
    }

    /// Read the next code from the stream, or `None` at end of input.
    /// A final partial code (zero padding) is discarded, matching the
    /// reference implementation; truncation is not detectable.
    fn read_code(&mut self) -> io::Result<Option<u32>> {
        if !self.fill_bits(self.n_bits)? {
            return Ok(None);
        }

        let code = (self.bit_buf & ((1 << self.n_bits) - 1)) as u32;

        self.bit_buf >>= self.n_bits;
        self.bit_len -= self.n_bits;
        self.epoch_bits += self.n_bits as u64;

        Ok(Some(code))
    }

    /// Skip to the next group boundary at the current code width, then
    /// start a new epoch. Ending mid-padding is treated as end of input.
    fn skip_epoch_padding(&mut self) -> io::Result<()> {
        let group_bits = (self.n_bits as u64) * 8;
        let mut skip = (group_bits - self.epoch_bits % group_bits) % group_bits;

        while skip > 0 {
            if self.bit_len == 0 && !self.fill_bits(1)? {
                break;
            }

            let chunk = skip.min(self.bit_len as u64) as u32;

            self.bit_buf >>= chunk;
            self.bit_len -= chunk;
            skip -= chunk as u64;
        }

        self.epoch_bits = 0;

        Ok(())
    }

    /// Decode the next code, refilling `pending` with its bytes. Sets
    /// `done` at end of input. Table clears produce no output.
    fn decode_next(&mut self) -> io::Result<()> {
        if self.free > self.max_code {
            self.skip_epoch_padding()?;
            self.n_bits += 1;
            self.max_code = widen_threshold(self.n_bits, self.max_bits);
        }

        let Some(code) = self.read_code()? else {
            self.done = true;
            return Ok(());
        };

        if self.block_mode && code == CLEAR {
            self.skip_epoch_padding()?;
            self.n_bits = INIT_BITS;
            self.max_code = (1 << INIT_BITS) - 1;
            self.free = FIRST - 1;
            return Ok(());
        }

        self.pending.clear();
        self.pending_pos = 0;

        let Some(old_code) = self.old_code else {
            // The very first code can only be a literal
            if code > 255 {
                return Err(corrupt("first code is not a literal"));
            }

            self.last_char = code as u8;
            self.old_code = Some(code as u16);
            self.pending.push(code as u8);

            return Ok(());
        };

        let in_code = code;

        // The KwKwK special case: the code for the string currently
        // being defined, which ends with the previous string's first byte
        let mut walk = if code >= self.free {
            if code > self.free {
                return Err(corrupt("code exceeds string table"));
            }

            self.pending.push(self.last_char);

            old_code as u32
        } else {
            code
        };

        // Walk the prefix chain, emitting the string back to front.
        // Every table entry points to a smaller code, so this terminates.
        while walk >= 256 {
            self.pending.push(self.suffixes[walk as usize]);
            walk = self.prefixes[walk as usize] as u32;
        }

        self.last_char = walk as u8;
        self.pending.push(self.last_char);
        self.pending.reverse();

        // Define the next entry: the previous string plus this one's
        // first byte. After a clear this writes a throwaway entry for
        // code 256, which realigns `free` with the encoder.
        if self.free < (1 << self.max_bits) {
            self.prefixes[self.free as usize] = old_code;
            self.suffixes[self.free as usize] = self.last_char;
            self.free += 1;
        }

        self.old_code = Some(in_code as u16);

        Ok(())
    }
}

impl<T: Read> Read for ZDecoder<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        if !self.started {
            self.parse_header()?;
        }

        let mut written = 0;

        loop {
            if self.pending_pos < self.pending.len() {
                let available = &self.pending[self.pending_pos..];
                let count = available.len().min(buf.len() - written);

                buf[written..written + count].copy_from_slice(&available[..count]);
                written += count;
                self.pending_pos += count;

                if written == buf.len() {
                    return Ok(written);
                }
            }

            if self.done {
                return Ok(written);
            }

            // Report a failure only once all decoded bytes are drained
            if let Some((kind, message)) = &self.failed {
                return if written > 0 {
                    Ok(written)
                } else {
                    Err(io::Error::new(*kind, message.clone()))
                };
            }

            if let Err(error) = self.decode_next() {
                self.failed = Some((error.kind(), error.to_string()));

                if written > 0 {
                    return Ok(written);
                }

                return Err(error);
            }
        }
    }
}

/// Streaming `.Z` compressor. Always produces a block-mode stream with
/// 16-bit codes, and clears the table whenever it fills. Finishing (or
/// dropping) writes the final code and any buffered bytes.
struct ZEncoder<T: Write> {
    /// Taken by `finish`; `Drop` completes the stream if still present.
    inner: Option<T>,

    /// String table: `(prefix code << 8) | next byte` to code.
    table: FxHashMap<u32, u16>,

    /// The code for the string matched so far.
    prefix: Option<u16>,

    // LZW state
    free: u32,
    n_bits: u32,
    max_code: u32,

    // Bit reservoir, LSB-first
    bit_buf: u64,
    bit_len: u32,

    /// Bits emitted since the last width change or clear, used to pad
    /// partial code groups.
    epoch_bits: u64,

    /// Bytes not yet written to the wrapped stream. Starts with the
    /// header so that construction does no I/O.
    out: Vec<u8>,

    finished: bool,
}

impl<T: Write> ZEncoder<T> {
    fn new(inner: T) -> Self {
        ZEncoder {
            inner: Some(inner),
            table: FxHashMap::default(),
            prefix: None,
            free: FIRST,
            n_bits: INIT_BITS,
            max_code: widen_threshold(INIT_BITS, MAX_BITS),
            bit_buf: 0,
            bit_len: 0,
            epoch_bits: 0,
            out: vec![MAGIC[0], MAGIC[1], BLOCK_MODE | MAX_BITS as u8],
            finished: false,
        }
    }

    fn put_code(&mut self, code: u32) {
        self.bit_buf |= (code as u64) << self.bit_len;
        self.bit_len += self.n_bits;
        self.epoch_bits += self.n_bits as u64;

        while self.bit_len >= 8 {
            self.out.push(self.bit_buf as u8);
            self.bit_buf >>= 8;
            self.bit_len -= 8;
        }
    }

    /// Pad the current code group to its full `n_bits` bytes, as the
    /// reference implementation does on every width change and clear.
    fn pad_epoch(&mut self) {
        let group_bits = (self.n_bits as u64) * 8;
        let mut pad = (group_bits - self.epoch_bits % group_bits) % group_bits;

        if pad > 0 && self.bit_len > 0 {
            // Complete the partial byte; its unused high bits are zero
            pad -= (8 - self.bit_len) as u64;
            self.out.push(self.bit_buf as u8);
            self.bit_buf = 0;
            self.bit_len = 0;
        }

        for _ in 0..pad / 8 {
            self.out.push(0);
        }

        self.epoch_bits = 0;
    }

    fn step(&mut self, byte: u8) {
        let Some(prefix) = self.prefix else {
            self.prefix = Some(byte as u16);
            return;
        };

        let key = ((prefix as u32) << 8) | byte as u32;

        if let Some(&code) = self.table.get(&key) {
            self.prefix = Some(code);
            return;
        }

        self.put_code(prefix as u32);

        // Widen using the entry count from *before* this step's define,
        // mirroring the reference; the decoder checks before each read
        if self.free > self.max_code {
            self.pad_epoch();
            self.n_bits += 1;
            self.max_code = widen_threshold(self.n_bits, MAX_BITS);
        }

        if self.free < (1 << MAX_BITS) {
            self.table.insert(key, self.free as u16);
            self.free += 1;
        } else {
            // Table is full: clear and start over. The reference only
            // clears once the ratio degrades, but an unconditional
            // clear is valid and keeps this simple.
            self.put_code(CLEAR);
            self.pad_epoch();
            self.n_bits = INIT_BITS;
            self.max_code = widen_threshold(INIT_BITS, MAX_BITS);
            self.table.clear();
            self.free = FIRST;
        }

        self.prefix = Some(byte as u16);
    }

    fn flush_out(&mut self) -> io::Result<()> {
        if !self.out.is_empty() {
            self.inner
                .as_mut()
                .expect("encoder already finished")
                .write_all(&self.out)?;
            self.out.clear();
        }

        Ok(())
    }

    fn do_finish(&mut self) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }

        self.finished = true;

        if let Some(prefix) = self.prefix.take() {
            self.put_code(prefix as u32);
        }

        // The final partial byte is written as-is, without group padding
        if self.bit_len > 0 {
            self.out.push(self.bit_buf as u8);
            self.bit_buf = 0;
            self.bit_len = 0;
        }

        self.flush_out()
    }
}

impl<T: Write> Write for ZEncoder<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Write out buffered bytes first, so an inner failure surfaces
        // before any of `buf` is consumed, then compress a bounded chunk
        // purely in memory
        self.flush_out()?;

        let chunk = buf.len().min(4096);

        for byte in &buf[..chunk] {
            self.step(*byte);
        }

        Ok(chunk)
    }

    fn flush(&mut self) -> io::Result<()> {
        // The code for the string being matched can't be emitted early
        // without corrupting the stream, so only whole bytes are flushed
        self.flush_out()?;
        self.inner
            .as_mut()
            .expect("encoder already finished")
            .flush()
    }
}

impl<T: Write> Drop for ZEncoder<T> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            let _ = self.do_finish();
        }
    }
}
