//! Pruebas del cliente DNS mínimo del resolvedor de respaldo.

use smartpot_iot_host::dns::{DnsError, build_query, parse_response};

const ID: u16 = 0x1234;
const HOST: &str = "pool.ntp.org";

/// Respuesta a la consulta de `HOST`: la pregunta repetida y los registros dados.
fn response(flags: u16, answers: &[&[u8]]) -> Vec<u8> {
    let mut packet = build_query(ID, HOST).unwrap().to_vec();
    packet[2..4].copy_from_slice(&flags.to_be_bytes());
    packet[6..8].copy_from_slice(&(answers.len() as u16).to_be_bytes());
    for answer in answers {
        packet.extend_from_slice(answer);
    }
    packet
}

/// Registro A con el nombre comprimido como puntero a la pregunta.
fn a_record(address: [u8; 4]) -> Vec<u8> {
    let mut record = vec![0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0x0E, 0x10, 0, 4];
    record.extend_from_slice(&address);
    record
}

#[test]
fn builds_a_recursive_a_query() {
    let query = build_query(ID, HOST).unwrap();
    let mut expected = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    expected.extend_from_slice(b"\x04pool\x03ntp\x03org\x00");
    expected.extend_from_slice(&[0, 1, 0, 1]);
    assert_eq!(query.as_slice(), expected.as_slice());
    // Un punto final no cambia la consulta.
    assert_eq!(build_query(ID, "pool.ntp.org.").unwrap(), query);
}

#[test]
fn rejects_invalid_names() {
    assert_eq!(build_query(ID, ""), Err(DnsError::InvalidName));
    assert_eq!(build_query(ID, "a..b"), Err(DnsError::InvalidName));
    assert_eq!(build_query(ID, &"a".repeat(64)), Err(DnsError::InvalidName));
    let long = ["abcdefghij"; 25].join(".");
    assert_eq!(build_query(ID, &long), Err(DnsError::InvalidName));
}

#[test]
fn returns_the_first_a_record() {
    let packet = response(
        0x8180,
        &[&a_record([162, 159, 200, 1]), &a_record([1, 2, 3, 4])],
    );
    assert_eq!(parse_response(&packet, ID, HOST), Ok([162, 159, 200, 1]));
}

#[test]
fn follows_a_cname_to_the_a_record() {
    let mut cname = vec![0xC0, 0x0C, 0, 5, 0, 1, 0, 0, 0, 60, 0, 5];
    cname.extend_from_slice(b"\x02eu\xC0\x11");
    let packet = response(0x8180, &[&cname, &a_record([10, 0, 0, 7])]);
    assert_eq!(parse_response(&packet, ID, HOST), Ok([10, 0, 0, 7]));
}

#[test]
fn compares_names_without_case() {
    let packet = response(0x8180, &[&a_record([9, 9, 9, 9])]);
    assert_eq!(
        parse_response(&packet, ID, "POOL.NTP.ORG"),
        Ok([9, 9, 9, 9])
    );
    assert_eq!(
        parse_response(&packet, ID, "pool.ntp.org."),
        Ok([9, 9, 9, 9])
    );
}

#[test]
fn rejects_responses_that_do_not_match_the_query() {
    let packet = response(0x8180, &[&a_record([9, 9, 9, 9])]);
    assert_eq!(
        parse_response(&packet, ID + 1, HOST),
        Err(DnsError::UnexpectedId)
    );
    assert_eq!(
        parse_response(&packet, ID, "time.ntp.org"),
        Err(DnsError::NameMismatch)
    );
    assert_eq!(
        parse_response(&packet, ID, "ntp.org"),
        Err(DnsError::NameMismatch)
    );
    let query = build_query(ID, HOST).unwrap();
    assert_eq!(
        parse_response(&query, ID, HOST),
        Err(DnsError::NotAResponse)
    );
}

#[test]
fn reports_server_errors() {
    assert_eq!(
        parse_response(&response(0x8183, &[]), ID, HOST),
        Err(DnsError::NoSuchName)
    );
    assert_eq!(
        parse_response(&response(0x8182, &[]), ID, HOST),
        Err(DnsError::ServerFailure(2))
    );
    assert_eq!(
        parse_response(&response(0x8180, &[]), ID, HOST),
        Err(DnsError::NoAddress)
    );
}

#[test]
fn rejects_truncated_and_looping_packets() {
    let packet = response(0x8180, &[&a_record([9, 9, 9, 9])]);
    assert_eq!(
        parse_response(&packet[..packet.len() - 2], ID, HOST),
        Err(DnsError::Malformed)
    );
    assert_eq!(
        parse_response(&packet[..8], ID, HOST),
        Err(DnsError::Malformed)
    );

    // La pregunta apunta a sí misma: no debe quedarse en un bucle.
    let mut looping = vec![
        0x12, 0x34, 0x81, 0x80, 0, 1, 0, 0, 0, 0, 0, 0, 0xC0, 0x0C, 0, 1, 0, 1,
    ];
    assert_eq!(parse_response(&looping, ID, HOST), Err(DnsError::Malformed));
    looping[12] = 0x80;
    assert_eq!(parse_response(&looping, ID, HOST), Err(DnsError::Malformed));
}
