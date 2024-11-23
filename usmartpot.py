import ujson
import urequests

class SmartPotAPI:
    def __init__(self, base_url="api-smartpot.onrender.com"):
        self.base_url = base_url
        self.token = None
        self.sleep_btw_updates = 3  # Tiempo en segundos entre cada actualización
    
    def login(self, email, password):
        """Realiza login y obtiene el token de autenticación"""
        
        data = {'email': email, 'password': password}
        
        headers = {
            'User-Agent': 'SmartPotClient/1.0.0 (https://wokwi.com/)',
            'Content-Type': 'application/json',
            'Accept': '*/*',
        }
        endpoint = f'https://{self.base_url}/auth/login'
        
        try:
            # Realiza la solicitud POST para el login
            response = urequests.post(endpoint, json=data, headers=headers)

            # Verifica el código de estado de la respuesta
            if response.status_code == 200:
                response_data = response.text.strip()
                if response_data:
                    self.token = response_data
                    print(f"Login exitoso, token: {self.token}")
                    return self.token
                else:
                    print("Error de login, no se obtuvo el token.")
                    return None
            else:
                print(f"Error en la solicitud de login. Status code: {response.status_code}")
                try:
                    print(f"Content: {response.json()}")
                except ValueError as e:
                    print(f"Content: {response.text}") 
                    print(f"Headers: {response.headers}") 
                return None
        except Exception as e:
            print(f"Error durante la solicitud: {e}")
            return None

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
            'Accept-Encoding': 'gzip, deflate, br',
            'Connection': 'keep-alive',
            'Host': 'api-smartpot.onrender.com',
            'Authorization': f'Bearer {self.token}'  # Autenticación Bearer
        }
        
        # Realiza la solicitud GET para obtener los usuarios
        try:
            response = urequests.get(f'https://{self.base_url}/Users/All', headers=headers)
            
            if response.status_code == 200:
                response_data = response.json()
                print(response_data)
                if isinstance(response_data, list):
                    print(f"Usuarios encontrados: {len(response_data)}")
                    for user in response_data:
                        print(user)  # Aquí puedes formatear los datos según lo que quieras mostrar
                else:
                    print("No se pudo obtener la lista de usuarios.")
            else:
                print(f"Error en la solicitud. Status code: {response.status_code}")
        except Exception as e:
            print(f"Error durante la solicitud GET: {e}")
