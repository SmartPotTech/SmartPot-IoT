//! Lectura mínima de certificados X.509 (DER) para el verificador TLS.
//!
//! Solo extrae lo que necesita la verificación del broker: los bytes firmados
//! (`tbsCertificate`), el algoritmo y la firma, la vigencia, la clave pública
//! RSA, la restricción de CA y los nombres DNS (SAN y CN). No depende del
//! hardware, así que se prueba en el host con los certificados reales.

/// Error al leer un certificado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X509Error {
    /// La estructura DER no corresponde a un certificado X.509 v3.
    Malformed,
}

/// Algoritmo con el que el emisor firmó el certificado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    RsaPkcs1Sha256,
    RsaPkcs1Sha384,
    RsaPkcs1Sha512,
    /// Cualquier otro algoritmo (ECDSA, Ed25519, RSA-PSS...): no se acepta.
    Unsupported,
}

/// Clave pública RSA con el módulo y el exponente en big-endian, sin ceros
/// iniciales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsaPublicKey<'a> {
    pub modulus: &'a [u8],
    pub exponent: &'a [u8],
}

/// Campos de un certificado necesarios para verificar la cadena.
#[derive(Debug, Clone, Copy)]
pub struct Certificate<'a> {
    /// `tbsCertificate` completo (etiqueta y longitud incluidas): es lo que se firma.
    pub signed_data: &'a [u8],
    pub signature_algorithm: SignatureAlgorithm,
    pub signature: &'a [u8],
    /// Vigencia en segundos Unix.
    pub not_before: u64,
    pub not_after: u64,
    /// `None` si la clave no es RSA.
    pub public_key: Option<RsaPublicKey<'a>>,
    /// `basicConstraints` con `cA = TRUE`.
    pub is_ca: bool,
    pub common_name: Option<&'a [u8]>,
    /// Contenido de la extensión `subjectAltName` (secuencia de `GeneralName`).
    pub subject_alt_names: Option<&'a [u8]>,
}

const TAG_BOOLEAN: u8 = 0x01;
const TAG_INTEGER: u8 = 0x02;
const TAG_BIT_STRING: u8 = 0x03;
const TAG_OCTET_STRING: u8 = 0x04;
const TAG_OID: u8 = 0x06;
const TAG_UTF8_STRING: u8 = 0x0c;
const TAG_PRINTABLE_STRING: u8 = 0x13;
const TAG_TELETEX_STRING: u8 = 0x14;
const TAG_IA5_STRING: u8 = 0x16;
const TAG_UTC_TIME: u8 = 0x17;
const TAG_GENERALIZED_TIME: u8 = 0x18;
const TAG_SEQUENCE: u8 = 0x30;
const TAG_SET: u8 = 0x31;
/// `[0] EXPLICIT Version` del `tbsCertificate`.
const TAG_VERSION: u8 = 0xa0;
/// `[3] EXPLICIT Extensions` del `tbsCertificate`.
const TAG_EXTENSIONS: u8 = 0xa3;
/// `dNSName [2] IA5String` dentro de `GeneralName`.
const TAG_DNS_NAME: u8 = 0x82;

/// 1.2.840.113549.1.1.1 (rsaEncryption)
const OID_RSA_ENCRYPTION: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
/// 1.2.840.113549.1.1.11 (sha256WithRSAEncryption)
const OID_SHA256_WITH_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b];
/// 1.2.840.113549.1.1.12 (sha384WithRSAEncryption)
const OID_SHA384_WITH_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0c];
/// 1.2.840.113549.1.1.13 (sha512WithRSAEncryption)
const OID_SHA512_WITH_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0d];
/// 2.5.4.3 (commonName)
const OID_COMMON_NAME: &[u8] = &[0x55, 0x04, 0x03];
/// 2.5.29.17 (subjectAltName)
const OID_SUBJECT_ALT_NAME: &[u8] = &[0x55, 0x1d, 0x11];
/// 2.5.29.19 (basicConstraints)
const OID_BASIC_CONSTRAINTS: &[u8] = &[0x55, 0x1d, 0x13];

/// Lector secuencial de elementos DER (TLV).
struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    fn peek_tag(&self) -> Option<u8> {
        self.data.first().copied()
    }

    /// Lee un elemento y devuelve su etiqueta y su contenido.
    fn read_any(&mut self) -> Result<(u8, &'a [u8]), X509Error> {
        let (tag, value, _) = self.read_tlv()?;
        Ok((tag, value))
    }

    /// Lee un elemento y devuelve la etiqueta, el contenido y el TLV completo.
    fn read_tlv(&mut self) -> Result<(u8, &'a [u8], &'a [u8]), X509Error> {
        let tag = *self.data.first().ok_or(X509Error::Malformed)?;
        // X.509 no usa etiquetas de varios bytes.
        if tag & 0x1f == 0x1f {
            return Err(X509Error::Malformed);
        }
        let first = *self.data.get(1).ok_or(X509Error::Malformed)?;
        let (len, header) = if first < 0x80 {
            (usize::from(first), 2)
        } else {
            // Longitud indefinida (BER) o de más de 3 bytes: no es un certificado válido.
            let count = usize::from(first & 0x7f);
            if count == 0 || count > 3 {
                return Err(X509Error::Malformed);
            }
            let mut len = 0usize;
            for i in 0..count {
                len = (len << 8) | usize::from(*self.data.get(2 + i).ok_or(X509Error::Malformed)?);
            }
            (len, 2 + count)
        };
        let end = header.checked_add(len).ok_or(X509Error::Malformed)?;
        if end > self.data.len() {
            return Err(X509Error::Malformed);
        }
        let (tlv, rest) = self.data.split_at(end);
        self.data = rest;
        Ok((tag, &tlv[header..], tlv))
    }

    /// Lee un elemento que debe tener la etiqueta indicada.
    fn read(&mut self, tag: u8) -> Result<&'a [u8], X509Error> {
        let (found, value) = self.read_any()?;
        if found != tag {
            return Err(X509Error::Malformed);
        }
        Ok(value)
    }

    /// Lee el elemento solo si tiene la etiqueta indicada.
    fn read_optional(&mut self, tag: u8) -> Result<Option<&'a [u8]>, X509Error> {
        if self.peek_tag() == Some(tag) {
            self.read(tag).map(Some)
        } else {
            Ok(None)
        }
    }
}

/// Lee un certificado X.509 en DER.
pub fn parse(der: &[u8]) -> Result<Certificate<'_>, X509Error> {
    let mut outer = Reader::new(der);
    let body = outer.read(TAG_SEQUENCE)?;
    if !outer.is_empty() {
        return Err(X509Error::Malformed);
    }

    let mut certificate = Reader::new(body);
    let (tag, tbs, signed_data) = certificate.read_tlv()?;
    if tag != TAG_SEQUENCE {
        return Err(X509Error::Malformed);
    }
    let signature_algorithm = read_signature_algorithm(certificate.read(TAG_SEQUENCE)?)?;
    let signature = bit_string(certificate.read(TAG_BIT_STRING)?)?;
    if !certificate.is_empty() {
        return Err(X509Error::Malformed);
    }

    let mut tbs = Reader::new(tbs);
    tbs.read_optional(TAG_VERSION)?;
    tbs.read(TAG_INTEGER)?;
    // RFC 5280 §4.1.1.2: el algoritmo del cuerpo debe coincidir con el exterior.
    if read_signature_algorithm(tbs.read(TAG_SEQUENCE)?)? != signature_algorithm {
        return Err(X509Error::Malformed);
    }
    tbs.read(TAG_SEQUENCE)?; // issuer
    let mut validity = Reader::new(tbs.read(TAG_SEQUENCE)?);
    let not_before = time(&mut validity)?;
    let not_after = time(&mut validity)?;
    let common_name = common_name(tbs.read(TAG_SEQUENCE)?)?;
    let public_key = rsa_public_key(tbs.read(TAG_SEQUENCE)?)?;

    let mut is_ca = false;
    let mut subject_alt_names = None;
    // Quedan los identificadores únicos opcionales y las extensiones.
    while !tbs.is_empty() {
        let (tag, value) = tbs.read_any()?;
        if tag == TAG_EXTENSIONS {
            let mut extensions = Reader::new(Reader::new(value).read(TAG_SEQUENCE)?);
            while !extensions.is_empty() {
                let mut extension = Reader::new(extensions.read(TAG_SEQUENCE)?);
                let oid = extension.read(TAG_OID)?;
                extension.read_optional(TAG_BOOLEAN)?; // critical
                let value = extension.read(TAG_OCTET_STRING)?;
                if oid == OID_SUBJECT_ALT_NAME {
                    subject_alt_names = Some(Reader::new(value).read(TAG_SEQUENCE)?);
                } else if oid == OID_BASIC_CONSTRAINTS {
                    let mut constraints = Reader::new(Reader::new(value).read(TAG_SEQUENCE)?);
                    is_ca = constraints
                        .read_optional(TAG_BOOLEAN)?
                        .is_some_and(|flag| flag.first().is_some_and(|&b| b != 0));
                }
            }
        }
    }

    Ok(Certificate {
        signed_data,
        signature_algorithm,
        signature,
        not_before,
        not_after,
        public_key,
        is_ca,
        common_name,
        subject_alt_names,
    })
}

fn read_signature_algorithm(identifier: &[u8]) -> Result<SignatureAlgorithm, X509Error> {
    let oid = Reader::new(identifier).read(TAG_OID)?;
    Ok(match oid {
        OID_SHA256_WITH_RSA => SignatureAlgorithm::RsaPkcs1Sha256,
        OID_SHA384_WITH_RSA => SignatureAlgorithm::RsaPkcs1Sha384,
        OID_SHA512_WITH_RSA => SignatureAlgorithm::RsaPkcs1Sha512,
        _ => SignatureAlgorithm::Unsupported,
    })
}

/// Contenido de un BIT STRING sin bits sobrantes.
fn bit_string(value: &[u8]) -> Result<&[u8], X509Error> {
    match value.split_first() {
        Some((0, bits)) => Ok(bits),
        _ => Err(X509Error::Malformed),
    }
}

/// Entero DER positivo sin los ceros iniciales.
fn unsigned_integer(value: &[u8]) -> Result<&[u8], X509Error> {
    match value.first() {
        None => Err(X509Error::Malformed),
        // Bit de signo activo: el entero sería negativo.
        Some(&b) if b & 0x80 != 0 => Err(X509Error::Malformed),
        Some(_) => {
            let start = value.iter().position(|&b| b != 0).unwrap_or(value.len());
            Ok(&value[start..])
        }
    }
}

/// `SubjectPublicKeyInfo`: devuelve la clave solo si es `rsaEncryption`.
fn rsa_public_key(info: &[u8]) -> Result<Option<RsaPublicKey<'_>>, X509Error> {
    let mut info = Reader::new(info);
    let algorithm = info.read(TAG_SEQUENCE)?;
    let key = bit_string(info.read(TAG_BIT_STRING)?)?;
    if Reader::new(algorithm).read(TAG_OID)? != OID_RSA_ENCRYPTION {
        return Ok(None);
    }
    let mut key = Reader::new(Reader::new(key).read(TAG_SEQUENCE)?);
    let modulus = unsigned_integer(key.read(TAG_INTEGER)?)?;
    let exponent = unsigned_integer(key.read(TAG_INTEGER)?)?;
    if modulus.is_empty() || exponent.is_empty() {
        return Err(X509Error::Malformed);
    }
    Ok(Some(RsaPublicKey { modulus, exponent }))
}

/// Primer `commonName` del sujeto.
fn common_name(name: &[u8]) -> Result<Option<&[u8]>, X509Error> {
    let mut rdns = Reader::new(name);
    while !rdns.is_empty() {
        let mut attributes = Reader::new(rdns.read(TAG_SET)?);
        while !attributes.is_empty() {
            let mut attribute = Reader::new(attributes.read(TAG_SEQUENCE)?);
            let oid = attribute.read(TAG_OID)?;
            let (tag, value) = attribute.read_any()?;
            let textual = matches!(
                tag,
                TAG_UTF8_STRING | TAG_PRINTABLE_STRING | TAG_IA5_STRING | TAG_TELETEX_STRING
            );
            if oid == OID_COMMON_NAME && textual {
                return Ok(Some(value));
            }
        }
    }
    Ok(None)
}

/// `Time` de X.509 (UTCTime o GeneralizedTime en UTC) en segundos Unix.
fn time(reader: &mut Reader<'_>) -> Result<u64, X509Error> {
    let (tag, text) = reader.read_any()?;
    let (year, rest) = match (tag, text.len()) {
        // RFC 5280 §4.1.2.5.1: YYMMDDHHMMSSZ, años 50-99 = 19xx.
        (TAG_UTC_TIME, 13) => {
            let yy = digits(&text[..2])?;
            (if yy >= 50 { 1900 + yy } else { 2000 + yy }, &text[2..])
        }
        // RFC 5280 §4.1.2.5.2: YYYYMMDDHHMMSSZ.
        (TAG_GENERALIZED_TIME, 15) => (digits(&text[..4])?, &text[4..]),
        _ => return Err(X509Error::Malformed),
    };
    if rest[10] != b'Z' {
        return Err(X509Error::Malformed);
    }
    let month = digits(&rest[0..2])?;
    let day = digits(&rest[2..4])?;
    let hour = digits(&rest[4..6])?;
    let minute = digits(&rest[6..8])?;
    let second = digits(&rest[8..10])?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return Err(X509Error::Malformed);
    }
    let days = days_from_civil(i64::from(year), month, day);
    let seconds = days * 86_400 + i64::from(hour * 3600 + minute * 60 + second);
    // Fechas anteriores a 1970 equivalen a «desde siempre».
    Ok(u64::try_from(seconds).unwrap_or(0))
}

fn digits(text: &[u8]) -> Result<u32, X509Error> {
    text.iter().try_fold(0u32, |acc, &b| {
        if b.is_ascii_digit() {
            Ok(acc * 10 + u32::from(b - b'0'))
        } else {
            Err(X509Error::Malformed)
        }
    })
}

/// Días desde 1970-01-01 para una fecha del calendario gregoriano.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = i64::from((month + 9) % 12);
    let day_of_year = (153 * month_from_march + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

impl Certificate<'_> {
    /// `true` si el instante `now` (segundos Unix) está dentro de la vigencia.
    pub fn is_valid_at(&self, now: u64) -> bool {
        self.not_before <= now && now <= self.not_after
    }

    /// Comprueba el nombre del servidor como RFC 6125: si el certificado trae
    /// nombres DNS en `subjectAltName` solo cuentan esos; si no, el CN. El
    /// comodín solo vale como etiqueta izquierda completa (`*.dominio.tld`).
    pub fn matches_hostname(&self, host: &str) -> bool {
        let host = host.strip_suffix('.').unwrap_or(host).as_bytes();
        if let Some(names) = self.subject_alt_names {
            let mut names = Reader::new(names);
            let mut has_dns_name = false;
            while let Ok((tag, value)) = names.read_any() {
                if tag == TAG_DNS_NAME {
                    has_dns_name = true;
                    if name_matches(value, host) {
                        return true;
                    }
                }
            }
            if has_dns_name {
                return false;
            }
        }
        self.common_name
            .is_some_and(|name| name_matches(name, host))
    }
}

fn name_matches(pattern: &[u8], host: &[u8]) -> bool {
    if host.is_empty() {
        return false;
    }
    if let Some(suffix) = pattern.strip_prefix(b"*.") {
        // Al menos dos etiquetas tras el comodín: `*.app` no se acepta.
        if !suffix.contains(&b'.') {
            return false;
        }
        return match host.iter().position(|&b| b == b'.') {
            Some(dot) if dot > 0 => host[dot + 1..].eq_ignore_ascii_case(suffix),
            _ => false,
        };
    }
    pattern.eq_ignore_ascii_case(host)
}
