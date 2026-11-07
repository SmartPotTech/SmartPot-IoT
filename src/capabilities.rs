//! Capacidades estáticas que el firmware publica en el broker al conectarse.
//!
//! El JSON se publica retenido en `smartpot/v1/{cropId}/capabilities` y declara
//! hardware, formatos de telemetría, rangos de sensores y actuadores. Se genera
//! con `concat!` para que viva en flash y no consuma heap ni pila. El perfil es
//! el que compila este firmware: si una función se desactiva con `features`, su
//! sensor o actuador no debe declararse aquí.

macro_rules! capabilities {
    ($sensors:expr, $actuators:expr) => {
        concat!(
            "{",
            "\"schema\":1,",
            "\"device\":{",
            "\"firmware\":\"smartpot-iot/",
            env!("CARGO_PKG_VERSION"),
            "\",",
            "\"hardware\":\"esp32-devkit-c-v4\",",
            "\"telemetry\":{",
            "\"formats\":[\"json-v1\",\"compact-v2\"],",
            "\"compactBytes\":27",
            "}",
            "},",
            "\"sensors\":[",
            $sensors,
            "],",
            "\"actuators\":[",
            $actuators,
            "]",
            "}"
        )
    };
}

/// Placa compartida con el firmware MicroPython: mismos sensores, rangos y
/// actuadores que declara `capabilities_payload` en `smartpot_client.py`.
#[cfg(not(feature = "board-physical"))]
pub const CAPABILITIES_JSON: &str = capabilities!(
    concat!(
        "{\"key\":\"temperature\",\"unit\":\"C\",\"resolution\":0.01,\"range\":[-20,60]},",
        "{\"key\":\"humidity\",\"unit\":\"%\",\"resolution\":0.01,\"range\":[0,100]},",
        "{\"key\":\"brightness\",\"unit\":\"lux\",\"resolution\":1,\"range\":[0,2000]},",
        "{\"key\":\"ph\",\"unit\":\"pH\",\"resolution\":0.01,\"range\":[0,14]},",
        "{\"key\":\"tds\",\"unit\":\"ppm\",\"resolution\":1,\"range\":[0,3000]},",
        "{\"key\":\"soilMoisture\",\"unit\":\"%\",\"resolution\":0.01,\"range\":[0,100]}"
    ),
    "\"WATER_PUMP\",\"UV_LIGHT\",\"FAN\""
);

/// Prototipo físico (BME280, BH1750 y humidificador).
#[cfg(feature = "board-physical")]
pub const CAPABILITIES_JSON: &str = capabilities!(
    concat!(
        "{\"key\":\"temperature\",\"unit\":\"C\",\"resolution\":0.01,\"range\":[-20,60]},",
        "{\"key\":\"humidity\",\"unit\":\"%\",\"resolution\":0.01,\"range\":[0,100]},",
        "{\"key\":\"brightness\",\"unit\":\"lux\",\"resolution\":1,\"range\":[0,200000]},",
        "{\"key\":\"ph\",\"unit\":\"pH\",\"resolution\":0.01,\"range\":[0,14]},",
        "{\"key\":\"tds\",\"unit\":\"ppm\",\"resolution\":1,\"range\":[0,10000]},",
        "{\"key\":\"atmosphere\",\"unit\":\"hPa\",\"resolution\":0.1,\"range\":[300,1100]},",
        "{\"key\":\"soilMoisture\",\"unit\":\"%\",\"resolution\":0.01,\"range\":[0,100]}"
    ),
    "\"WATER_PUMP\",\"UV_LIGHT\",\"FAN\",\"HUMIDIFIER\""
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_fit_in_a_single_mqtt_publish() {
        assert!(CAPABILITIES_JSON.len() < 2_048);
    }

    #[test]
    fn capabilities_declare_the_expected_contract() {
        assert!(CAPABILITIES_JSON.contains("\"schema\":1"));
        assert!(CAPABILITIES_JSON.contains("\"device\":{"));
        assert!(CAPABILITIES_JSON.contains("\"firmware\":\"smartpot-iot/"));
        assert!(CAPABILITIES_JSON.contains("\"compactBytes\":27"));
        assert!(CAPABILITIES_JSON.contains("\"UV_LIGHT\""));
        assert_eq!(
            CAPABILITIES_JSON.matches("\"key\":").count(),
            if cfg!(feature = "board-physical") {
                7
            } else {
                6
            }
        );
        assert_eq!(
            CAPABILITIES_JSON.matches("\"range\":").count(),
            CAPABILITIES_JSON.matches("\"key\":").count()
        );
    }
}
