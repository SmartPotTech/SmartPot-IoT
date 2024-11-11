import ujson as json
import urequests
import time
import gc

class SmartPotAPI:
    def __init__(self, base_url="api-smartpot.onrender.com"):
        self.base_url = base_url
        self.token = None
        self.sleep_btw_updates = 3  # Tiempo en segundos entre cada actualización
    
    def login(self, email, password):
        """Realiza login y obtiene el token de autenticación"""
        
        # Payload con las credenciales
        payload = json.dumps({
            "email": email,
            "password": password
        })
        
        headers = {
            'User-Agent': 'SmartPotClient/1.0.0',
            'Content-Type': 'application/json'
        }
        
        # Realiza la solicitud POST para el login
        response = urequests.post(f'https://{self.base_url}/auth/login', data=payload, headers=headers)
        
        if response.status_code == 200:
            response_data = response.json()
            if 'token' in response_data:
                self.token = response_data['token']
                print(f"Login exitoso, token: {self.token}")
                return self.token
            else:
                print("Error de login, no se obtuvo el token.")
                return None
        else:
            print(f"Error en la solicitud de login. Status code: {response.status_code}")
            return None
    
    def get_all_users(self):
        """Obtiene todos los usuarios utilizando el token Bearer"""
        if not self.token:
            print("No se puede hacer la solicitud sin un token válido.")
            return
        
        headers = {
            'User-Agent': 'SmartPotClient/1.0.0',
            'Authorization': f'Bearer {self.token}'  # Autenticación Bearer
        }
        
        # Realiza la solicitud GET para obtener los usuarios
        response = urequests.get(f'https://{self.base_url}/Users/All', headers=headers)
        
        if response.status_code == 200:
            response_data = response.json()
            if isinstance(response_data, list):
                print(f"Usuarios encontrados: {len(response_data)}")
                for user in response_data:
                    print(user)  # Aquí puedes formatear los datos según lo que quieras mostrar
            else:
                print("No se pudo obtener la lista de usuarios.")
        else:
            print(f"Error en la solicitud. Status code: {response.status_code}")