import ujson
import urequests


class Ubot:
    def __init__(self, email, password):
        self.base_url_mw = "smartpot-middleware.onrender.com"
        self.token = None
        self.login(email, password)

    def login(self, email, password):
        if not isinstance(email, str) or not isinstance(password, str):
            raise TypeError("El email y la contraseña deben ser cadenas de texto.")

        if "@" not in email or "." not in email.split('@')[-1]:
            raise ValueError("El email proporcionado no es válido.")

        if not email or not password:
            raise ValueError("El email y la contraseña no pueden estar vacíos.")

        data = f'<credentials><email>{email}</email><password>{password}</password></credentials>'

        headers = {
            'User-Agent': 'SmartPotClient/1.0.0 (https://wokwi.com/)',
            'Content-Type': 'application/xml',
            'Accept': 'application/json',
        }

        try:
            response = urequests.post(f"https://{self.base_url_mw}/login", data=data, headers=headers)
            response_data = response.json()

            if "token" not in response_data:
                raise KeyError(response_data["message"])

            self.token = response_data["token"]
        except Exception as e:
            raise RuntimeError(f"{e}")

    def create_record(self, crop_id, measures):
        """Crea un registro con las medidas proporcionadas para un cultivo específico en formato XML (sin xml.etree.ElementTree)"""
        if not self.token:
            print("No se puede hacer la solicitud sin un token válido.")
            return

        if not crop_id or not isinstance(measures, dict):
            raise ValueError("El ID del cultivo y las medidas deben ser válidos.")

        xml_str = "<root>"
        xml_str += "<record>"

        for measure, value in measures.items():
            xml_str += f"<{measure}>{value}</{measure}>"

        xml_str += "</record>"

        xml_str += f"<crop>{crop_id}</crop>"
        xml_str += f"<token>{self.token}</token>"

        xml_str += "</root>"

        headers = {
            'User-Agent': 'SmartPotClient/1.0.0 (https://wokwi.com/)',
            'Content-Type': 'application/xml',
            'Accept': 'application/json',
            'Cache-Control': 'no-cache',
        }

        try:
            # Hacer la solicitud POST al nuevo endpoint
            response = urequests.post(f'https://{self.base_url_mw}/create_record', data=xml_str, headers=headers)

            if response.status_code == 201:
                response_data = response.json()
                print(f"Registro creado exitosamente")
            else:
                print(f"Error en la solicitud. Status code: {response.status_code}")
                print(response.headers)
                print(response.text)
        except Exception as e:
            print(f"Error durante la solicitud POST: {e}")
