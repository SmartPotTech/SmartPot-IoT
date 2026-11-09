//! Verificación de firmas RSA (PKCS#1 v1.5 y PSS) con la operación pública
//! delegada en un multiplicador modular.
//!
//! En el firmware la multiplicación la hace el acelerador RSA del ESP32; en
//! las pruebas de host, una implementación de referencia. La exponenciación
//! se hace aquí con cuadrados y multiplicaciones (17 operaciones para
//! e = 65537), igual que el puerto de mbedTLS para el ESP32, porque la
//! exponenciación del hardware recorre todos los bits del exponente.

use sha2::{Digest, Sha256, Sha384, Sha512};

/// Palabras de 32 bits del operando más grande (RSA-4096).
pub const MAX_WORDS: usize = 128;
/// Bytes del módulo más grande.
pub const MAX_BYTES: usize = MAX_WORDS * 4;
/// El acelerador trabaja con operandos de 512 bits en 512 bits.
const WORDS_PER_STEP: usize = 16;
/// Claves más cortas no se aceptan: RSA-1024 ya no es seguro.
const MIN_MODULUS_BITS: usize = 2048;

/// Motivo por el que no se pudo verificar una firma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RsaError {
    /// Módulo de menos de 2048 o más de 4096 bits, o par.
    UnsupportedModulus,
    /// Exponente par, menor que 3 o de más de 32 bits.
    UnsupportedExponent,
    /// La firma no tiene la longitud del módulo o no es menor que él.
    SignatureOutOfRange,
}

/// Función resumen usada por la firma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hash {
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    /// Longitud del resumen en bytes.
    pub const fn output_len(self) -> usize {
        match self {
            Hash::Sha256 => 32,
            Hash::Sha384 => 48,
            Hash::Sha512 => 64,
        }
    }

    /// Resume la concatenación de `parts`; devuelve el resumen y su longitud.
    pub fn digest(self, parts: &[&[u8]]) -> ([u8; 64], usize) {
        fn run<D: Digest>(parts: &[&[u8]], out: &mut [u8; 64]) -> usize {
            let mut hasher = D::new();
            for part in parts {
                hasher.update(part);
            }
            let result = hasher.finalize();
            out[..result.len()].copy_from_slice(&result);
            result.len()
        }

        let mut out = [0u8; 64];
        let len = match self {
            Hash::Sha256 => run::<Sha256>(parts, &mut out),
            Hash::Sha384 => run::<Sha384>(parts, &mut out),
            Hash::Sha512 => run::<Sha512>(parts, &mut out),
        };
        (out, len)
    }

    /// `DigestInfo` DER que precede al resumen en PKCS#1 v1.5 (RFC 8017 §9.2).
    const fn digest_info_prefix(self) -> &'static [u8] {
        match self {
            Hash::Sha256 => &[
                0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x01, 0x05, 0x00, 0x04, 0x20,
            ],
            Hash::Sha384 => &[
                0x30, 0x41, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x02, 0x05, 0x00, 0x04, 0x30,
            ],
            Hash::Sha512 => &[
                0x30, 0x51, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x03, 0x05, 0x00, 0x04, 0x40,
            ],
        }
    }
}

/// Clave pública con los parámetros de Montgomery que pide el acelerador.
///
/// Las palabras están en orden little-endian (la 0 es la menos significativa),
/// el formato de los registros del ESP32.
#[derive(Clone)]
pub struct PreparedKey {
    words: usize,
    modulus_bits: usize,
    modulus: [u32; MAX_WORDS],
    /// `R² mod n` con `R = 2^(32·words)`.
    r_squared: [u32; MAX_WORDS],
    /// `-n⁻¹ mod 2³²`.
    m_prime: u32,
    exponent: u32,
}

impl PreparedKey {
    /// Prepara una clave a partir del módulo y el exponente en big-endian.
    pub fn new(modulus: &[u8], exponent: &[u8]) -> Result<Self, RsaError> {
        let modulus = strip_leading_zeros(modulus);
        let exponent = strip_leading_zeros(exponent);

        let modulus_bits = match modulus.first() {
            Some(&top) => modulus.len() * 8 - top.leading_zeros() as usize,
            None => 0,
        };
        let odd = modulus.last().is_some_and(|b| b & 1 == 1);
        if !(MIN_MODULUS_BITS..=MAX_WORDS * 32).contains(&modulus_bits) || !odd {
            return Err(RsaError::UnsupportedModulus);
        }

        if exponent.len() > 4 {
            return Err(RsaError::UnsupportedExponent);
        }
        let exponent = exponent
            .iter()
            .fold(0u32, |acc, &b| (acc << 8) | u32::from(b));
        if exponent < 3 || exponent & 1 == 0 {
            return Err(RsaError::UnsupportedExponent);
        }

        let words = modulus_bits.div_ceil(32 * WORDS_PER_STEP) * WORDS_PER_STEP;
        let mut key = Self {
            words,
            modulus_bits,
            modulus: [0; MAX_WORDS],
            r_squared: [0; MAX_WORDS],
            m_prime: 0,
            exponent,
        };
        words_from_be(modulus, &mut key.modulus[..words]);
        key.m_prime = m_prime(key.modulus[0]);
        r_squared(
            &key.modulus[..words],
            modulus_bits,
            &mut key.r_squared[..words],
        );
        Ok(key)
    }

    /// Tamaño del operando en palabras (múltiplo de 16).
    pub fn words(&self) -> usize {
        self.words
    }

    pub fn modulus_bits(&self) -> usize {
        self.modulus_bits
    }

    /// Bytes del módulo (y de toda firma válida).
    pub fn modulus_len(&self) -> usize {
        self.modulus_bits.div_ceil(8)
    }

    pub fn modulus(&self) -> &[u32] {
        &self.modulus[..self.words]
    }

    pub fn r_squared(&self) -> &[u32] {
        &self.r_squared[..self.words]
    }

    pub fn m_prime(&self) -> u32 {
        self.m_prime
    }
}

/// Multiplicador modular: el acelerador del ESP32 o una implementación de
/// referencia.
pub trait ModMul {
    /// Calcula `out = a · b mod n`. Los tres operandos tienen `key.words()`
    /// palabras y `a`, `b` son menores que el módulo.
    fn mod_mul(&mut self, key: &PreparedKey, a: &[u32], b: &[u32], out: &mut [u32]);
}

/// Operación pública RSAVP1 (`s^e mod n`). Escribe el mensaje codificado en
/// `encoded` (big-endian, `key.modulus_len()` bytes) y devuelve su longitud.
pub fn public_operation<M: ModMul>(
    engine: &mut M,
    key: &PreparedKey,
    signature: &[u8],
    encoded: &mut [u8; MAX_BYTES],
) -> Result<usize, RsaError> {
    let len = key.modulus_len();
    let words = key.words();
    // RFC 8017 §8.1.2/§8.2.2: la firma ocupa exactamente los bytes del módulo.
    if signature.len() != len {
        return Err(RsaError::SignatureOutOfRange);
    }
    let mut base = [0u32; MAX_WORDS];
    words_from_be(signature, &mut base[..words]);
    if !less_than(&base[..words], key.modulus()) {
        return Err(RsaError::SignatureOutOfRange);
    }

    let mut acc = base;
    let mut scratch = [0u32; MAX_WORDS];
    let top_bit = 31 - key.exponent.leading_zeros();
    for bit in (0..top_bit).rev() {
        engine.mod_mul(key, &acc[..words], &acc[..words], &mut scratch[..words]);
        reduce_once(&mut scratch[..words], key.modulus());
        acc[..words].copy_from_slice(&scratch[..words]);
        if (key.exponent >> bit) & 1 == 1 {
            engine.mod_mul(key, &acc[..words], &base[..words], &mut scratch[..words]);
            reduce_once(&mut scratch[..words], key.modulus());
            acc[..words].copy_from_slice(&scratch[..words]);
        }
    }

    // El resultado es menor que n, así que cabe en `len` bytes.
    be_from_words(&acc[..words], &mut encoded[..len]);
    Ok(len)
}

/// Comprueba la codificación EMSA-PKCS1-v1_5 (RFC 8017 §9.2) de `digest`.
pub fn pkcs1v15_matches(encoded: &[u8], hash: Hash, digest: &[u8]) -> bool {
    let prefix = hash.digest_info_prefix();
    let info_len = prefix.len() + hash.output_len();
    // 0x00 0x01, al menos 8 bytes 0xFF, 0x00 y el DigestInfo.
    if digest.len() != hash.output_len() || encoded.len() < info_len + 11 {
        return false;
    }
    let padding = encoded.len() - info_len - 3;
    let (head, info) = encoded.split_at(encoded.len() - info_len);
    head[0] == 0x00
        && head[1] == 0x01
        && head[2..2 + padding].iter().all(|&b| b == 0xff)
        && head[2 + padding] == 0x00
        && info[..prefix.len()] == *prefix
        && info[prefix.len()..] == *digest
}

/// Comprueba la codificación EMSA-PSS (RFC 8017 §9.1.2) del resumen
/// `message_hash` con MGF1 y sal de la longitud del resumen, como exige
/// TLS 1.3 (RFC 8446 §4.2.3).
pub fn pss_matches(encoded: &[u8], modulus_bits: usize, hash: Hash, message_hash: &[u8]) -> bool {
    let hash_len = hash.output_len();
    let salt_len = hash_len;
    let em_bits = modulus_bits - 1;
    let em_len = em_bits.div_ceil(8);
    if message_hash.len() != hash_len || encoded.len() < em_len {
        return false;
    }
    // Si emLen es menor que el módulo, el byte sobrante debe ser cero.
    let (extra, em) = encoded.split_at(encoded.len() - em_len);
    if extra.iter().any(|&b| b != 0) {
        return false;
    }
    if em_len < hash_len + salt_len + 2 || em[em_len - 1] != 0xbc {
        return false;
    }

    let db_len = em_len - hash_len - 1;
    let (masked_db, rest) = em.split_at(db_len);
    let h = &rest[..hash_len];
    // Bits a la izquierda que no forman parte de emBits.
    let unused_bits = 8 * em_len - em_bits;
    let top_mask = 0xffu8 >> unused_bits;
    if masked_db[0] & !top_mask != 0 {
        return false;
    }

    // DB = maskedDB XOR MGF1(H)
    let mut db = [0u8; MAX_BYTES];
    let db = &mut db[..db_len];
    db.copy_from_slice(masked_db);
    for (counter, chunk) in (0u32..).zip(db.chunks_mut(hash_len)) {
        let (mask, _) = hash.digest(&[h, &counter.to_be_bytes()]);
        for (byte, m) in chunk.iter_mut().zip(mask.iter()) {
            *byte ^= m;
        }
    }
    db[0] &= top_mask;

    // DB = PS (ceros) || 0x01 || sal
    let padding = db_len - salt_len - 1;
    if db[..padding].iter().any(|&b| b != 0) || db[padding] != 0x01 {
        return false;
    }
    let salt = &db[db_len - salt_len..];
    let (expected, len) = hash.digest(&[&[0u8; 8], message_hash, salt]);
    expected[..len] == *h
}

fn strip_leading_zeros(bytes: &[u8]) -> &[u8] {
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len());
    &bytes[start..]
}

/// Big-endian a palabras little-endian; rellena con ceros las palabras altas.
fn words_from_be(bytes: &[u8], words: &mut [u32]) {
    words.fill(0);
    for (i, &byte) in bytes.iter().rev().enumerate() {
        if let Some(word) = words.get_mut(i / 4) {
            *word |= u32::from(byte) << (8 * (i % 4));
        }
    }
}

/// Palabras little-endian a big-endian con la longitud de `bytes`.
fn be_from_words(words: &[u32], bytes: &mut [u8]) {
    let len = bytes.len();
    for (i, byte) in bytes.iter_mut().enumerate() {
        let index = len - 1 - i;
        *byte = words
            .get(index / 4)
            .map_or(0, |word| (word >> (8 * (index % 4))) as u8);
    }
}

/// Resta el módulo si `value` no es menor que él. La multiplicación de
/// Montgomery del hardware puede devolver resultados en `[n, R)`; el puerto de
/// mbedTLS para el ESP32 hace la misma corrección tras cada operación.
fn reduce_once(value: &mut [u32], modulus: &[u32]) {
    if less_than(value, modulus) {
        return;
    }
    let mut borrow = 0u32;
    for (word, &m) in value.iter_mut().zip(modulus) {
        let (diff, b1) = word.overflowing_sub(m);
        let (diff, b2) = diff.overflowing_sub(borrow);
        *word = diff;
        borrow = u32::from(b1 || b2);
    }
}

fn less_than(a: &[u32], b: &[u32]) -> bool {
    for (x, y) in a.iter().rev().zip(b.iter().rev()) {
        if x != y {
            return x < y;
        }
    }
    false
}

/// `-n⁻¹ mod 2³²` por iteración de Newton (n impar).
pub fn m_prime(n0: u32) -> u32 {
    // n0·n0 ≡ 1 (mod 8): cada paso duplica los bits correctos (3, 6, 12, 24, 48).
    let mut inverse = n0;
    for _ in 0..4 {
        inverse = inverse.wrapping_mul(2u32.wrapping_sub(n0.wrapping_mul(inverse)));
    }
    inverse.wrapping_neg()
}

/// `R² mod n` con `R = 2^(32·len)`, duplicando desde `2^(bits-1)`, que ya es
/// menor que n. Solo usa desplazamientos y restas: el acelerador necesita este
/// valor para operar, así que no puede calcularlo él.
fn r_squared(modulus: &[u32], modulus_bits: usize, out: &mut [u32]) {
    out.fill(0);
    let top = modulus_bits - 1;
    out[top / 32] = 1 << (top % 32);
    let doublings = 2 * 32 * modulus.len() - top;
    for _ in 0..doublings {
        // x < n antes de duplicar, así que 2x < 2n y basta una resta.
        let mut carry = 0u32;
        for word in out.iter_mut() {
            let next = *word >> 31;
            *word = (*word << 1) | carry;
            carry = next;
        }
        if carry == 1 {
            // 2x desbordó las palabras: 2x - n cabe y es menor que n.
            let mut borrow = 0u32;
            for (word, &m) in out.iter_mut().zip(modulus) {
                let (diff, b1) = word.overflowing_sub(m);
                let (diff, b2) = diff.overflowing_sub(borrow);
                *word = diff;
                borrow = u32::from(b1 || b2);
            }
        } else {
            reduce_once(out, modulus);
        }
    }
}
