//! Pruebas de la validación SNTP y de la conciliación del reloj.

use smartpot_iot_host::sntp::{
    MIN_VALID_EPOCH, NTP_PACKET_SIZE, build_request, parse_response, reconcile_epoch,
};

const NTP_UNIX_DELTA: u64 = 2_208_988_800;
const NONCE: u64 = 0x0123_4567_89AB_CDEF;
const EPOCH: u64 = 1_794_046_494; // 2026-11-07

fn response(ntp_seconds: u32, nonce: u64) -> [u8; NTP_PACKET_SIZE] {
    let mut packet = [0u8; NTP_PACKET_SIZE];
    packet[0] = 0x24; // LI = 0, VN = 4, modo 4 (servidor).
    packet[1] = 2;
    packet[24..32].copy_from_slice(&nonce.to_be_bytes());
    packet[40..44].copy_from_slice(&ntp_seconds.to_be_bytes());
    packet
}

fn ntp(epoch: u64) -> u32 {
    ((epoch + NTP_UNIX_DELTA) % (1 << 32)) as u32
}

#[test]
fn request_carries_the_nonce_in_the_transmit_timestamp() {
    let request = build_request(NONCE);
    assert_eq!(request[0], 0x23);
    assert_eq!(request[40..48], NONCE.to_be_bytes());
}

#[test]
fn accepts_a_matching_response() {
    assert_eq!(
        parse_response(&response(ntp(EPOCH), NONCE), NONCE),
        Ok(EPOCH)
    );
}

#[test]
fn rejects_responses_that_do_not_echo_the_nonce() {
    let spoofed = response(ntp(EPOCH), NONCE ^ 1);
    assert!(parse_response(&spoofed, NONCE).is_err());
}

#[test]
fn rejects_wrong_mode_version_stratum_and_short_packets() {
    let mut packet = response(ntp(EPOCH), NONCE);
    packet[0] = 0x23; // modo cliente
    assert!(parse_response(&packet, NONCE).is_err());

    let mut packet = response(ntp(EPOCH), NONCE);
    packet[0] = 0x14; // VN = 2
    assert!(parse_response(&packet, NONCE).is_err());

    let mut packet = response(ntp(EPOCH), NONCE);
    packet[0] = 0x1c; // VN = 3 se acepta
    assert_eq!(parse_response(&packet, NONCE), Ok(EPOCH));

    let mut packet = response(ntp(EPOCH), NONCE);
    packet[1] = 0; // Kiss-o'-Death
    assert!(parse_response(&packet, NONCE).is_err());

    assert!(parse_response(&[0u8; 12], NONCE).is_err());
}

#[test]
fn handles_the_2036_ntp_era_rollover() {
    let after_rollover = 2_100_000_000; // 2036-07-18, ya en la era 1
    assert_eq!(
        parse_response(&response(ntp(after_rollover), NONCE), NONCE),
        Ok(after_rollover)
    );
}

#[test]
fn rejects_timestamps_before_the_firmware_existed() {
    assert!(parse_response(&response(ntp(MIN_VALID_EPOCH - 1), NONCE), NONCE).is_err());
}

#[test]
fn reconcile_steps_forward_ignores_truncation_and_corrects_fast_clocks() {
    assert_eq!(reconcile_epoch(None, EPOCH), Some(EPOCH));
    assert_eq!(reconcile_epoch(Some(EPOCH), EPOCH + 3), Some(EPOCH + 3));
    // Un valor igual renueva la referencia sin mover la hora.
    assert_eq!(reconcile_epoch(Some(EPOCH), EPOCH), Some(EPOCH));
    // Hasta dos segundos de retraso son truncado del segundo: se ignoran.
    assert_eq!(reconcile_epoch(Some(EPOCH + 2), EPOCH), None);
    // Un cristal que adelanta ya no impide corregir la hora.
    assert_eq!(reconcile_epoch(Some(EPOCH + 60), EPOCH), Some(EPOCH));
}
