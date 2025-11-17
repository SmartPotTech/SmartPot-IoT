# Proyecto de sistema de control de riego e índices en plantas

[![Pylint](https://github.com/SmartPotTech/SmartPot-IoT/actions/workflows/pylint.yml/badge.svg)](https://github.com/SmartPotTech/SmartPot-IoT/actions/workflows/pylint.yml)
[![CodeQL Advanced](https://github.com/SmartPotTech/SmartPot-IoT/actions/workflows/codeql.yml/badge.svg)](https://github.com/SmartPotTech/SmartPot-IoT/actions/workflows/codeql.yml)

**Descripción General**

SmartPot es un sistema de software diseñado para la gestión y automatización de jardines hidropónicos, con enfoque en
cultivos de tomates y lechugas. Este proyecto integra tecnologías de Internet de las Cosas (IoT) para ofrecer una
solución completa y eficiente en el manejo de cultivos.

Alcance del Proyecto:

- Simulación Avanzada: Utilización de la plataforma Wokwi para emular sensores y actuadores, permitiendo un desarrollo y
  pruebas precisas sin necesidad de hardware físico inicial.
- Solución Integral de Software: Desarrollo de una plataforma que incluye base de datos, backend eficiente, frontend
  intuitivo, capacidades de análisis de datos y sistemas de automatización de procesos.
- Monitoreo en Tiempo Real: Implementación de sensores virtuales para medir parámetros críticos como humedad, luz,
  temperatura, pH y niveles de nutrientes.
- Control Automatizado: Desarrollo de algoritmos para ajustar automáticamente las condiciones del cultivo basados en los
  datos recopilados y en los estándares de cada cultivo.
- Cabe destacar que en nuestro prototipo tendremos 2 tipos de plantas(lechuga y tomates) para específicamente jardines
  hidropónicos, podremos añadir sensores o eliminar sensores al igual que los actuadores.

## Simulación

[Simulación en Wokwi](https://wokwi.com/projects/408863167711709185)

## Prueba

### **Configuración de Wokwi en PyCharm para ESP32**

Este documento describe cómo configurar **Wokwi** en **PyCharm** para simular un ESP32, cargar el firmware y ejecutar el
código de Python en paralelo con la simulación.

### 1. **Crear el Proyecto en PyCharm**

1. Abre **PyCharm** y crea un nuevo proyecto de Python.

2. Asegúrate de que tu proyecto esté configurado con el entorno virtual adecuado para tu versión de Python (se
   recomienda Python 3.7 o superior).

3. Instala las dependencias necesarias. Abre la terminal de PyCharm y ejecuta el siguiente comando para instalar
   `mpremote`:
   ```bash
   pip install mpremote
   ```

---

### 2. **Configurar Wokwi para ESP32**

Para usar **Wokwi** con **ESP32**, necesitas asegurarte de tener los archivos de configuración correctos en tu proyecto:

#### Archivos Necesarios:

1. **wokwi.toml**: Este archivo define la configuración de la simulación de Wokwi. Asegúrate de que el archivo
   `wokwi.toml` esté ubicado en el directorio `esp32/` de tu proyecto. Este archivo es esencial para configurar el
   entorno de simulación de **Wokwi**.

2. **diagram.json**: Este archivo define el diagrama de conexiones para tu simulación. Debe estar ubicado en
   `esp32/diagram.json`.

3. **Firmware ESP32**: Necesitas el archivo binario de firmware compatible con **ESP32**. Un ejemplo es
   `ESP32_GENERIC-20240602-v1.23.0.bin`. Este archivo debe ser cargado en la simulación para ejecutar el código en el
   microcontrolador.

#### Estructura del Proyecto:

Tu proyecto debería tener una estructura similar a esta:

```
SmarPot/
│
├── esp32/
│   ├── wokwi.toml
│   ├── diagram.json
│   └── ESP32_GENERIC-20240602-v1.23.0.bin
│
├── main.py
```

### 3. **Cargar el Firmware en Wokwi**

1. Asegúrate de que el firmware **ESP32_GENERIC-20240602-v1.23.0.bin** esté correctamente configurado en tu simulador
   Wokwi.

2. En **Wokwi**, carga este firmware para emular el comportamiento del **ESP32**.

---

### 4. **Ejecutar el Código en Paralelo con la Simulación**

Para ejecutar tu código de **Python** en paralelo con la simulación, debes utilizar el comando de **mpremote**.

#### Comando para Ejecutar en Paralelo:

En la terminal de PyCharm, usa el siguiente comando para conectarte al puerto **RFC2217** y ejecutar tu script
`main.py`:

```bash
python -m mpremote connect port:rfc2217://localhost:4000 run main.py
```

Este comando se conecta al puerto de simulación (`localhost:4000`), carga el firmware y ejecuta el script **`main.py`**
que contiene el código Python para el ESP32.

- **port:rfc2217://localhost:4000**: Especifica la conexión RFC2217 en el puerto `4000`.
- **run main.py**: Ejecuta el archivo `main.py` en el ESP32.

---

### 5. **Configuración del Entorno en PyCharm**

Para que PyCharm ejecute correctamente el comando en paralelo, sigue estos pasos:

1. **Configurar un Script de Ejecución en PyCharm**:
    - En PyCharm, ve a `Run > Edit Configurations`.
    - Haz clic en el ícono de **"+"** y selecciona **"Python"**.
    - En el campo **"Script path"**, selecciona el archivo `main.py`.
    - En el campo **"Parameters"**, escribe el comando de conexión y ejecución:
      ```bash
      -m mpremote connect port:rfc2217://localhost:4000 run main.py
      ```
    - Asegúrate de que el entorno de ejecución esté configurado correctamente (por ejemplo, seleccionando el entorno
      virtual adecuado).

2. **Ejecutar en Paralelo**:
    - Ahora puedes ejecutar tu código directamente desde PyCharm usando el botón de **"Run"** o **"Debug"**.
    - El código se ejecutará en paralelo con la simulación, permitiéndote interactuar con el ESP32 simulado mientras el
      código se ejecuta.

---
