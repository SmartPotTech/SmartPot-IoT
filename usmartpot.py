import ujson
import requests
import ussl
import socket
import time
import utilitys


class Ubot:
    def __init__(self, email, password):
        self.base_url_mw = "https://api.smartpot.app"
        self.token = None
        self.login(email, password)

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

            response = requests.request(method, url, data=data.encode("utf-8"), headers=headers)

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

    def login(self, email, password):
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

    def create_record(self, crop_id, measures):
        if not self.token:
            print("No hay token, no se puede crear record.")
            return None

        if not crop_id or not isinstance(measures, dict):
            raise ValueError("El ID del cultivo y las medidas deben ser válidos.")

        payload = ujson.dumps({
            "measures": measures,
            "crop": crop_id
        })

        url = f"{self.base_url_mw}/Records/Create"

        return self._request("POST", url, data=payload)
