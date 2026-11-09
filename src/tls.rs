//! Proveedor TLS que verifica la cadena del broker con el acelerador RSA.
//!
//! La CA y el servidor de SmartPot usan RSA (4096 y 2048 bits). En software,
//! esas verificaciones tardaban unos 10 s con la CPU de Wokwi (limitada a unos
//! 8 MHz) y EMQX corta los handshakes que pasan de 15 s. El acelerador RSA del
//! ESP32, que Wokwi también simula, las resuelve en milisegundos.
//!
//! Se aceptan certificados firmados con RSA PKCS#1 v1.5 (SHA-256/384/512) y
//! firmas de handshake RSA-PSS, que es lo que usa la CA de SmartPot. La
//! vigencia se comprueba con el epoch de SNTP: sin hora válida la conexión se
//! rechaza (fail closed).

use embassy_time::Instant;
use embedded_io_async::{ErrorType, Read, Write};
use embedded_tls::{
    Aes128GcmSha256, CertificateEntryRef, CertificateRef, CertificateVerifyRef, CryptoProvider,
    CryptoRngCore, SignatureScheme, TlsError, TlsVerifier,
};
use esp_hal::Blocking;
use esp_hal::rng::Trng;
use esp_hal::rsa::Rsa;
use heapless::String;
use sha2::{Digest, Sha256};

use crate::rsa_verify::{self, Hash, MAX_BYTES, ModMul, PreparedKey};
use crate::x509::{self, SignatureAlgorithm};

/// Certificados intermedios admitidos entre la CA y el del broker.
const MAX_INTERMEDIATES: usize = 2;

/// Contexto que el servidor firma en `CertificateVerify` (RFC 8446 §4.4.3).
const SERVER_VERIFY_CONTEXT: &[u8] = b"TLS 1.3, server CertificateVerify\x00";

/// CA del broker con su clave lista para el acelerador. Se prepara antes de
/// abrir el socket porque el broker cuenta el handshake desde la conexión TCP.
pub struct TrustAnchor<'a> {
    der: &'a [u8],
    key: PreparedKey,
}

impl<'a> TrustAnchor<'a> {
    /// Lee la CA en DER; falla si no es un certificado RSA admitido.
    pub fn new(der: &'a [u8]) -> Result<Self, TlsError> {
        let ca = x509::parse(der).map_err(|_| TlsError::DecodeError)?;
        let key = ca.public_key.ok_or(TlsError::InvalidCertificate)?;
        let key = PreparedKey::new(key.modulus, key.exponent)
            .map_err(|_| TlsError::InvalidCertificate)?;
        Ok(Self { der, key })
    }
}

/// Proveedor que combina el TRNG del ESP32 con el verificador por hardware.
pub struct VerifyingProvider<'a> {
    rng: Trng,
    verifier: HardwareVerifier<'a>,
}

impl<'a> VerifyingProvider<'a> {
    pub fn new(
        rng: Trng,
        anchor: &'a TrustAnchor<'a>,
        rsa: &'a mut Rsa<'static, Blocking>,
    ) -> Self {
        Self {
            rng,
            verifier: HardwareVerifier {
                anchor,
                engine: Accelerator { _rsa: rsa },
                host: None,
                transcript: None,
                server_key: None,
            },
        }
    }
}

impl CryptoProvider for VerifyingProvider<'_> {
    type CipherSuite = Aes128GcmSha256;
    type Signature = &'static [u8];

    fn rng(&mut self) -> impl CryptoRngCore {
        &mut self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut self.verifier)
    }
}

/// Registros del acelerador RSA del ESP32 (manual de referencia, cap. 15).
///
/// Se escriben directamente porque el PAC que usa esp-hal 1.1 declara cada
/// bloque de memoria con 32 palabras (1024 bits) aunque el hardware tiene 128:
/// con `RsaModularMultiplication` los operandos de 2048 y 4096 bits llegaban
/// truncados y ninguna firma se verificaba.
mod registers {
    const BASE: usize = 0x3ff0_2000;
    pub const M_MEM: usize = BASE;
    pub const Z_MEM: usize = BASE + 0x200;
    pub const X_MEM: usize = BASE + 0x600;
    pub const M_PRIME: usize = BASE + 0x800;
    pub const MULT_MODE: usize = BASE + 0x80c;
    pub const MULT_START: usize = BASE + 0x810;
    pub const INTERRUPT: usize = BASE + 0x814;
    /// Palabras de cada bloque de memoria.
    pub const MEM_WORDS: usize = 128;
}

/// Sondeos máximos del fin de una operación (una de 4096 bits tarda
/// microsegundos en el silicio); si se agotan, la verificación falla.
const MAX_POLLS: u32 = 1_000_000;

/// Multiplicador modular sobre el acelerador RSA del ESP32.
pub struct Accelerator<'a> {
    /// El controlador de esp-hal enciende el periférico y espera a que limpie
    /// su memoria; tenerlo prestado garantiza el acceso exclusivo.
    _rsa: &'a mut Rsa<'static, Blocking>,
}

impl Accelerator<'_> {
    /// # Safety
    /// El periférico debe estar encendido y `address` debe ser un bloque de
    /// memoria del acelerador con espacio para `words`.
    unsafe fn write_block(address: usize, words: &[u32]) {
        for (i, &word) in words.iter().enumerate() {
            // SAFETY: lo garantiza quien llama.
            unsafe { core::ptr::write_volatile((address + 4 * i) as *mut u32, word) };
        }
    }

    /// Espera el fin de la operación en curso y limpia el aviso.
    ///
    /// # Safety
    /// El periférico debe estar encendido.
    unsafe fn wait_and_clear() -> bool {
        let interrupt = registers::INTERRUPT as *mut u32;
        for _ in 0..MAX_POLLS {
            // SAFETY: lo garantiza quien llama.
            if unsafe { core::ptr::read_volatile(interrupt) } & 1 == 1 {
                // SAFETY: escribir 1 limpia el aviso de fin de operación.
                unsafe { core::ptr::write_volatile(interrupt, 1) };
                return true;
            }
        }
        false
    }
}

impl ModMul for Accelerator<'_> {
    fn mod_mul(&mut self, key: &PreparedKey, a: &[u32], b: &[u32], out: &mut [u32]) {
        let words = key.words();
        let valid_size = words.is_multiple_of(16) && (16..=registers::MEM_WORDS).contains(&words);
        if !valid_size || a.len() != words || b.len() != words || out.len() != words {
            // Un resultado nulo hace fallar la verificación.
            out.fill(0);
            return;
        }

        // Multiplicación modular en dos pasadas (manual §15.3.3): la primera
        // lleva X a la forma de Montgomery con r = R² mod M y la segunda la
        // multiplica por Y.
        // SAFETY: `_rsa` mantiene el periférico encendido y en exclusiva, las
        // direcciones son las del manual y los operandos caben en los bloques.
        let finished = unsafe {
            core::ptr::write_volatile(registers::MULT_MODE as *mut u32, (words / 16 - 1) as u32);
            Self::write_block(registers::X_MEM, a);
            Self::write_block(registers::M_MEM, key.modulus());
            Self::write_block(registers::Z_MEM, key.r_squared());
            core::ptr::write_volatile(registers::M_PRIME as *mut u32, key.m_prime());
            core::ptr::write_volatile(registers::MULT_START as *mut u32, 1);
            let first = Self::wait_and_clear();
            Self::write_block(registers::X_MEM, b);
            core::ptr::write_volatile(registers::MULT_START as *mut u32, 1);
            first && Self::wait_and_clear()
        };
        if !finished {
            out.fill(0);
            return;
        }
        for (i, word) in out.iter_mut().enumerate() {
            // SAFETY: lectura del bloque Z, dentro de sus 128 palabras.
            *word = unsafe { core::ptr::read_volatile((registers::Z_MEM + 4 * i) as *const u32) };
        }
    }
}

/// Verificador de la cadena del broker y de la firma del handshake.
pub struct HardwareVerifier<'a> {
    anchor: &'a TrustAnchor<'a>,
    engine: Accelerator<'a>,
    host: Option<String<64>>,
    transcript: Option<Sha256>,
    /// Clave del certificado del broker para `CertificateVerify`.
    server_key: Option<PreparedKey>,
}

impl HardwareVerifier<'_> {
    fn verify_chain(&mut self, chain: &CertificateRef) -> Result<(), TlsError> {
        let now = crate::clock::now().ok_or(TlsError::InvalidCertificate)?;
        let host = self.host.as_deref().ok_or(TlsError::InvalidCertificate)?;
        let anchor = self.anchor;

        // El broker envía también la CA; es el ancla de confianza, así que no
        // se vuelve a verificar (en software eso costaba unos 4 s en Wokwi).
        let mut entries: &[CertificateEntryRef] = &chain.entries;
        while let Some((CertificateEntryRef::X509(last), rest)) = entries.split_last() {
            if *last != anchor.der {
                break;
            }
            entries = rest;
        }
        let Some((leaf, intermediates)) = entries.split_first() else {
            return Err(TlsError::InvalidCertificate);
        };
        if intermediates.len() > MAX_INTERMEDIATES {
            return Err(TlsError::InvalidCertificate);
        }

        // De la CA hacia el broker: cada certificado se verifica con la clave
        // del anterior.
        let mut issuer: Option<PreparedKey> = None;
        let path = intermediates.iter().rev().chain(core::iter::once(leaf));
        for (index, entry) in path.enumerate() {
            let CertificateEntryRef::X509(der) = entry else {
                return Err(TlsError::InvalidCertificate);
            };
            let certificate = x509::parse(der).map_err(|_| TlsError::DecodeError)?;
            if !certificate.is_valid_at(now) {
                error!("[TLS] A broker certificate is expired or not yet valid");
                return Err(TlsError::InvalidCertificate);
            }
            let issuer_key = issuer.as_ref().unwrap_or(&anchor.key);
            verify_issued_by(&mut self.engine, issuer_key, &certificate)?;

            let key = certificate.public_key.ok_or(TlsError::InvalidCertificate)?;
            let key = PreparedKey::new(key.modulus, key.exponent)
                .map_err(|_| TlsError::InvalidCertificate)?;
            if index < intermediates.len() {
                if !certificate.is_ca {
                    return Err(TlsError::InvalidCertificate);
                }
                issuer = Some(key);
            } else {
                if !certificate.matches_hostname(host) {
                    error!("[TLS] The broker certificate is not valid for {}", host);
                    return Err(TlsError::InvalidCertificate);
                }
                self.server_key = Some(key);
            }
        }
        Ok(())
    }

    fn verify_handshake(&mut self, verify: &CertificateVerifyRef) -> Result<(), TlsError> {
        let key = self.server_key.take().ok_or(TlsError::InvalidSignature)?;
        let transcript = self.transcript.take().ok_or(TlsError::InvalidSignature)?;
        let hash = match verify.signature_scheme {
            SignatureScheme::RsaPssRsaeSha256 => Hash::Sha256,
            SignatureScheme::RsaPssRsaeSha384 => Hash::Sha384,
            SignatureScheme::RsaPssRsaeSha512 => Hash::Sha512,
            _ => return Err(TlsError::InvalidSignatureScheme),
        };

        // 64 espacios, el contexto y el resumen de la transcripción.
        let transcript_hash = transcript.finalize();
        let (message_hash, len) = hash.digest(&[
            &[0x20; 64],
            SERVER_VERIFY_CONTEXT,
            transcript_hash.as_slice(),
        ]);
        let mut encoded = [0u8; MAX_BYTES];
        let encoded_len =
            rsa_verify::public_operation(&mut self.engine, &key, verify.signature, &mut encoded)
                .map_err(|_| TlsError::InvalidSignature)?;
        let valid = rsa_verify::pss_matches(
            &encoded[..encoded_len],
            key.modulus_bits(),
            hash,
            &message_hash[..len],
        );
        if valid {
            Ok(())
        } else {
            Err(TlsError::InvalidSignature)
        }
    }
}

/// Comprueba que `certificate` está firmado por la clave `issuer`.
fn verify_issued_by(
    engine: &mut Accelerator<'_>,
    issuer: &PreparedKey,
    certificate: &x509::Certificate<'_>,
) -> Result<(), TlsError> {
    let hash = match certificate.signature_algorithm {
        SignatureAlgorithm::RsaPkcs1Sha256 => Hash::Sha256,
        SignatureAlgorithm::RsaPkcs1Sha384 => Hash::Sha384,
        SignatureAlgorithm::RsaPkcs1Sha512 => Hash::Sha512,
        SignatureAlgorithm::Unsupported => {
            error!("[TLS] A broker certificate uses a signature algorithm other than RSA PKCS#1");
            return Err(TlsError::InvalidSignatureScheme);
        }
    };
    let (digest, len) = hash.digest(&[certificate.signed_data]);
    let mut encoded = [0u8; MAX_BYTES];
    let encoded_len =
        rsa_verify::public_operation(engine, issuer, certificate.signature, &mut encoded)
            .map_err(|_| TlsError::InvalidCertificate)?;
    if rsa_verify::pkcs1v15_matches(&encoded[..encoded_len], hash, &digest[..len]) {
        Ok(())
    } else {
        // Un bloque válido empieza por 00 01 FF FF; sirve para distinguir un
        // fallo del acelerador de una firma realmente distinta.
        debug!(
            "[TLS] Recovered signature block starts with {:02x?}",
            &encoded[..encoded_len.min(4)]
        );
        error!("[TLS] A broker certificate signature does not match its issuer");
        Err(TlsError::InvalidCertificate)
    }
}

impl TlsVerifier<Aes128GcmSha256> for HardwareVerifier<'_> {
    fn set_hostname_verification(&mut self, hostname: &str) -> Result<(), TlsError> {
        self.host = Some(String::try_from(hostname).map_err(|_| TlsError::InsufficientSpace)?);
        Ok(())
    }

    fn verify_certificate(
        &mut self,
        transcript: &Sha256,
        chain: CertificateRef,
    ) -> Result<(), TlsError> {
        let started = Instant::now();
        let result = self.verify_chain(&chain);
        debug!(
            "[TLS] Server certificate checked against the CA in {} ms",
            started.elapsed().as_millis()
        );
        if result.is_ok() {
            self.transcript = Some(transcript.clone());
        }
        result
    }

    fn verify_signature(&mut self, verify: CertificateVerifyRef) -> Result<(), TlsError> {
        let started = Instant::now();
        let result = self.verify_handshake(&verify);
        debug!(
            "[TLS] Server handshake signature checked in {} ms",
            started.elapsed().as_millis()
        );
        result
    }
}

/// Operaciones registradas por conexión: cubren el handshake TLS y el CONNECT.
#[cfg(feature = "verbose-logs")]
const TRACED_OPERATIONS: u8 = 24;

/// Transporte que, con `verbose-logs`, registra las primeras lecturas y
/// escrituras de cada conexión con lo que esperó cada una. Así se separa la
/// latencia de la red del cálculo criptográfico del handshake, que el broker
/// corta si pasa de 15 s. Sin `verbose-logs` solo delega.
pub struct TracedIo<T> {
    inner: T,
    #[cfg(feature = "verbose-logs")]
    remaining: u8,
}

impl<T> TracedIo<T> {
    /// Envuelve el transporte de una conexión nueva.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            #[cfg(feature = "verbose-logs")]
            remaining: TRACED_OPERATIONS,
        }
    }

    #[cfg(feature = "verbose-logs")]
    fn should_trace(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

impl<T: ErrorType> ErrorType for TracedIo<T> {
    type Error = T::Error;
}

impl<T: Read> Read for TracedIo<T> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        #[cfg(feature = "verbose-logs")]
        if self.should_trace() {
            let started = Instant::now();
            let result = self.inner.read(buf).await;
            let waited = started.elapsed().as_millis();
            match &result {
                Ok(n) => debug!("[TLS] <- {} B after waiting {} ms", n, waited),
                Err(e) => debug!("[TLS] <- {:?} after waiting {} ms", e, waited),
            }
            return result;
        }
        self.inner.read(buf).await
    }
}

impl<T: Write> Write for TracedIo<T> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        #[cfg(feature = "verbose-logs")]
        if self.should_trace() {
            let started = Instant::now();
            let result = self.inner.write(buf).await;
            let waited = started.elapsed().as_millis();
            match &result {
                Ok(n) => debug!("[TLS] -> {} B after waiting {} ms", n, waited),
                Err(e) => debug!("[TLS] -> {:?} after waiting {} ms", e, waited),
            }
            return result;
        }
        self.inner.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush().await
    }
}
