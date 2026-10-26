import math
import time
from pythonosc import udp_client

client = udp_client.SimpleUDPClient('127.0.0.1', 9000)

print("[*] Iniciando animación de prueba del visualizador de cámara a 60 Hz por 5 segundos...")
print("    Observa en VRChat si la cámara se mueve en una órbita suave.")

# Ancla de referencia
MAX_RANGE = 50.0

start = time.time()
frame_interval = 1.0 / 60.0

while time.time() - start < 5.0:
    t = time.time() - start
    
    # Órbita de 1.5 metros alrededor del ancla
    angle = t * 1.5  # rad
    orbit_r = 1.5
    
    rel_x = orbit_r * math.cos(angle)
    rel_y = 0.2 * math.sin(t * 2.0)
    rel_z = orbit_r * math.sin(angle)
    
    # Normalizado [-1, 1] respecto a 50m
    nx = rel_x / MAX_RANGE
    ny = rel_y / MAX_RANGE
    nz = rel_z / MAX_RANGE
    
    # Rotación mirando hacia el centro: yaw gira con el ángulo
    yaw_deg = math.degrees(math.atan2(-rel_x, -rel_z))
    pitch_deg = -math.degrees(math.atan2(rel_y, orbit_r))
    
    # Normalizado [-1, 1] respecto a 180°
    def wrap_norm(deg):
        w = ((deg + 180.0) % 360.0 + 360.0) % 360.0 - 180.0
        return max(-1.0, min(1.0, w / 180.0))
    
    nrx = wrap_norm(pitch_deg)
    nry = wrap_norm(yaw_deg)
    nrz = 0.0
    
    client.send_message("/avatar/parameters/iris_cam_x", float(nx))
    client.send_message("/avatar/parameters/iris_cam_y", float(ny))
    client.send_message("/avatar/parameters/iris_cam_z", float(nz))
    client.send_message("/avatar/parameters/iris_cam_rx", float(nrx))
    client.send_message("/avatar/parameters/iris_cam_ry", float(nry))
    client.send_message("/avatar/parameters/iris_cam_rz", float(nrz))
    
    time.sleep(frame_interval)

print("[+] Animación completada. 300 frames emitidos exitosamente.")
