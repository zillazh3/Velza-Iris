import math
import socket
import time
from pythonosc import osc_message, udp_client

LISTEN_PORT = 9001
SEND_PORT = 9000

print("[1] Capturando pose actual de la cámara de VRChat en puerto 9001...")
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(('127.0.0.1', LISTEN_PORT))
sock.settimeout(1.5)

captured_pose = None
start = time.time()
while time.time() - start < 3.0:
    try:
        data, _ = sock.recvfrom(4096)
        msg = osc_message.OscMessage(data)
        if msg.address == "/usercamera/Pose" and len(msg.params) >= 6:
            captured_pose = msg.params[:6]
            break
    except socket.timeout:
        pass
sock.close()

if not captured_pose:
    print("[-] No se recibió /usercamera/Pose en 3 segundos. Usando pose de referencia previa.")
    captured_pose = [1.80, 1.81, 7.14, 7.11, 214.04, 0.0]

ax, ay, az, pitch, yaw, roll = captured_pose
print(f"[+] Pose de Ancla capturada: pos=({ax:.3f}, {ay:.3f}, {az:.3f}), rot=(pitch={pitch:.1f}°, yaw={yaw:.1f}°, roll={roll:.1f}°)")

client = udp_client.SimpleUDPClient('127.0.0.1', SEND_PORT)

# --- FLUJO A: Calibrar Ancla ---
print("\n[FLUJO A] Reseteando y activando Ancla...")
client.send_message("/avatar/parameters/iris_enable", False)
client.send_message("/avatar/parameters/iris_wp_index", 0)
time.sleep(0.050)

client.send_message("/avatar/parameters/iris_enable", True)
time.sleep(0.050)
print("  -> iris_enable = True enviado (Ancla clavada en el mundo)")

# --- FLUJO B: Generar 5 Waypoints en línea recta curva frente al usuario ---
# Vector forward a partir del yaw de Unity
yaw_rad = math.radians(yaw)
# En Unity forward con yaw: x = sin(yaw), z = cos(yaw) o -sin, -cos según convención de cámara
# Probemos vector hacia el frente de donde mira la cámara
fx = math.sin(yaw_rad)
fz = math.cos(yaw_rad)

MAX_RANGE = 50.0

print("\n[FLUJO B] Muestreando 5 puntos frente al usuario...")
waypoints_world = []
for i in range(1, 6):
    dist = 0.5 * i  # 0.5m, 1.0m, 1.5m, 2.0m, 2.5m
    # Pequeña elevación y arco
    wx = ax + fx * dist + 0.1 * math.sin(i * 0.8)
    wy = ay + 0.05 * i
    wz = az + fz * dist
    waypoints_world.append((wx, wy, wz))
    print(f"  Punto {i}: mundo=({wx:.3f}, {wy:.3f}, {wz:.3f})")

print("\n[FLUJO B] Ejecutando protocolo estricto de Multiplexación Temporal...")
client.send_message("/avatar/parameters/iris_wp_index", 0)
time.sleep(0.025)

for i, (wx, wy, wz) in enumerate(waypoints_world, start=1):
    # Normalización matemática estricta respecto al Ancla:
    nx = max(-1.0, min(1.0, (wx - ax) / MAX_RANGE))
    ny = max(-1.0, min(1.0, (wy - ay) / MAX_RANGE))
    nz = max(-1.0, min(1.0, (wz - az) / MAX_RANGE))

    print(f"  [Paso {i}/5] Waypoint_0{i}:")
    print(f"    a. Enviar cursor normalizado: wp_x={nx:.4f}, wp_y={ny:.4f}, wp_z={nz:.4f}")
    client.send_message("/avatar/parameters/iris_wp_x", float(nx))
    client.send_message("/avatar/parameters/iris_wp_y", float(ny))
    client.send_message("/avatar/parameters/iris_wp_z", float(nz))
    
    print("    b. Esperar 30 ms (BlendTree posiciona WP_Target)...")
    time.sleep(0.030)
    
    print(f"    c. Enviar iris_wp_index = {i} (Constraint salta a WP_Target)...")
    client.send_message("/avatar/parameters/iris_wp_index", int(i))
    
    print("    d. Esperar 30 ms...")
    time.sleep(0.030)
    
    print("    e. Enviar iris_wp_index = 0 (Congelar hueso con Weight=0)...")
    client.send_message("/avatar/parameters/iris_wp_index", int(0))
    
    print("    f. Esperar 20 ms...")
    time.sleep(0.020)

print("[+] ¡Multiplexación de los 5 huesos completada exitosamente!")

# --- FLUJO C: Sincronizar posición de la cámara visualizadora ---
print("\n[FLUJO C] Sincronizando visualizador de cámara...")
# Colocar la cámara del visualizador a 1.2m al frente de la persona
cam_wx = ax + fx * 1.2
cam_wy = ay + 0.1
cam_wz = az + fz * 1.2

cam_nx = max(-1.0, min(1.0, (cam_wx - ax) / MAX_RANGE))
cam_ny = max(-1.0, min(1.0, (cam_wy - ay) / MAX_RANGE))
cam_nz = max(-1.0, min(1.0, (cam_wz - az) / MAX_RANGE))

# Envolver rotación [-180, 180] / 180.0
def wrap_norm_angle(deg):
    w = ((deg + 180.0) % 360.0 + 360.0) % 360.0 - 180.0
    return max(-1.0, min(1.0, w / 180.0))

cam_nrx = wrap_norm_angle(pitch)
cam_nry = wrap_norm_angle(yaw)
cam_nrz = wrap_norm_angle(roll)

print(f"  Enviando cam_pos=({cam_nx:.4f}, {cam_ny:.4f}, {cam_nz:.4f}), cam_rot=({cam_nrx:.4f}, {cam_nry:.4f}, {cam_nrz:.4f})")
client.send_message("/avatar/parameters/iris_cam_x", float(cam_nx))
client.send_message("/avatar/parameters/iris_cam_y", float(cam_ny))
client.send_message("/avatar/parameters/iris_cam_z", float(cam_nz))
client.send_message("/avatar/parameters/iris_cam_rx", float(cam_nrx))
client.send_message("/avatar/parameters/iris_cam_ry", float(cam_nry))
client.send_message("/avatar/parameters/iris_cam_rz", float(cam_nrz))

print("\n[*] Testeo directo de OSC finalizado. Verifica el visor en VRChat.")
