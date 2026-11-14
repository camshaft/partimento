//! SHA-256, as FIPS 180-4 defines it, for the keys of the artifact cache and the digest that ends
//! each artifact's encoding.

/// The round constants: the first 32 bits of the fractional parts of the cube roots of the first
/// 64 primes.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// The first hash value: the first 32 bits of the fractional parts of the square roots of the
/// first 8 primes.
const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// The digest of `bytes`.
pub(crate) fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::default();
    hasher.update(bytes);
    hasher.finish()
}

/// A digest that takes its input in parts.
pub(crate) struct Sha256 {
    state: [u32; 8],
    /// The input since the last block that was compressed.
    block: [u8; 64],
    /// The bytes of `block` that the input fills.
    filled: usize,
    /// The bytes of the whole input.
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Sha256 {
            state: INITIAL,
            block: [0; 64],
            filled: 0,
            length: 0,
        }
    }
}

impl Sha256 {
    /// Adds `bytes` to the input.
    pub(crate) fn update(&mut self, mut bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        while !bytes.is_empty() {
            let room = self.block.len() - self.filled;
            let (part, rest) = bytes.split_at(room.min(bytes.len()));
            let (start, end) = (self.filled, self.filled + part.len());
            self.block[start..end].copy_from_slice(part);
            self.filled = end;
            bytes = rest;
            if self.filled == self.block.len() {
                compress(&mut self.state, &self.block);
                self.filled = 0;
            }
        }
    }

    /// The digest of the input: the input is padded with a 1 bit, 0 bits up to 8 bytes before the
    /// end of a block, and the input's length in bits.
    pub(crate) fn finish(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        self.update(&[0x80]);
        while self.filled != 56 {
            self.update(&[0]);
        }
        self.update(&bits.to_be_bytes());
        let mut output = [0; 32];
        for (chunk, word) in output.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        output
    }
}

/// Runs the compression function over one block.
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut schedule = [0u32; 64];
    for (word, bytes) in schedule.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    for t in 16..64 {
        let (w15, w2) = (schedule[t - 15], schedule[t - 2]);
        let s0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
        let s1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
        schedule[t] = schedule[t - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[t - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for (k, w) in K.into_iter().zip(schedule) {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(choice)
            .wrapping_add(k)
            .wrapping_add(w);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (word, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *word = word.wrapping_add(value);
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::{digest, Sha256};

    fn hex(bytes: &[u8]) -> String {
        let mut text = String::new();
        for byte in bytes {
            let _ = write!(text, "{byte:02x}");
        }
        text
    }

    /// Asserts that the digest of `input`, in hex, is `expected`.
    fn digests(input: &[u8], expected: &str) {
        assert_eq!(hex(&digest(input)), expected);
    }

    #[test]
    fn the_digests_of_the_standards_examples() {
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        digests(b"", empty);
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        digests(b"abc", abc);
        let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let two_blocks = "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1";
        digests(input, two_blocks);
        let million = vec![b'a'; 1_000_000];
        let expected = "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0";
        digests(&million, expected);
    }

    #[test]
    fn an_input_in_parts_has_the_digest_of_the_whole() {
        let input: Vec<u8> = (0..=u8::MAX).cycle().take(1000).collect();
        let whole = digest(&input);
        for split in [0, 1, 55, 56, 63, 64, 65, 128, 999, 1000] {
            let (first, second) = input.split_at(split);
            let mut hasher = Sha256::default();
            hasher.update(first);
            hasher.update(second);
            assert_eq!(hasher.finish(), whole, "{split}");
        }
    }
}
