import ujson
import urequests


class Ubot:
    def __init__(self, email, password):
        if not isinstance(email, str) or not isinstance(password, str):
            raise TypeError("El email y la contraseña deben ser cadenas de texto.")

        if "@" not in email or "." not in email.split('@')[-1]:
            raise ValueError("El email proporcionado no es válido.")

        if not email or not password:
            raise ValueError("El email y la contraseña no pueden estar vacíos.")

        self.base_url = "api-smartpot.onrender.com"
        self.token = None

        data = f'<credentials><email>{email}</email><password>{password}</password></credentials>'

        headers = {
            'User-Agent': 'SmartPotClient/1.0.0 (https://wokwi.com/)',
            'Content-Type': 'application/xml',
            'Accept': 'application/json',
        }

        try:
            response = urequests.post(f"https://smartpot-middleware.onrender.com/login", data=data, headers=headers)
            response_data = response.json()

            if "token" not in response_data:
                raise KeyError(response_data["message"])

            self.token = response_data["token"]
        except Exception as e:
            raise RuntimeError(f"{e}")

    def get_all_users(self):
        """Obtiene todos los usuarios utilizando el token Bearer"""
        if not self.token:
            print("No se puede hacer la solicitud sin un token válido.")
            return

        headers = {
            'User-Agent': 'SmartPotClient/1.0.0 (https://wokwi.com/)',
            'Content-Type': 'application/json',
            'Cache-Control': 'no-cache',
            'Accept': '*/*',
            'Authorization': f'Bearer {self.token}'
        }

        try:
            response = urequests.get(f'https://{self.base_url}/Users/All', headers=headers)

            if response.status_code == 200:
                response_data = response.json()
                print(response_data)
                if isinstance(response_data, list):
                    print(f"Usuarios encontrados: {len(response_data)}")
                    for user in response_data:
                        print(user)
                else:
                    print("No se pudo obtener la lista de usuarios.")
            else:
                print(f"Error en la solicitud. Status code: {response.status_code}")
        except Exception as e:
            print(f"Error durante la solicitud GET: {e}")
