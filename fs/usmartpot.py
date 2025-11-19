import ujson
import requests
import ussl
import socket
import time
import utilitys
import gc

class Ubot:
    def __init__(self, email, password, crop):
        self.base_url_mw = "https://api.smartpot.app"
        self.email = email
        self.password = password
        self.crop = crop
        self.token = None
        self.commands = {}
        self.sleep_btw_updates = 3

        self.login()
        
    

    def _headers(self, auth=True):
        """Genera los headers para cada request."""
        h = {
            "Content-Type": "application/json"

        }
        if auth and self.token:
            h['Authorization'] = f"SmartPot-OAuth {self.token}"
        return h

    def _request(self, method, url, data=None, auth=True):
        """
        Maneja de forma centralizada el envío HTTP.
        Muestra:
        - URL
        - STATUS
        - BODY (solo si hay error)
        """
        try:
            headers = self._headers(auth)
            
            # Debug: Mostrar lo que se está enviando
            print(f"[DEBUG] Headers: {headers}")
            if data:
                print(f"[DEBUG] Data: {data}")
            
            if data is not None:
                response = requests.request(method, url, data=data.encode("utf-8"), headers=headers)
            else:
                response = requests.request(method, url, headers=headers)

            status = response.status_code

            # Log pequeño (centralizado)
            print(f"[HTTP] {method} {url}")
            print(f"[STATUS] {status}")

            text = response.text

            # Si es success intentamos parsear JSON
            if 200 <= status < 300:
                try:
                    return response.json()
                except:
                    print("[WARN] Respuesta no es JSON")
                    return text

            # Si es error, mostramos response unificada
            print("[ERROR] Respuesta del servidor:")
            print(text)
            return None

        except Exception as e:
            print(f"[EXCEPTION] Error en request {method}: {e}")
            return None

    def login(self):
        email=self.email
        password=self.password
        if not isinstance(email, str) or not isinstance(password, str):
            raise TypeError("El email y la contraseña deben ser cadenas de texto.")

        if "@" not in email:
            raise ValueError("Email inválido.")

        # Asegurarse de que el JSON esté bien formateado
        payload = ujson.dumps({
            "email": email,
            "password": password
        })
        

        url = f"{self.base_url_mw}/auth/login"

        response = self._request("POST", url, data=payload, auth=False)

        if not response or "token" not in response:
            raise RuntimeError("No se recibió un token en el login.")

        self.token = response["token"]
        print(f"[SUCCESS] Login exitoso. Token recibido.")

    def refresh_token(self):
        """Refresca el token realizando el login nuevamente."""
        try:
            self.login()
            print("Token refrescado correctamente.")
            return True
        except Exception as e:
            print(f"Error refrescando token: {e}")
            return False

    def read_commands(self):
        try:
            url = f"{self.base_url_mw}/Commands/crop/{self.crop}"

            data = self._request("GET", url)

            if not data:
                return []

            if isinstance(data, list):
                return data

            if isinstance(data, dict) and "data" in data:
                return data["data"]

            return []

        except Exception as e:
            print("ERROR leyendo comandos:", e)
            return []

    def mark_executed(self, command_id, result):
        try:
            url = f"{self.base_url_mw}/Commands/{command_id}/run/{result}"
            r = self._request("POST", url)
            print(f"Comando {command_id} marcado como EXECUTED")
            return True
        except Exception as e:
            print("Error ejecutando:", e)
            return False

    def register(self, commandType, handler):
        self.commands[commandType.upper()] = handler

    def listen(self, limit):
        start_time = time.time()
        while True:
            current_time = time.time()
            elapsed_time = current_time - start_time
            
            if elapsed_time > limit:  # Si se ha superado el límite de tiempo
                break  # Salir del bucle

            self.read_once()
            gc.collect()
            time.sleep(self.sleep_btw_updates)

    def read_once(self):

        commands = self.read_commands()
        print(commands)
        if not commands:
            return

        pending = [c for c in commands if c["status"] == "PENDING"]

        for cmd in pending:
            cmd_type = cmd["commandType"].upper()
            print("Comando recibido:", cmd_type)

            if cmd_type in self.commands:
                try:
                    self.commands[cmd_type](cmd)
                except Exception as e:
                    print("Error en handler:", e)

                self.mark_executed(cmd["id"],"SUCESSS")
            else:
                self.mark_executed(cmd["id"],"ERROR")
                print("Comando no soportado:", cmd_type)

    def create_record(self, temp, humidity_air, light, ph, tds, humidity_soil):
        if not self.token:
            print("No hay token, no se puede crear record.")
            return None

        if not self.crop:
            raise ValueError("El ID del cultivo debe ser válidos.")

        payload = ujson.dumps({
            "measures": ({
                "atmosphere": str(temp),
                "brightness": str(light),
                "temperature": str(temp),
                "ph": str(ph),
                "tds": str(tds),
                "humidity": str(humidity_air)
            }),
            "crop": self.crop
        })

        url = f"{self.base_url_mw}/Records/Create"

        return self._request("POST", url, data=payload)
