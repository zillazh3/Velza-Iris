import time
from pythonosc import udp_client

TARGET_IP = "127.0.0.1"
TARGET_PORT = 9000
N_POINTS = 8

client = udp_client.SimpleUDPClient(TARGET_IP, TARGET_PORT)
P = "/avatar/parameters/"

print(f"[*] Conectando con VRChat en {TARGET_IP}:{TARGET_PORT}...")

# 1. Encender el rig: vive en el origen del mundo, no hay nada que calibrar.
print("[Flujo A] iris_enable = True")
client.send_message(P + "iris_enable", True)
time.sleep(0.05)

# 2. Path: 8 puntos de control. Coordenadas normalizadas (1.0 = 50 m) relativas al centro de la región.
points = [(-0.04 + 0.0114 * i, 0.02 * (i % 3), 0.02 + 0.0114 * i) for i in range(N_POINTS)]

# 2a. Dueño: parámetros locales con float32 exacto (instantáneo, 0 bits de red).
print("[Flujo B] Dueño: puntos de control por iris_pc_i_x/y/z")
for i, p3 in enumerate(points):
    for ax, v in zip("xyz", p3):
        client.send_message(P + f"iris_pc_{i}_{ax}", float(v))

# 2b. Remotos: bus time-sliced. Por slot: datos -> esperar -> selector -> esperar. Slot 2+i = posición coarse del punto i.
# (Sin fine/rotación: solo para ver el path en otro cliente. La app manda región, coarse, rotación y fine.)
print("[Flujo B] Remotos: slots de posición coarse por iris_net_slot")
for i, (nx, ny, nz) in enumerate(points):
    client.send_message(P + "iris_net_x", float(nx))
    client.send_message(P + "iris_net_y", float(ny))
    client.send_message(P + "iris_net_z", float(nz))
    time.sleep(0.25)
    client.send_message(P + "iris_net_slot", 2 + i)
    time.sleep(0.25)
client.send_message(P + "iris_net_slot", 0)
client.send_message(P + "iris_net_slot", 2 + 3 * N_POINTS + 3)  # MODE_PATH
time.sleep(0.25)
client.send_message(P + "iris_net_slot", 0)

# 3. Flujo C: pose en vivo del dueño (local) y progreso del path para los remotos.
print("[Flujo C] Pose en vivo del dueño y Path_T a mitad del recorrido...")
client.send_message(P + "iris_cam_x", 0.0)
client.send_message(P + "iris_cam_y", 0.02)
client.send_message(P + "iris_cam_z", 0.04)
client.send_message(P + "iris_cam_rx", 0.0)
client.send_message(P + "iris_cam_ry", 0.0)
client.send_message(P + "iris_cam_rz", 0.0)
client.send_message(P + "iris_path_t", 0.0)  # -1..1  ->  T = 0..1

print("[*] Prueba directa de OSC completada. VRChat debería mostrar el visualizador.")
