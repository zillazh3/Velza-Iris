import time
import socket
from pythonosc import udp_client

TARGET_IP = "127.0.0.1"
TARGET_PORT = 9000

client = udp_client.SimpleUDPClient(TARGET_IP, TARGET_PORT)

print(f"[*] Conectando con VRChat en {TARGET_IP}:{TARGET_PORT}...")

# 1. Flujo A: Activar el Ancla
print("[Flujo A] Enviando /avatar/parameters/iris_enable = False e iris_wp_index = 0")
client.send_message("/avatar/parameters/iris_enable", False)
client.send_message("/avatar/parameters/iris_wp_index", 0)
time.sleep(0.05)

print("[Flujo A] Clavando Ancla en el Mundo: iris_enable = True")
client.send_message("/avatar/parameters/iris_enable", True)
time.sleep(0.05)

# 2. Flujo B: Multiplexación de los 5 huesos
# Coordenadas relativas de prueba frente al avatar (normalizadas en [-1.0, 1.0])
# Suponiendo un arco que se extiende de x=-0.1 a x=+0.1, y=0.0 a 0.2, z=0.02 a 0.10 (1 a 5 metros)
waypoints = [
    (-0.04, 0.00, 0.02),  # W1: 1m frente al ancla
    (-0.02, 0.02, 0.04),  # W2: 2m
    ( 0.00, 0.03, 0.06),  # W3: 3m
    ( 0.02, 0.02, 0.08),  # W4: 4m
    ( 0.04, 0.00, 0.10),  # W5: 5m
]

print("[Flujo B] Iniciando multiplexación temporal de los 5 huesos del Path...")
client.send_message("/avatar/parameters/iris_wp_index", 0)
time.sleep(0.025)

for i, (nx, ny, nz) in enumerate(waypoints, start=1):
    print(f"  -> Hueso Waypoint_0{i}: cursor a ({nx:.3f}, {ny:.3f}, {nz:.3f})")
    client.send_message("/avatar/parameters/iris_wp_x", float(nx))
    client.send_message("/avatar/parameters/iris_wp_y", float(ny))
    client.send_message("/avatar/parameters/iris_wp_z", float(nz))
    time.sleep(0.030)

    print(f"  -> Asignando iris_wp_index = {i}")
    client.send_message("/avatar/parameters/iris_wp_index", int(i))
    time.sleep(0.030)

    print(f"  -> Congelando iris_wp_index = 0")
    client.send_message("/avatar/parameters/iris_wp_index", int(0))
    time.sleep(0.020)

print("[Flujo B] ¡Multiplexación completada exitosamente!")

# 3. Flujo C: Posicionar visualizador de cámara
print("[Flujo C] Posicionando visualizador de cámara...")
client.send_message("/avatar/parameters/iris_cam_x", 0.0)
client.send_message("/avatar/parameters/iris_cam_y", 0.02)
client.send_message("/avatar/parameters/iris_cam_z", 0.04)
client.send_message("/avatar/parameters/iris_cam_rx", 0.0)
client.send_message("/avatar/parameters/iris_cam_ry", 0.0)
client.send_message("/avatar/parameters/iris_cam_rz", 0.0)

print("[*] Prueba directa de OSC completada. VRChat debería mostrar el visualizador.")
