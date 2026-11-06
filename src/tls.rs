//! Proveedor TLS con verificación de la cadena de certificados del broker.
//!
//! La CA y el servidor de SmartPot usan RSA (4096/2048), por lo que se usa el
//! verificador `embedded_tls::pki::CertVerifier`, que soporta RSA PKCS#1
//! SHA-256/384/512. El reloj lo aporta [`EspClock`], que delega en el epoch
//! sincronizado por SNTP; sin hora válida la verificación de vigencia falla y
//! la conexión se rechaza (fail closed).

use embedded_tls::pki::CertVerifier;
use embedded_tls::{
    Aes128GcmSha256, Certificate, CryptoProvider, CryptoRngCore, TlsClock, TlsError, TlsVerifier,
};
use esp_hal::rng::Trng;

/// Tamaño máximo del mensaje de certificados del servidor que se conserva
/// durante el handshake. La cadena de SmartPot cabe holgadamente en 2 KiB.
pub const CERT_BUFFER_SIZE: usize = 2_048;

/// Reloj TLS basado en el epoch Unix sincronizado por SNTP.
pub struct EspClock;

impl TlsClock for EspClock {
    fn now() -> Option<u64> {
        crate::clock::now()
    }
}

/// Proveedor que combina el TRNG del ESP32 con un verificador de CA propia.
pub struct VerifyingProvider<'a> {
    rng: Trng,
    verifier: CertVerifier<'a, Aes128GcmSha256, EspClock, CERT_BUFFER_SIZE>,
}

impl<'a> VerifyingProvider<'a> {
    /// Crea el proveedor a partir del generador de hardware y la CA en DER.
    pub fn new(rng: Trng, ca_der: &'a [u8]) -> Self {
        Self {
            rng,
            verifier: CertVerifier::new(Certificate::X509(ca_der)),
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
