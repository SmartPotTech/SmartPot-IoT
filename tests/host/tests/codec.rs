//! Pruebas de contrato del códec compacto v2 contra una implementación
//! independiente del redondeo y de la máscara de presencia.
//!
//! Para cada vector se reconstruye el paquete esperado byte a byte: cabecera,
//! secuencia, época, máscara, campos escalados con `libm::roundf` y CRC-8.

use libm::{floorf, roundf};
use smartpot_iot_host::codec::{
    COMPACT_BINARY_MAGIC, COMPACT_BINARY_VERSION, PACKET_SIZE, calc_crc8, encode_state,
};
use smartpot_iot_host::limits::{self, ApiRange};
use smartpot_iot_host::protocol::SensorState;

const FIELD_COUNT: usize = 7;

/// Escala de cada campo, en el mismo orden que la máscara de presencia.
const SCALES: [f32; FIELD_COUNT] = [100.0, 100.0, 1.0, 100.0, 1.0, 10.0, 100.0];

/// Rangos de la API compartidos con el firmware.
const RANGES: [ApiRange; FIELD_COUNT] = [
    limits::TEMPERATURE,
    limits::HUMIDITY,
    limits::BRIGHTNESS,
    limits::PH,
    limits::TDS,
    limits::ATMOSPHERE,
    limits::SOIL_MOISTURE,
];

const BITS: [u16; FIELD_COUNT] = [1 << 0, 1 << 1, 1 << 2, 1 << 3, 1 << 4, 1 << 5, 1 << 6];

/// Generador congruencial lineal determinista: no hace falta `rand` y el test
/// es reproducible entre ejecuciones y plataformas.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 32) as u32
    }

    fn unit(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.unit()
    }

    fn below(&mut self, limit: u32) -> u32 {
        self.next_u32() % limit
    }
}

/// Redondeo y validación esperados, calculados fuera del códec.
fn expected_scaled(value: Option<f32>, range: ApiRange, scale: f32) -> Option<f32> {
    let value = value.filter(|v| v.is_finite())?;
    let rounded = roundf(value * scale);
    if rounded.is_finite() && rounded >= range.min * scale && rounded <= range.max * scale {
        Some(rounded)
    } else {
        None
    }
}

/// Genera un valor con mezcla de casos válidos, límites, fuera de rango y no
/// finitos, para que la prueba cubra también las rutas de omisión.
fn random_value(lcg: &mut Lcg, range: ApiRange, scale: f32) -> Option<f32> {
    match lcg.below(12) {
        0 => None,
        1 => Some(f32::NAN),
        2 => Some(f32::INFINITY),
        3 => Some(f32::NEG_INFINITY),
        4 => Some(range.max + 0.5 / scale),
        5 => Some(range.min - 0.5 / scale),
        6 => Some(range.max + lcg.range(0.0, 50.0)),
        7 => Some(range.min - lcg.range(0.0, 50.0)),
        _ => Some(lcg.range(range.min, range.max)),
    }
}

fn field_slice(packet: &[u8; PACKET_SIZE], index: usize) -> &[u8] {
    let (start, end) = match index {
        0 => (10, 12),
        1 => (12, 14),
        2 => (14, 18),
        3 => (18, 20),
        4 => (20, 22),
        5 => (22, 24),
        6 => (24, 26),
        _ => unreachable!("field index out of range"),
    };
    &packet[start..end]
}

fn decoded_value(packet: &[u8; PACKET_SIZE], index: usize) -> i64 {
    match index {
        0 => i16::from_be_bytes([packet[10], packet[11]]) as i64,
        1 => u16::from_be_bytes([packet[12], packet[13]]) as i64,
        2 => u32::from_be_bytes([packet[14], packet[15], packet[16], packet[17]]) as i64,
        3 => u16::from_be_bytes([packet[18], packet[19]]) as i64,
        4 => u16::from_be_bytes([packet[20], packet[21]]) as i64,
        5 => u16::from_be_bytes([packet[22], packet[23]]) as i64,
        6 => u16::from_be_bytes([packet[24], packet[25]]) as i64,
        _ => unreachable!("field index out of range"),
    }
}

/// Construye el paquete esperado de forma independiente al códec.
fn expected_packet(
    values: &[Option<f32>; FIELD_COUNT],
    seq: u16,
    epoch: u32,
) -> ([u8; PACKET_SIZE], u16) {
    let mut packet = [0u8; PACKET_SIZE];
    packet[0] = COMPACT_BINARY_MAGIC;
    packet[1] = COMPACT_BINARY_VERSION;
    packet[2..4].copy_from_slice(&seq.to_be_bytes());
    packet[4..8].copy_from_slice(&epoch.to_be_bytes());

    let mut mask = 0u16;
    let mut scaled = [0i64; FIELD_COUNT];
    for index in 0..FIELD_COUNT {
        if let Some(value) = expected_scaled(values[index], RANGES[index], SCALES[index]) {
            mask |= BITS[index];
            scaled[index] = value as i64;
        }
    }

    packet[8..10].copy_from_slice(&mask.to_be_bytes());
    if mask & BITS[0] != 0 {
        packet[10..12].copy_from_slice(&(scaled[0] as i16).to_be_bytes());
    }
    if mask & BITS[1] != 0 {
        packet[12..14].copy_from_slice(&(scaled[1] as u16).to_be_bytes());
    }
    if mask & BITS[2] != 0 {
        packet[14..18].copy_from_slice(&(scaled[2] as u32).to_be_bytes());
    }
    if mask & BITS[3] != 0 {
        packet[18..20].copy_from_slice(&(scaled[3] as u16).to_be_bytes());
    }
    if mask & BITS[4] != 0 {
        packet[20..22].copy_from_slice(&(scaled[4] as u16).to_be_bytes());
    }
    if mask & BITS[5] != 0 {
        packet[22..24].copy_from_slice(&(scaled[5] as u16).to_be_bytes());
    }
    if mask & BITS[6] != 0 {
        packet[24..26].copy_from_slice(&(scaled[6] as u16).to_be_bytes());
    }

    packet[PACKET_SIZE - 1] = calc_crc8(&packet[..PACKET_SIZE - 1]);
    (packet, mask)
}

#[test]
fn crc8_still_matches_the_reference_vector() {
    assert_eq!(calc_crc8(b"123456789"), 0xF4);
}

#[test]
fn thousand_pseudorandom_states_match_the_independent_encoder() {
    let mut lcg = Lcg::new(0x5EED_2026);

    for case in 0..1_000 {
        let mut values = [None; FIELD_COUNT];
        for index in 0..FIELD_COUNT {
            values[index] = random_value(&mut lcg, RANGES[index], SCALES[index]);
        }

        let state = SensorState {
            temperature: values[0],
            humidity: values[1],
            brightness: values[2],
            ph: values[3],
            tds: values[4],
            atmosphere: values[5],
            soil_moisture: values[6],
        };
        let seq = lcg.next_u32() as u16;
        let epoch = lcg.next_u32();

        let (expected, expected_mask) = expected_packet(&values, seq, epoch);
        let mut packet = [0u8; PACKET_SIZE];
        let mask = encode_state(&state, seq, epoch, &mut packet);

        assert_eq!(
            mask, expected_mask,
            "presence mask mismatch in case {case} for {values:?}"
        );
        assert_eq!(
            packet, expected,
            "packet mismatch in case {case} for {values:?}"
        );
        assert_eq!(u16::from_be_bytes([packet[2], packet[3]]), seq);
        assert_eq!(
            u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]),
            epoch
        );
        assert_eq!(u16::from_be_bytes([packet[8], packet[9]]), mask);
        assert_eq!(
            packet[PACKET_SIZE - 1],
            calc_crc8(&packet[..PACKET_SIZE - 1])
        );

        for index in 0..FIELD_COUNT {
            let present = mask & BITS[index] != 0;
            assert_eq!(
                present,
                expected_scaled(values[index], RANGES[index], SCALES[index]).is_some(),
                "presence bit {index} mismatch in case {case}"
            );
            if !present {
                assert!(
                    field_slice(&packet, index).iter().all(|byte| *byte == 0),
                    "absent field {index} must stay zero in case {case}"
                );
            }
        }
    }
}

#[test]
fn half_lsb_rounds_away_from_zero_for_every_field() {
    // Todos los valores son fracciones binarias exactas: x.125 o x.25.
    let values = [
        Some(20.125f32),
        Some(49.125),
        Some(0.5),
        Some(7.125),
        Some(1234.5),
        Some(1013.25),
        Some(12.125),
    ];
    let expected = [2013i64, 4913, 1, 713, 1235, 10_133, 1213];

    let state = SensorState {
        temperature: values[0],
        humidity: values[1],
        brightness: values[2],
        ph: values[3],
        tds: values[4],
        atmosphere: values[5],
        soil_moisture: values[6],
    };
    let mut packet = [0u8; PACKET_SIZE];
    let mask = encode_state(&state, 1, 0, &mut packet);

    assert_eq!(mask, 0b0111_1111);
    for index in 0..FIELD_COUNT {
        assert_eq!(
            decoded_value(&packet, index),
            expected[index],
            "field {index}"
        );

        // Sesgo de truncamiento cero: en el punto medio positivo el resultado
        // es floor + 1, nunca floor.
        let raw = values[index].unwrap() * SCALES[index];
        let floor = floorf(raw) as i64;
        assert_eq!(
            decoded_value(&packet, index),
            floor + 1,
            "field {index} must round the positive half up, not truncate"
        );
    }

    // La mitad negativa también se aleja de cero, no hacia cero.
    let negative = SensorState {
        temperature: Some(-12.125),
        ..Default::default()
    };
    let mut packet = [0u8; PACKET_SIZE];
    assert_eq!(encode_state(&negative, 1, 0, &mut packet), 1);
    assert_eq!(decoded_value(&packet, 0), -1213);
}

#[test]
fn out_of_range_fields_clear_their_bit_and_leave_bytes_zero() {
    // Cada valor queda al menos una unidad escalada fuera del rango de la API.
    let outside = [
        Some(60.01f32),
        Some(100.01),
        Some(200_001.0),
        Some(14.01),
        Some(10_001.0),
        Some(1_100.1),
        Some(100.01),
    ];

    for (index, value) in outside.iter().enumerate() {
        let mut state = SensorState::default();
        match index {
            0 => state.temperature = *value,
            1 => state.humidity = *value,
            2 => state.brightness = *value,
            3 => state.ph = *value,
            4 => state.tds = *value,
            5 => state.atmosphere = *value,
            6 => state.soil_moisture = *value,
            _ => unreachable!(),
        }

        let mut packet = [0u8; PACKET_SIZE];
        let mask = encode_state(&state, 9, 0, &mut packet);
        assert_eq!(mask, 0, "field {index} must be omitted");
        assert!(
            field_slice(&packet, index).iter().all(|byte| *byte == 0),
            "field {index} must leave its bytes zero"
        );
        assert_eq!(
            packet[PACKET_SIZE - 1],
            calc_crc8(&packet[..PACKET_SIZE - 1])
        );
    }

    // Todos fuera de rango a la vez: máscara cero y datos en cero.
    let state = SensorState {
        temperature: outside[0],
        humidity: outside[1],
        brightness: outside[2],
        ph: outside[3],
        tds: outside[4],
        atmosphere: outside[5],
        soil_moisture: outside[6],
    };
    let (packet, mask) = {
        let mut packet = [0u8; PACKET_SIZE];
        let mask = encode_state(&state, 7, 0, &mut packet);
        (packet, mask)
    };
    assert_eq!(mask, 0);
    assert!(packet[10..26].iter().all(|byte| *byte == 0));

    // Por debajo del mínimo también se omite.
    let below = [
        Some(-20.01f32),
        Some(-0.01),
        Some(-1.0),
        Some(-0.01),
        Some(-1.0),
        Some(299.9),
        Some(-0.01),
    ];
    let state = SensorState {
        temperature: below[0],
        humidity: below[1],
        brightness: below[2],
        ph: below[3],
        tds: below[4],
        atmosphere: below[5],
        soil_moisture: below[6],
    };
    let mut packet = [0u8; PACKET_SIZE];
    assert_eq!(encode_state(&state, 7, 0, &mut packet), 0);
}

#[test]
fn exact_api_boundaries_are_accepted_and_encoded_verbatim() {
    let maximum = SensorState {
        temperature: Some(60.0),
        humidity: Some(100.0),
        brightness: Some(200_000.0),
        ph: Some(14.0),
        tds: Some(10_000.0),
        atmosphere: Some(1_100.0),
        soil_moisture: Some(100.0),
    };
    let expected_max = [6_000i64, 10_000, 200_000, 1_400, 10_000, 11_000, 10_000];
    let mut packet = [0u8; PACKET_SIZE];
    assert_eq!(encode_state(&maximum, 1, 0, &mut packet), 0b0111_1111);
    for (index, expected) in expected_max.iter().enumerate() {
        assert_eq!(decoded_value(&packet, index), *expected);
    }

    let minimum = SensorState {
        temperature: Some(-20.0),
        humidity: Some(0.0),
        brightness: Some(0.0),
        ph: Some(0.0),
        tds: Some(0.0),
        atmosphere: Some(300.0),
        soil_moisture: Some(0.0),
    };
    let expected_min = [-2_000i64, 0, 0, 0, 0, 3_000, 0];
    let mut packet = [0u8; PACKET_SIZE];
    assert_eq!(encode_state(&minimum, 1, 0, &mut packet), 0b0111_1111);
    for (index, expected) in expected_min.iter().enumerate() {
        assert_eq!(decoded_value(&packet, index), *expected);
    }
}

#[test]
fn sequence_and_epoch_round_trip_big_endian() {
    for (seq, epoch) in [
        (0u16, 0u32),
        (1, 1),
        (0xABCD, 0x1234_5678),
        (u16::MAX, u32::MAX),
    ] {
        let mut packet = [0u8; PACKET_SIZE];
        assert_eq!(
            encode_state(&SensorState::default(), seq, epoch, &mut packet),
            0
        );
        assert_eq!(u16::from_be_bytes([packet[2], packet[3]]), seq);
        assert_eq!(
            u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]),
            epoch
        );
        assert_eq!(packet[0], COMPACT_BINARY_MAGIC);
        assert_eq!(packet[1], COMPACT_BINARY_VERSION);
        assert_eq!(
            packet[PACKET_SIZE - 1],
            calc_crc8(&packet[..PACKET_SIZE - 1])
        );
    }
}

#[test]
fn value_just_outside_the_raw_range_that_rounds_inside_is_kept() {
    // 60.004 °C redondea a 6000 centésimas, un valor de API válido.
    let state = SensorState {
        temperature: Some(60.004),
        ..Default::default()
    };
    let mut packet = [0u8; PACKET_SIZE];
    assert_eq!(encode_state(&state, 0, 0, &mut packet), 1);
    assert_eq!(decoded_value(&packet, 0), 6_000);
}
