//! Pruebas de configuración: decodificación PEM → DER de la CA y rechazo de
//! entradas sin cabeceras o con base64 inválido (fallo cerrado para TLS).

use smartpot_iot_host::config::ca_cert_der;

const CERTIFICATE_HEADER: &str = "-----BEGIN CERTIFICATE-----";
const CERTIFICATE_FOOTER: &str = "-----END CERTIFICATE-----";

fn pem(body: &str) -> String {
    format!("{CERTIFICATE_HEADER}\n{body}\n{CERTIFICATE_FOOTER}\n")
}

#[test]
fn decodes_known_base64_vectors_to_der() {
    // "AQIDBAUGBwg=" son los bytes 0x01..=0x08.
    let der = ca_cert_der::<32>(&pem("AQIDBAUGBwg=")).unwrap();
    assert_eq!(
        der.as_slice(),
        &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
    );

    // Secuencia DER sintética: 30 09 02 01 01 02 03 04 05 06 07.
    let der = ca_cert_der::<64>(&pem("MAkCAQECAwQFBgc=")).unwrap();
    assert_eq!(
        der.as_slice(),
        &[
            0x30, 0x09, 0x02, 0x01, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07
        ]
    );
}

#[test]
fn ignores_line_breaks_and_whitespace_inside_the_body() {
    let with_crlf =
        format!("{CERTIFICATE_HEADER}\r\nAQID\r\nBAUG\r\nBwg=\r\n{CERTIFICATE_FOOTER}\r\n");
    let der = ca_cert_der::<32>(&with_crlf).unwrap();
    assert_eq!(
        der.as_slice(),
        &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
    );

    let padded = format!("  {CERTIFICATE_HEADER}  \n\tAQID BAUG\nBwg=  \n{CERTIFICATE_FOOTER}  \n");
    let der = ca_cert_der::<32>(&padded).unwrap();
    assert_eq!(
        der.as_slice(),
        &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
    );
}

#[test]
fn rejects_empty_or_missing_headers() {
    assert!(ca_cert_der::<32>("").is_err());
    assert!(ca_cert_der::<32>("   \r\n\t ").is_err());
    // Cuerpo base64 sin cabeceras PEM.
    assert!(ca_cert_der::<32>("AQIDBAUGBwg=").is_err());
    // Solo cabecera inicial.
    assert!(ca_cert_der::<32>(CERTIFICATE_HEADER).is_err());
    // Solo cabecera final.
    assert!(ca_cert_der::<32>(CERTIFICATE_FOOTER).is_err());
    // Cabeceras invertidas.
    assert!(
        ca_cert_der::<32>(&format!("{CERTIFICATE_FOOTER}\nAQID\n{CERTIFICATE_HEADER}")).is_err()
    );
    // Cuerpo vacío entre cabeceras.
    assert!(ca_cert_der::<32>(&pem("")).is_err());
}

#[test]
fn rejects_invalid_base64_padding_and_characters() {
    assert!(ca_cert_der::<32>(&pem("not*base64")).is_err());
    assert!(ca_cert_der::<32>(&pem("AQID_BAUG")).is_err());
    // Bits sobrantes distintos de cero en el último grupo.
    assert!(ca_cert_der::<32>(&pem("AB==")).is_err());
    // Datos después del relleno.
    assert!(ca_cert_der::<32>(&pem("AQ==AQ==")).is_err());
    // Longitud imposible (un solo carácter base64).
    assert!(ca_cert_der::<32>(&pem("A")).is_err());
}

#[test]
fn rejects_a_der_buffer_that_is_too_small() {
    // El vector necesita 8 bytes y el búfer solo admite 4.
    assert!(ca_cert_der::<4>(&pem("AQIDBAUGBwg=")).is_err());
}
