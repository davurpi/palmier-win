# Prompt para probar Palmier Win en el PC Windows

Pega esto en Claude (Code o Desktop con acceso a terminal) en el PC Windows:

---

Quiero que pruebes Palmier Win, un editor de vídeo open source que he publicado en
https://github.com/davurpi/palmier-win (autorizo las descargas e instalaciones de estos pasos).

1. **FFmpeg**: comprueba `ffmpeg -version` y `ffprobe -version`. Si no están:
   `winget install ffmpeg` (o Gyan.FFmpeg), y reabre la terminal para refrescar el PATH.
2. **Descarga el instalador** de la última release:
   `https://github.com/davurpi/palmier-win/releases/latest` → asset `Palmier.Win_*_x64-setup.exe`.
   Puedes bajarlo con `curl -LO <url del asset>`.
3. **Instálalo y ábrelo** (SmartScreen avisará porque va sin firmar: "Más información → Ejecutar de todas formas").
4. **Test funcional por MCP** (la app arranca un servidor MCP en `http://127.0.0.1:19790/mcp`):
   - Genera 2 clips de prueba con ffmpeg (testsrc2 y smptebars, 4-6 s, con audio sine).
   - Vía HTTP JSON-RPC (o registrándolo con `claude mcp add --transport http palmier-win http://127.0.0.1:19790/mcp`):
     `import_media` con esos clips, `add_clip` de ambos, `apply_layout` "pip-bottom-right",
     `add_text` con un rótulo, `set_clip_properties` (speed 2, saturation 0 en el PIP),
     `remove_silence` sobre un audio con huecos, `undo`, y `export_project` a un MP4.
   - Verifica el MP4 con ffprobe (duración/resolución) y extrayendo un frame con ffmpeg.
5. **Test de UI**: mueve un clip en el timeline (debe tener snapping), parte con S, borra con Supr,
   Ctrl+Z para deshacer, prueba el panel Inspector (escala, opacidad, fades) y el diálogo Export.
6. **Repórtame**: versión de Windows, si todo funcionó, cualquier error (con su texto exacto),
   y un pantallazo de la app con el timeline montado.

---

Con ese informe, correcciones que salgan se arreglan desde el Mac y se publica una release nueva.
