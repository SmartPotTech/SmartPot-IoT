import socket
import ujson

class SmartPotAPI:
    def __init__(self, base_url="api-smartpot.onrender.com"):
        self.base_url = base_url
        self.token = None
        self.sleep_btw_updates = 3  # Tiempo en segundos entre cada actualización
        self.port = 80

    def _send_request(self, method, endpoint, headers, data=None):
        """Método genérico para enviar solicitudes HTTP con el socket"""
        # Crear el socket y conectar con el servidor
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((self.base_url, self.port))
        
        # Construir la solicitud HTTP
        request = f"{method} {endpoint} HTTP/1.1\r\n"
        request += f"Host: {self.base_url}\r\n"
        
        # Añadir los encabezados a la solicitud
        for header, value in headers.items():
            request += f"{header}: {value}\r\n"
        
        # Si hay datos, añadirlos al cuerpo de la solicitud
        if data:
            request += f"Content-Length: {len(data)}\r\n"
            request += "\r\n"
            request += data
        
        # Enviar la solicitud
        sock.send(request.encode())

        # Leer la respuesta
        response = sock.recv(4096)
        sock.close()
        
        # Decodificar la respuesta y retornar el contenido
        return response.decode()

    def login(self, email, password):
        """Realiza login y obtiene el token de autenticación"""
        
        data = {
            'email': email,
            'password': password
        }
        
        headers = {
            'User-Agent': 'SmartPotClient/1.0.0 (https://wokwi.com/)',
            'Content-Type': 'application/json',
            'Accept': 'application/json',
        }

        # Convertir los datos en formato JSON
        json_data = ujson.dumps(data)

        # Endpoint para el login
        endpoint = '/auth/login'
        
        try:
            # Enviar la solicitud POST para login
            response = self._send_request("POST", endpoint, headers, json_data)
            
            # Verificar el código de estado de la respuesta
            if "200 OK" in response:
                # Extraer el token de la respuesta (asumimos que la respuesta es solo el token)
                token_start = response.find("\r\n\r\n") + 4
                self.token = response[token_start:].strip()
                print(f"Login exitoso, token: {self.token}")
                return self.token
            else:
                print(f"Error en la solicitud de login. Respuesta: {response}")
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
            'Authorization': f'Bearer {self.token}',
            'Accept': 'application/json',
        }
        
        # Endpoint para obtener todos los usuarios
        endpoint = '/Users/All'
        
        try:
            # Enviar la solicitud GET para obtener los usuarios
            response = self._send_request("GET", endpoint, headers)
            
            # Verificar que la respuesta contiene datos JSON
            if "200 OK" in response:
                try:
                    response_data = ujson.loads(response.split("\r\n\r\n")[1])
                    print(response_data)
                    if isinstance(response_data, list):
                        print(f"Usuarios encontrados: {len(response_data)}")
                        for user in response_data:
                            print(user)  # Aquí puedes formatear los datos según lo que quieras mostrar
                    else:
                        print("No se pudo obtener la lista de usuarios.")
                except ValueError:
                    print("Error al parsear la respuesta como JSON.")
            else:
                print(f"Error en la solicitud. Respuesta: {response}")
        except Exception as e:
            print(f"Error durante la solicitud GET: {e}")
