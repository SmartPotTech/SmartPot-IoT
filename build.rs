fn main() {
    load_shared_config();
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}

/// Lee `config.json` (mismos bloques y claves que el firmware MicroPython:
/// WIFI, SMARTPOT y BROKER) y lo pasa al firmware como variables de
/// compilación. Una variable de entorno con el mismo nombre tiene prioridad,
/// para que CI y la simulación puedan sobrescribir valores sueltos.
fn load_shared_config() {
    println!("cargo:rerun-if-changed=config.json");
    let Ok(text) = std::fs::read_to_string("config.json") else {
        return;
    };
    let config: serde_json::Value =
        serde_json::from_str(&text).expect("config.json is not valid JSON");
    let wifi = &config["WIFI"];
    let smartpot = &config["SMARTPOT"];

    let text_of = |value: &serde_json::Value| value.as_str().map(str::to_owned);
    let number_of = |value: &serde_json::Value| value.as_u64().map(|n| n.to_string());
    let flag_of =
        |value: &serde_json::Value| value.as_bool().map(|b| if b { "1" } else { "0" }.to_owned());

    emit("SSID", text_of(&wifi["ssid"]));
    emit("PASSWORD", text_of(&wifi["password"]));
    emit("CROP_ID", text_of(&smartpot["crop_id"]));
    emit("DEVICE_KEY", text_of(&smartpot["device_key"]));
    emit("BROKER_HOST", text_of(&smartpot["host"]));
    emit("BROKER_PORT", number_of(&smartpot["port"]));
    emit("USE_TLS", flag_of(&smartpot["tls"]));
    emit("TELEMETRY_INTERVAL_SECS", number_of(&smartpot["interval_seconds"]));
    emit("WATCHDOG_TIMEOUT_SECS", number_of(&smartpot["watchdog_seconds"]));
    emit("ACTUATOR_ACTIVE_LOW", flag_of(&smartpot["active_low"]));
    emit("HEAP_MIN_FREE", number_of(&smartpot["heap_min_free"]));
    emit("SNTP_SERVER", text_of(&smartpot["ntp_server"]));
    // El firmware activa el paquete compacto v2 si la variable existe.
    if smartpot["compact_telemetry"].as_bool() == Some(true) {
        emit("COMPACT_TELEMETRY", Some("1".to_owned()));
    }
    // Una directiva de cargo no admite saltos de línea: el PEM se pasa en una
    // sola línea, y el decodificador ignora los espacios entre las cabeceras.
    let ca = text_of(&config["BROKER"]["CA_CRT"]).map(|pem| pem.split_whitespace().collect::<Vec<_>>().join(""));
    emit(
        "CA_CERT_PEM",
        ca.map(|pem| {
            pem.replace("-----BEGINCERTIFICATE-----", "-----BEGIN CERTIFICATE-----")
                .replace("-----ENDCERTIFICATE-----", "-----END CERTIFICATE-----")
        }),
    );
}

fn emit(key: &str, value: Option<String>) {
    println!("cargo:rerun-if-env-changed={key}");
    if std::env::var_os(key).is_some() {
        return;
    }
    if let Some(value) = value {
        println!("cargo:rustc-env={key}={value}");
    }
}
