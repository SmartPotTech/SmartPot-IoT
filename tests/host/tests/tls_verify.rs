//! Pruebas del lector X.509 y de la verificación RSA del broker.
//!
//! Usan la CA y el certificado reales de `mqtt.smartpot.app` (públicos),
//! certificados auxiliares y firmas PSS generadas con openssl, y un
//! multiplicador de Montgomery que recibe `R² mod n` y `m'` igual que el
//! acelerador del ESP32: si esos parámetros fueran incorrectos, ninguna firma
//! se verificaría.

use smartpot_iot_host::rsa_verify::{
    Hash, MAX_BYTES, ModMul, PreparedKey, RsaError, m_prime, pkcs1v15_matches, pss_matches,
    public_operation,
};
use smartpot_iot_host::x509::{self, SignatureAlgorithm};

const CA: &[u8] = include_bytes!("fixtures/tls/smartpot_ca.der");
const LEAF: &[u8] = include_bytes!("fixtures/tls/mqtt_smartpot_app.der");
const WILDCARD: &[u8] = include_bytes!("fixtures/tls/wildcard_sha384.der");
const CN_ONLY: &[u8] = include_bytes!("fixtures/tls/cn_only_sha512.der");
const PSS_VECTORS: &str = include_str!("fixtures/tls/pss_vectors.json");

/// Multiplicador de referencia con la semántica del acelerador del ESP32:
/// primero `a·R² ·R⁻¹ = a·R` y después `(a·R)·b·R⁻¹ = a·b mod n`.
struct Montgomery;

impl ModMul for Montgomery {
    fn mod_mul(&mut self, key: &PreparedKey, a: &[u32], b: &[u32], out: &mut [u32]) {
        let a_mont = mont_mul(a, key.r_squared(), key.modulus(), key.m_prime());
        let product = mont_mul(&a_mont, b, key.modulus(), key.m_prime());
        out.copy_from_slice(&product);
    }
}

/// Producto de Montgomery `x·y·R⁻¹ mod n` (CIOS) con `m' = -n⁻¹ mod 2³²`.
fn mont_mul(x: &[u32], y: &[u32], n: &[u32], m_prime: u32) -> Vec<u32> {
    let s = n.len();
    let mut t = vec![0u32; s + 2];
    for &yi in y {
        let mut carry = 0u64;
        for j in 0..s {
            let v = u64::from(t[j]) + u64::from(x[j]) * u64::from(yi) + carry;
            t[j] = v as u32;
            carry = v >> 32;
        }
        let v = u64::from(t[s]) + carry;
        t[s] = v as u32;
        t[s + 1] = (v >> 32) as u32;

        let m = t[0].wrapping_mul(m_prime);
        let mut carry = (u64::from(t[0]) + u64::from(m) * u64::from(n[0])) >> 32;
        for j in 1..s {
            let v = u64::from(t[j]) + u64::from(m) * u64::from(n[j]) + carry;
            t[j - 1] = v as u32;
            carry = v >> 32;
        }
        let v = u64::from(t[s]) + carry;
        t[s - 1] = v as u32;
        t[s] = t[s + 1] + (v >> 32) as u32;
        t[s + 1] = 0;
    }

    let mut result = t[..s].to_vec();
    let ge = t[s] != 0 || {
        let mut ge = true;
        for (a, b) in result.iter().rev().zip(n.iter().rev()) {
            if a != b {
                ge = a > b;
                break;
            }
        }
        ge
    };
    if ge {
        let mut borrow = 0u32;
        for (r, &m) in result.iter_mut().zip(n) {
            let (d, b1) = r.overflowing_sub(m);
            let (d, b2) = d.overflowing_sub(borrow);
            *r = d;
            borrow = u32::from(b1 || b2);
        }
    }
    result
}

/// Como `Montgomery`, pero devuelve `a·b mod n + n` siempre que quepa en las
/// palabras del operando, como puede hacer el hardware sin la resta final.
struct Unreduced;

impl ModMul for Unreduced {
    fn mod_mul(&mut self, key: &PreparedKey, a: &[u32], b: &[u32], out: &mut [u32]) {
        Montgomery.mod_mul(key, a, b, out);
        let mut sum = out.to_vec();
        let mut carry = 0u64;
        for (s, &m) in sum.iter_mut().zip(key.modulus()) {
            let v = u64::from(*s) + u64::from(m) + carry;
            *s = v as u32;
            carry = v >> 32;
        }
        if carry == 0 {
            out.copy_from_slice(&sum);
        }
    }
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn hash_of(algorithm: SignatureAlgorithm) -> Hash {
    match algorithm {
        SignatureAlgorithm::RsaPkcs1Sha256 => Hash::Sha256,
        SignatureAlgorithm::RsaPkcs1Sha384 => Hash::Sha384,
        SignatureAlgorithm::RsaPkcs1Sha512 => Hash::Sha512,
        SignatureAlgorithm::Unsupported => panic!("algoritmo no soportado"),
    }
}

/// Verifica la firma de `subject` con la clave de `issuer`.
fn issued_by(subject: &x509::Certificate, issuer: &x509::Certificate) -> bool {
    let key = issuer.public_key.expect("clave RSA");
    let key = PreparedKey::new(key.modulus, key.exponent).expect("clave soportada");
    let hash = hash_of(subject.signature_algorithm);
    let (digest, len) = hash.digest(&[subject.signed_data]);
    let mut encoded = [0u8; MAX_BYTES];
    match public_operation(&mut Montgomery, &key, subject.signature, &mut encoded) {
        Ok(n) => pkcs1v15_matches(&encoded[..n], hash, &digest[..len]),
        Err(_) => false,
    }
}

#[test]
fn reads_the_broker_ca() {
    let ca = x509::parse(CA).unwrap();
    assert_eq!(ca.signature_algorithm, SignatureAlgorithm::RsaPkcs1Sha256);
    assert!(ca.is_ca);
    assert_eq!(ca.common_name, Some(&b"SmartPot MQTT CA"[..]));
    assert_eq!(ca.not_before, 1_790_465_202); // 2026-09-26 23:26:42 UTC
    assert_eq!(ca.not_after, 2_105_825_202); // 2036-09-23 23:26:42 UTC
    let key = ca.public_key.unwrap();
    assert_eq!(key.modulus.len(), 512);
    assert_eq!(key.exponent, &[0x01, 0x00, 0x01]);
    let prepared = PreparedKey::new(key.modulus, key.exponent).unwrap();
    assert_eq!(prepared.words(), 128);
    assert_eq!(prepared.modulus_bits(), 4096);
}

#[test]
fn reads_the_broker_certificate() {
    let leaf = x509::parse(LEAF).unwrap();
    assert!(!leaf.is_ca);
    assert_eq!(leaf.common_name, Some(&b"mqtt.smartpot.app"[..]));
    assert_eq!(leaf.not_before, 1_790_465_202);
    assert_eq!(leaf.not_after, 1_861_745_202); // 2028-12-29 23:26:42 UTC
    assert!(leaf.is_valid_at(1_794_249_912));
    assert!(!leaf.is_valid_at(1_790_465_201));
    assert!(!leaf.is_valid_at(1_861_745_203));
    let key = PreparedKey::new(leaf.public_key.unwrap().modulus, &[1, 0, 1]).unwrap();
    assert_eq!(key.words(), 64);
}

#[test]
fn matches_the_broker_hostnames() {
    let leaf = x509::parse(LEAF).unwrap();
    assert!(leaf.matches_hostname("mqtt.smartpot.app"));
    assert!(leaf.matches_hostname("MQTT.SmartPot.App."));
    assert!(leaf.matches_hostname("localhost"));
    assert!(leaf.matches_hostname("broker-smartpot"));
    assert!(!leaf.matches_hostname("smartpot.app"));
    assert!(!leaf.matches_hostname("evil-mqtt.smartpot.app"));
    // La entrada IP del SAN no es un nombre DNS.
    assert!(!leaf.matches_hostname("127.0.0.1"));
    assert!(!leaf.matches_hostname(""));
}

#[test]
fn wildcards_cover_exactly_one_label() {
    let cert = x509::parse(WILDCARD).unwrap();
    assert!(cert.matches_hostname("device.smartpot.test"));
    assert!(cert.matches_hostname("exact.example"));
    assert!(!cert.matches_hostname("smartpot.test"));
    assert!(!cert.matches_hostname("a.b.smartpot.test"));
    // Con nombres DNS en el SAN, el CN no cuenta.
    assert!(!cert.matches_hostname("ignored.example"));
}

#[test]
fn falls_back_to_the_common_name_without_san() {
    let cert = x509::parse(CN_ONLY).unwrap();
    assert!(cert.subject_alt_names.is_none());
    assert!(cert.matches_hostname("cn-only.example"));
    assert!(!cert.matches_hostname("other.example"));
}

#[test]
fn verifies_the_broker_chain() {
    let ca = x509::parse(CA).unwrap();
    let leaf = x509::parse(LEAF).unwrap();
    assert!(issued_by(&leaf, &ca));
    assert!(issued_by(&ca, &ca));
    // La hoja no firmó la CA.
    assert!(!issued_by(&ca, &leaf));
}

#[test]
fn tolerates_unreduced_hardware_results() {
    let ca = x509::parse(CA).unwrap();
    let leaf = x509::parse(LEAF).unwrap();
    let key = ca.public_key.unwrap();
    let key = PreparedKey::new(key.modulus, key.exponent).unwrap();
    let (digest, len) = Hash::Sha256.digest(&[leaf.signed_data]);
    let mut encoded = [0u8; MAX_BYTES];
    let n = public_operation(&mut Unreduced, &key, leaf.signature, &mut encoded).unwrap();
    assert!(pkcs1v15_matches(
        &encoded[..n],
        Hash::Sha256,
        &digest[..len]
    ));
}

#[test]
fn verifies_sha384_and_sha512_certificates() {
    let wildcard = x509::parse(WILDCARD).unwrap();
    assert_eq!(
        wildcard.signature_algorithm,
        SignatureAlgorithm::RsaPkcs1Sha384
    );
    assert!(issued_by(&wildcard, &wildcard));

    let cn_only = x509::parse(CN_ONLY).unwrap();
    assert_eq!(
        cn_only.signature_algorithm,
        SignatureAlgorithm::RsaPkcs1Sha512
    );
    assert!(issued_by(&cn_only, &cn_only));
}

#[test]
fn rejects_a_tampered_certificate() {
    let ca = x509::parse(CA).unwrap();
    let leaf = x509::parse(LEAF).unwrap();
    // Cambia un byte del cuerpo firmado (dentro del nombre del sujeto).
    let mut forged = LEAF.to_vec();
    let at = forged
        .windows(17)
        .position(|w| w == b"mqtt.smartpot.app")
        .unwrap();
    forged[at] = b'n';
    let forged = x509::parse(&forged).unwrap();
    assert!(!issued_by(&forged, &ca));

    // Firma alterada.
    let mut signature = leaf.signature.to_vec();
    signature[100] ^= 1;
    let altered = x509::Certificate {
        signature: &signature,
        ..leaf
    };
    assert!(!issued_by(&altered, &ca));
}

#[test]
fn rejects_malformed_der() {
    assert_eq!(x509::parse(&[]).unwrap_err(), x509::X509Error::Malformed);
    assert!(x509::parse(&LEAF[..LEAF.len() - 1]).is_err());
    let mut trailing = LEAF.to_vec();
    trailing.push(0);
    assert!(x509::parse(&trailing).is_err());
    // Longitud que excede los datos.
    assert!(x509::parse(&[0x30, 0x84, 0xff, 0xff, 0xff, 0xff]).is_err());
}

#[test]
fn verifies_pss_handshake_signatures() {
    let vectors: serde_json::Value = serde_json::from_str(PSS_VECTORS).unwrap();
    for vector in vectors.as_array().unwrap() {
        let bits = vector["bits"].as_u64().unwrap() as usize;
        let hash = match vector["hash"].as_str().unwrap() {
            "sha256" => Hash::Sha256,
            "sha384" => Hash::Sha384,
            _ => Hash::Sha512,
        };
        let key = PreparedKey::new(&hex(vector["modulus"].as_str().unwrap()), &[1, 0, 1]).unwrap();
        assert_eq!(key.modulus_bits(), bits);
        let message = hex(vector["message"].as_str().unwrap());
        let signature = hex(vector["signature"].as_str().unwrap());
        let (digest, len) = hash.digest(&[&message]);

        let mut encoded = [0u8; MAX_BYTES];
        let n = public_operation(&mut Montgomery, &key, &signature, &mut encoded).unwrap();
        assert!(
            pss_matches(&encoded[..n], bits, hash, &digest[..len]),
            "{bits}"
        );

        // Otro mensaje con la misma firma.
        let mut other = message.clone();
        *other.last_mut().unwrap() ^= 1;
        let (digest, len) = hash.digest(&[&other]);
        assert!(!pss_matches(&encoded[..n], bits, hash, &digest[..len]));

        // Firma alterada.
        let mut altered = signature.clone();
        altered[10] ^= 0x40;
        let n = public_operation(&mut Montgomery, &key, &altered, &mut encoded).unwrap();
        let (digest, len) = hash.digest(&[&message]);
        assert!(!pss_matches(&encoded[..n], bits, hash, &digest[..len]));
    }
}

#[test]
fn rejects_unsupported_keys_and_signatures() {
    let ca = x509::parse(CA).unwrap();
    let modulus = ca.public_key.unwrap().modulus;

    // Claves de menos de 2048 bits, pares o con exponentes no válidos.
    assert_eq!(
        PreparedKey::new(&modulus[..128], &[1, 0, 1]).err(),
        Some(RsaError::UnsupportedModulus)
    );
    let mut even = modulus.to_vec();
    *even.last_mut().unwrap() &= 0xfe;
    assert_eq!(
        PreparedKey::new(&even, &[1, 0, 1]).err(),
        Some(RsaError::UnsupportedModulus)
    );
    for exponent in [&[1u8][..], &[2], &[1, 0, 0, 0, 1]] {
        assert_eq!(
            PreparedKey::new(modulus, exponent).err(),
            Some(RsaError::UnsupportedExponent)
        );
    }

    let key = PreparedKey::new(modulus, &[1, 0, 1]).unwrap();
    let mut encoded = [0u8; MAX_BYTES];
    // Firma más corta que el módulo.
    assert_eq!(
        public_operation(&mut Montgomery, &key, &[1u8; 511], &mut encoded).unwrap_err(),
        RsaError::SignatureOutOfRange
    );
    // Firma mayor o igual que el módulo.
    assert_eq!(
        public_operation(&mut Montgomery, &key, modulus, &mut encoded).unwrap_err(),
        RsaError::SignatureOutOfRange
    );
}

#[test]
fn computes_the_montgomery_constant() {
    for n0 in [1u32, 3, 0x0001_0001, 0xffff_ffff, 0x8000_0001, 0x1234_5679] {
        assert_eq!(n0.wrapping_mul(m_prime(n0)), u32::MAX, "{n0:#x}");
    }
}
