<!-- portada
eyebrow: Documentación del componente
titulo: SmartPot-IoT
acento: IoT
subtitulo: El firmware de un cultivo real
bajada: MicroPython en un ESP32, físico o simulado en Wokwi: sensores, pantalla, actuadores y contrato MQTT v1 con TLS, con su circuito, su ciclo principal, su configuración y sus pruebas.
documento: SmartPot-IoT
version: 1.0 · septiembre 2026
equipo: SmartPotTech
proyecto: smartpot.app
-->

# SmartPot-IoT

## Ficha del documento

| Campo | Valor |
| --- | --- |
| Proyecto | SmartPot · [smartpot.app](https://smartpot.app) |
| Componente | [SmartPot-IoT](https://github.com/SmartPotTech/SmartPot-IoT) |
| Versión | 1.0 · septiembre 2026 |
| Alcance | Circuito, módulos del firmware, ciclo principal, comandos, conexión segura, configuración y pruebas |
| Documentación de la plataforma | [Documentación técnica](https://github.com/SmartPotTech/.github/blob/main/docs/SmartPot_Technical_Documentation.md), [recorrido del proyecto](https://github.com/SmartPotTech/.github/blob/main/docs/SmartPot_Project_Journey.md), [ciclo de vida](https://github.com/SmartPotTech/.github/blob/main/docs/SmartPot_Software_Lifecycle.md) y [diagramas generales](https://github.com/SmartPotTech/.github/blob/main/docs/README.md#diagramas-generales) |
| Mantenimiento | Se genera desde `docs/` de este repositorio con las herramientas de `.github/docs/tools`; se actualiza con cada cambio del componente |

<!-- parte: PARTE I | El componente -->

## 1. Propósito

### En palabras simples

SmartPot-IoT es el programa que corre en el dispositivo de un **cultivo real**: un ESP32 con sus sensores, sus actuadores y una pantalla. Mide, muestra, publica las lecturas en el broker de SmartPot y obedece las órdenes que llegan, confirmando cada una. No decide nada por su cuenta: la API y el asistente deciden. El mismo firmware corre en una placa física o en [Wokwi](https://wokwi.com/projects/408863167711709185), y en los dos casos el cultivo es real para la plataforma; los cultivos virtuales, en cambio, los simula SmartPot-DataGenerator.

## 2. Arquitectura del componente

<!-- diagrama: SmartPot_IoT_Global_Component | titulo=SmartPot-IoT por dentro | lamina=H -->
```mermaid
%%{init: {"theme": "base", "fontFamily": "Segoe UI, Arial, sans-serif", "themeVariables": {"fontFamily": "Segoe UI, Arial, sans-serif", "fontSize": "15px", "primaryColor": "#DDF5EA", "primaryTextColor": "#17261F", "primaryBorderColor": "#067A52", "secondaryColor": "#E3F2FB", "secondaryTextColor": "#17261F", "secondaryBorderColor": "#1F6FA0", "tertiaryColor": "#F2F7F4", "tertiaryTextColor": "#17261F", "tertiaryBorderColor": "#D5E3DC", "lineColor": "#5B6B63", "textColor": "#17261F", "mainBkg": "#DDF5EA", "nodeBorder": "#067A52", "clusterBkg": "#F7FAF8", "clusterBorder": "#D5E3DC", "edgeLabelBackground": "#FFFFFF", "actorBkg": "#067A52", "actorBorder": "#0B3D2B", "actorTextColor": "#FFFFFF", "actorLineColor": "#5B6B63", "signalColor": "#17261F", "signalTextColor": "#17261F", "labelBoxBkgColor": "#0B3D2B", "labelBoxBorderColor": "#0B3D2B", "labelTextColor": "#FFFFFF", "loopTextColor": "#0B3D2B", "noteBkgColor": "#FDF4DD", "noteBorderColor": "#C98D12", "noteTextColor": "#17261F", "activationBkgColor": "#DDF5EA", "activationBorderColor": "#067A52", "attributeBackgroundColorOdd": "#FFFFFF", "attributeBackgroundColorEven": "#F2F7F4"}, "layout": "elk", "elk": {"nodePlacementStrategy": "BRANDES_KOEPF", "mergeEdges": false, "cycleBreakingStrategy": "GREEDY"}}}%%
flowchart LR
  subgraph esp["ESP32 · MicroPython 1.23 · físico o en Wokwi"]
    direction TB
    main["main.py<br/>WiFi · NTP · ciclo principal"]
    config["config.py<br/>WIFI · SMARTPOT (no se versiona)"]
    client["smartpot_client.py<br/>SmartPotClient · tópicos v1<br/>telemetría · comandos · ACK · estado"]
    sensors["sensors.py<br/>AtmosphereSensor DHT22<br/>Light · PH · TDS · SoilMoisture (ADC)"]
    actuators["actuators.py<br/>ActuatorBank · apagado por duración"]
    display["display.py<br/>LCDDisplay 20 × 4 por I2C"]
    utils["utils.py<br/>hora por NTP · tabla por consola"]
    ca["ca.crt<br/>CA de SmartPot"]
  end
  subgraph hw["Circuito"]
    direction TB
    dht["DHT22 · GPIO 15"]
    adc["Luz 34 · pH 35 · TDS 32 · sustrato 33"]
    outs["Bomba 19 · luz de cultivo 18 · ventilador 5"]
    lcd["LCD · SCL 16 · SDA 17"]
  end
  broker["mqtt.smartpot.app:8883<br/>TLS 1.2+"]
  main --> config & client & sensors & actuators & display & utils
  client --> ca
  sensors --> dht & adc
  actuators --> outs
  display --> lcd
  client <-->|"usuario cropId · clave del dispositivo"| broker
  classDef leaf fill:#DDF5EA,stroke:#067A52,color:#17261F
  classDef water fill:#E3F2FB,stroke:#1F6FA0,color:#17261F
  classDef sun fill:#FDF4DD,stroke:#C98D12,color:#17261F
  classDef clay fill:#FBE9E1,stroke:#B85A38,color:#17261F
  classDef core fill:#067A52,stroke:#0B3D2B,color:#FFFFFF
  classDef deep fill:#0B3D2B,stroke:#06281C,color:#FFFFFF
  classDef muted fill:#F2F7F4,stroke:#5B6B63,color:#17261F
  class main core
  class config,ca,utils muted
  class client,sensors,actuators,display leaf
  class dht,adc,outs,lcd clay
  class broker water
```

| Componente | Pin ESP32 | Escala enviada |
| --- | --- | --- |
| DHT22 (temperatura y humedad del aire) | GPIO 15 | °C y % |
| Sensor de luz (potenciómetro en Wokwi) | GPIO 34 | 0–2000 lux |
| Sensor de pH | GPIO 35 | 0–14 |
| Sensor de TDS | GPIO 32 | 0–3000 ppm |
| Humedad del sustrato | GPIO 33 | 0–100 % |
| Bomba de agua | GPIO 19 | `WATER_PUMP` |
| Luz de cultivo | GPIO 18 | `UV_LIGHT` |
| Ventilador | GPIO 5 | `FAN` |
| LCD 20×4 I2C | SCL 16 · SDA 17 | — |

<!-- parte: PARTE II | Funcionamiento -->

## 3. Ciclo principal

<!-- diagrama: SmartPot_IoT_01_Main_Loop | titulo=Ciclo principal del firmware -->
```mermaid
%%{init: {"theme": "base", "fontFamily": "Segoe UI, Arial, sans-serif", "themeVariables": {"fontFamily": "Segoe UI, Arial, sans-serif", "fontSize": "15px", "primaryColor": "#DDF5EA", "primaryTextColor": "#17261F", "primaryBorderColor": "#067A52", "secondaryColor": "#E3F2FB", "secondaryTextColor": "#17261F", "secondaryBorderColor": "#1F6FA0", "tertiaryColor": "#F2F7F4", "tertiaryTextColor": "#17261F", "tertiaryBorderColor": "#D5E3DC", "lineColor": "#5B6B63", "textColor": "#17261F", "mainBkg": "#DDF5EA", "nodeBorder": "#067A52", "clusterBkg": "#F7FAF8", "clusterBorder": "#D5E3DC", "edgeLabelBackground": "#FFFFFF", "actorBkg": "#067A52", "actorBorder": "#0B3D2B", "actorTextColor": "#FFFFFF", "actorLineColor": "#5B6B63", "signalColor": "#17261F", "signalTextColor": "#17261F", "labelBoxBkgColor": "#0B3D2B", "labelBoxBorderColor": "#0B3D2B", "labelTextColor": "#FFFFFF", "loopTextColor": "#0B3D2B", "noteBkgColor": "#FDF4DD", "noteBorderColor": "#C98D12", "noteTextColor": "#17261F", "activationBkgColor": "#DDF5EA", "activationBorderColor": "#067A52", "attributeBackgroundColorOdd": "#FFFFFF", "attributeBackgroundColorEven": "#F2F7F4"}}}%%
flowchart TB
  boot(["Encendido"]) --> wifi{"¿WiFi conectado?<br/>hasta 30 s"}
  wifi -->|"No"| wait["Espera 10 s"] --> wifi
  wifi -->|"Sí"| ntp["Hora por NTP"]
  ntp --> mqtt{"¿Conectado al broker?"}
  mqtt -->|"No"| connect["CONNECT con TLS y ca.crt<br/>última voluntad offline<br/>status online · suscripción a commands"]
  connect --> mqtt
  mqtt -->|"Sí"| poll["poll: atiende los comandos que llegaron"]
  poll --> tick["bank.tick: apaga los actuadores<br/>cuya duración venció"]
  tick --> due{"¿Toca leer?<br/>cada interval_seconds"}
  due -->|"Sí"| read["Lee los sensores · pantalla LCD<br/>publica telemetry"]
  due -->|"No"| sleep["Espera 0,5 s"]
  read --> sleep --> mqtt
  poll -.->|"OSError"| lost["Sin conexión: espera 10 s<br/>y vuelve a conectar"] -.-> mqtt
  classDef leaf fill:#DDF5EA,stroke:#067A52,color:#17261F
  classDef water fill:#E3F2FB,stroke:#1F6FA0,color:#17261F
  classDef sun fill:#FDF4DD,stroke:#C98D12,color:#17261F
  classDef clay fill:#FBE9E1,stroke:#B85A38,color:#17261F
  classDef core fill:#067A52,stroke:#0B3D2B,color:#FFFFFF
  classDef deep fill:#0B3D2B,stroke:#06281C,color:#FFFFFF
  classDef muted fill:#F2F7F4,stroke:#5B6B63,color:#17261F
  class boot core
  class wifi,mqtt,due sun
  class wait,sleep,lost muted
  class ntp,connect,poll,tick,read leaf
```

## 4. Comandos

<!-- diagrama: SmartPot_IoT_02_Command_Handling | titulo=Cómo atiende una orden -->
```mermaid
%%{init: {"theme": "base", "fontFamily": "Segoe UI, Arial, sans-serif", "themeVariables": {"fontFamily": "Segoe UI, Arial, sans-serif", "fontSize": "15px", "primaryColor": "#DDF5EA", "primaryTextColor": "#17261F", "primaryBorderColor": "#067A52", "secondaryColor": "#E3F2FB", "secondaryTextColor": "#17261F", "secondaryBorderColor": "#1F6FA0", "tertiaryColor": "#F2F7F4", "tertiaryTextColor": "#17261F", "tertiaryBorderColor": "#D5E3DC", "lineColor": "#5B6B63", "textColor": "#17261F", "mainBkg": "#DDF5EA", "nodeBorder": "#067A52", "clusterBkg": "#F7FAF8", "clusterBorder": "#D5E3DC", "edgeLabelBackground": "#FFFFFF", "actorBkg": "#067A52", "actorBorder": "#0B3D2B", "actorTextColor": "#FFFFFF", "actorLineColor": "#5B6B63", "signalColor": "#17261F", "signalTextColor": "#17261F", "labelBoxBkgColor": "#0B3D2B", "labelBoxBorderColor": "#0B3D2B", "labelTextColor": "#FFFFFF", "loopTextColor": "#0B3D2B", "noteBkgColor": "#FDF4DD", "noteBorderColor": "#C98D12", "noteTextColor": "#17261F", "activationBkgColor": "#DDF5EA", "activationBorderColor": "#067A52", "attributeBackgroundColorOdd": "#FFFFFF", "attributeBackgroundColorEven": "#F2F7F4"}}}%%
sequenceDiagram
  autonumber
  participant A as SmartPot-API
  participant B as Broker
  participant C as SmartPotClient
  participant K as ActuatorBank
  A->>B: commands {id, actuator, action, durationSeconds} (QoS 1)
  B->>C: mensaje en smartpot/v1/{cropId}/commands
  C->>C: parse_command valida la carga
  C->>K: execute(command, ahora)
  alt Actuador instalado
    K->>K: enciende o apaga el pin
    K-->>C: EXECUTED · «Bomba de agua encendida por 15 s»
  else No existe en el dispositivo
    K-->>C: FAILED · «Este dispositivo no tiene HUMIDIFIER»
  end
  C->>B: commands/ack {id, status, message} (QoS 1)
  B->>A: confirma el comando
  Note over K: Con durationSeconds, tick lo apaga solo<br/>al cumplirse el tiempo
```

## 5. Conexión segura

| Parámetro | Valor |
| --- | --- |
| Broker | `mqtt.smartpot.app:8883`, MQTT sobre TLS 1.2 o superior |
| Certificado | Firmado por la CA propia de SmartPot; el firmware lo verifica con `ca.crt` |
| Usuario y clave | El id del cultivo y la clave del dispositivo, que la PWA muestra una sola vez al crear el cultivo real o al rotarla |
| Client id | `smartpot-<cropId>` |
| Estado | `online` retenido al conectar y `offline` como última voluntad |

> [!WARNING]
> **Wokwi.** Deja tu copia del proyecto como privada: quien vea `config.py` puede publicar como tu cultivo. Si se filtra, genera una nueva clave desde la pestaña Dispositivo.

<!-- parte: PARTE III | Operación -->

## 6. Configuración

La PWA genera `config.py` en su guía de conexión, con la red WiFi (la tuya o `Wokwi-GUEST`) y el bloque `SMARTPOT` del cultivo. `config.py` no se versiona; `config.example.py` es la plantilla.

| Clave | Uso |
| --- | --- |
| `WIFI.ssid`, `WIFI.password` | Red a la que se conecta el ESP32 |
| `SMARTPOT.crop_id`, `device_key` | Cuenta MQTT del cultivo real |
| `SMARTPOT.host`, `port`, `tls`, `ca_file` | Broker y verificación del certificado |
| `SMARTPOT.interval_seconds` | Segundos entre lecturas (30 por defecto) |

## 7. Pruebas

`uv run ruff check .` y `uv run pytest`: 13 pruebas con CPython y módulos de MicroPython simulados sobre el contrato MQTT, los comandos y su ACK, el apagado por tiempo de los actuadores, la escala de los sensores y el respaldo de TLS en versiones anteriores de MicroPython.

## 8. Puesta en marcha

| Dónde | Pasos |
| --- | --- |
| Placa física | Arma el circuito, graba MicroPython 1.23, copia `fs/` con `mpremote`, agrega `config.py` y `ca.crt` y enciende |
| Wokwi en el navegador | Abre el proyecto, pega `config.py` y `ca.crt` y ejecuta; la red `Wokwi-GUEST` tiene salida a internet |
| Wokwi local | `uv sync`, inicia la simulación con `wokwi.toml` y ejecuta `uv run python start.py` |
