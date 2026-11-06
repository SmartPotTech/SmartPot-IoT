//! Capacidades estáticas que el firmware publica en el broker al conectarse.
//!
//! El JSON se publica retenido en `smartpot/v1/{cropId}/capabilities` y declara
//! hardware, formatos de telemetría, rangos de sensores y actuadores. Se genera
//! con `concat!` para que viva en flash y no consuma heap ni pila. El perfil es
//! el que compila este firmware: si una función se desactiva con `features`, su
//! sensor o actuador no debe declararse aquí.

pub const CAPABILITIES_JSON: &str = concat!(
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
    "{\"key\":\"temperature\",\"unit\":\"C\",\"resolution\":0.01,\"range\":[-20,60]},",
    "{\"key\":\"humidity\",\"unit\":\"%\",\"resolution\":0.01,\"range\":[0,100]},",
    "{\"key\":\"brightness\",\"unit\":\"lux\",\"resolution\":1,\"range\":[0,200000]},",
    "{\"key\":\"ph\",\"unit\":\"pH\",\"resolution\":0.01,\"range\":[0,14]},",
    "{\"key\":\"tds\",\"unit\":\"ppm\",\"resolution\":1,\"range\":[0,10000]},",
    "{\"key\":\"atmosphere\",\"unit\":\"hPa\",\"resolution\":0.1,\"range\":[300,1100]},",
    "{\"key\":\"soilMoisture\",\"unit\":\"%\",\"resolution\":0.01,\"range\":[0,100]}",
    "],",
    "\"actuators\":[\"WATER_PUMP\",\"UV_LIGHT\",\"FAN\",\"HUMIDIFIER\"]",
    "}"
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
        assert_eq!(CAPABILITIES_JSON.matches("\"key\":").count(), 7);
        assert_eq!(CAPABILITIES_JSON.matches("\"range\":").count(), 7);
    }
}
