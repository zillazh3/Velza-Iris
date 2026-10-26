# 🔒 Política de seguridad / Security policy

## Versiones con soporte / Supported versions

Solo la última versión publicada en [Releases](https://github.com/zillazh3/Velza-Iris/releases) recibe correcciones.
Only the latest version published in [Releases](https://github.com/zillazh3/Velza-Iris/releases) receives fixes.

## Reportar una vulnerabilidad / Reporting a vulnerability

**No abras un issue público.** Usa el [reporte privado de GitHub](https://github.com/zillazh3/Velza-Iris/security/advisories/new).
**Don't open a public issue.** Use [GitHub's private vulnerability reporting](https://github.com/zillazh3/Velza-Iris/security/advisories/new).

Incluye / Please include: versión de Velza Iris, versión de Windows, pasos para reproducirlo e impacto esperado / Velza Iris version, Windows version, reproduction steps and expected impact.

## Alcance / Scope

Velza Iris no usa telemetría ni internet; solo se comunica con VRChat por OSC/OSCQuery en la red local (UDP 9000/9001). Son de interés, por ejemplo, el manejo de archivos JSON importados, la escritura en `VelzaIris-data/` y los mensajes OSC recibidos.
Velza Iris uses no telemetry or internet; it only talks to VRChat over OSC/OSCQuery on the local network (UDP 9000/9001). Of interest: imported JSON handling, writes to `VelzaIris-data/`, and received OSC messages.
