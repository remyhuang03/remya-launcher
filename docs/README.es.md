<p align="center">
  <img src="../assets/icons/launcher.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">Un lanzador y gestor de portapapeles con Material Design 3 Expressive para Hyprland</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.47.8-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.47.8">
  <img src="https://img.shields.io/badge/Rust-2024-000000?style=flat-square&logo=rust&logoColor=white" alt="Rust 2024">
  <img src="https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-4C1?style=flat-square" alt="License: PolyForm Noncommercial 1.0.0">
</p>

<p align="center">
  <a href="../README.md">English</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.zh-TW.md">繁體中文</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.de.md">Deutsch</a> ·
  <strong>Español</strong> ·
  <a href="README.ja.md">日本語</a>
</p>

Remya Launcher (`top.remya.launcher`) es un lanzador de aplicaciones, selector rápido y gestor de historial del portapapeles en Wayland pensado para usarse con el teclado en Hyprland, desarrollado con Flutter, Rust y Material Design 3 Expressive.

- **Flujo centrado en el teclado**: diseñado para que tus manos permanezcan en la zona principal de teclas, con ámbitos por prefijo natural, selección rápida `Alt+1–9` e indicaciones contextuales en el pie.
- **Historial del portapapeles en Wayland integrado**: un servicio en Rust en segundo plano guarda texto, código, enlaces, colores, archivos y miniaturas de imágenes con enmascarado de datos sensibles e integración con Remya Settings.
- **Material Design 3 Expressive**: transformación suave de formas, tipografía Inter y JetBrains Mono, y una capa `gtk-layer-shell` anclada al 22 % de la altura de la pantalla.

> [!CAUTION]
> **Early Development & Preview Only — Not Ready for Daily Use (Desarrollo temprano y solo vista previa — No apto para uso diario)**
>
> Remya Launcher se encuentra en desarrollo activo temprano. Actualmente solo están implementados el **historial del portapapeles en vivo** (el demonio en Rust y la vista Clipboard en vivo) y las **vistas previas interactivas de diseño** (`--preview`). La mayoría de los demás ámbitos del lanzador, motores de búsqueda e integraciones de ventanas y acciones de Hyprland descritos a continuación corresponden a la [especificación de diseño en Figma de Remya Launcher](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) y aún no están implementados.

## 🚀 Instalación

### AUR (canal `dev`)

En Arch Linux, instala el paquete de desarrollo cuando esté disponible en AUR:

```bash
paru -S remya-launcher-git || yay -S remya-launcher-git
```

### Archivo de lanzamiento

Los archivos precompilados aún no están publicados. Cuando estén disponibles en [GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases), verifica la suma de comprobación, extrae el archivo y ejecuta el instalador local:

```bash
sha256sum -c SHA256SUMS --ignore-missing
tar --zstd -xf remya_launcher-0.1.0+1-linux-x86_64.tar.zst
cd remya_launcher-0.1.0+1-linux-x86_64
python tool/install.py
```

Para instalaciones combinadas del escritorio Remya, instala **Remya Settings** antes que **Remya Bar**. Las preferencias de retención y captura del portapapeles se gestionan en **Remya Settings → System → Clipboard**.

## ✨ Características

### Disponible actualmente

- **Historial del portapapeles en vivo (`remya_launcher --clipboard` / `Ctrl+6` / `;`)**:
  - Observador en segundo plano en Rust (`remya-launcher-clipboard`) ejecutándose como unidad de usuario de systemd `top.remya.launcher.clipboard.service` junto con la sesión gráfica.
  - Conserva por defecto hasta 500 elementos no fijados durante 30 días en `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3` (por defecto `~/.local/state/remya/launcher/clipboard.sqlite3`); los elementos fijados quedan exentos de la limpieza automática.
  - Captura selecciones de hasta 64 MiB e imágenes decodificadas de hasta 24 megapíxeles (PNG, JPEG, WebP, GIF, BMP); las imágenes de más de 20 MB dentro de esos límites conservan una miniatura.
  - Enmascara automáticamente en memoria durante hasta 45 segundos los elementos marcados como sensibles por la aplicación de origen sin escribirlos en disco.
  - Expone una interfaz IPC JSON sobre socket Unix para el mismo usuario en `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` para Remya Settings (`preferences`, `setPreference` para `enabled`/`maxItems`/`retentionDays` y `clear`).
  - *Limitación conocida de Wayland*: Wayland data-control no identifica qué aplicación proporcionó el contenido del portapapeles, por lo que las etiquetas de aplicación de origen y las listas de exclusión por aplicación del diseño no pueden implementarse de forma fiable.
- **Vistas previas interactivas de diseño (`--preview`)**:
  - Inspecciona todos los estados de interfaz planeados desde un paquete compilado usando `--preview=<name>`:
    - Estados del lanzador: `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, `clipboard`
    - Variantes de portapapeles: `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, `clipboard-hidden`

### Ámbitos y capacidades planificadas (especificación de diseño)

Las siguientes capacidades están planificadas según la [especificación de diseño de Remya Launcher](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher):

- **Ámbitos con prefijos naturales**:
  - **All (`Ctrl+1`)**: resultados mixtos entre aplicaciones, archivos, historial web y comandos (hasta 3 por grupo, ordenados dinámicamente por mejor puntuación), tarjeta automática de calculadora para expresiones numéricas o entre paréntesis y detección directa de URL.
  - **Apps (`Ctrl+2`)**: búsqueda difusa de entradas `.desktop` XDG por nombre, `GenericName`, `Keywords` y ejecutable con ponderación frecency y acciones secundarias de escritorio (p. ej., nueva ventana privada).
  - **Files (`Ctrl+3`, prefijo `/` o `~`)**: autocompletado nativo de rutas (`Tab` segmento a segmento), búsqueda difusa de nombres de archivo (`fd` + recientes) y panel derecho de vista previa (contenido de carpetas, fragmentos de texto, miniaturas) ampliando el panel a 880 px.
  - **Web (`Ctrl+4`, prefijo `?`)**: historial local de navegador en modo solo lectura (Firefox `places.sqlite` / Chromium `History`) ordenado por frecency, con búsqueda web de respaldo (`Ctrl+Enter`).
  - **Calc (`Ctrl+5`, prefijo `=`)**: cálculos con `libqalculate`, conversión de unidades y divisas, bases numéricas, porcentajes y aritmética de fechas (`Tab` para rotar unidades, `↑` para recuperar expresiones anteriores).
  - **Clipboard (`Ctrl+6`, prefijo `;`)**: vista dividida de 960 px con filtros por tipo (`All · Text · Images · Links · Files · Colors` mediante `Ctrl+F`), agrupación `Pinned → Today → Yesterday → Earlier`, fusión de duplicados con contador de copias, vistas previas por tipo (código con resaltado y números de línea, muestras HEX/RGB/HSL/OKLCH, fondo ajedrezado en imágenes) y pegado directo en la ventana enfocada anterior (`Enter` o `Ctrl+Enter` para acciones secundarias por tipo).
  - **Windows (`Ctrl+7`, prefijo `#`)**: coincidencia mediante `hyprctl clients` por título y clase con insignia de espacio de trabajo; `Enter` enfoca y cambia de espacio de trabajo, `Ctrl+Enter` mueve la ventana al espacio actual.
  - **Emoji (prefijo `:`)** y **Run (prefijo `$` / `>`)**: búsqueda de emojis y símbolos por nombre o alias, y ejecución directa de comandos en shell o terminal.
- **Navegación centrada en el teclado**:
  - `↑`/`↓` o `Ctrl+N`/`Ctrl+P` para moverse entre grupos; `Ctrl+↑`/`Ctrl+↓` para saltar al inicio del grupo anterior o siguiente.
  - Mantén `Alt` para mostrar los números de fila y pulsa `Alt+1–9` para ejecutar directamente la acción principal (`Alt+1–6` para elementos fijados en el estado vacío), o `Ctrl+Alt+1–9` para la acción secundaria.
  - `→` o `Ctrl+K` abre el menú filtrable de acciones; `Ctrl+C` copia el valor seleccionado; `Ctrl+D` fija o desfija; `Shift+Del` olvida de Recent o elimina del historial.
  - `Ctrl+W` / `Ctrl+Backspace` borra una palabra o segmento de ruta; `Ctrl+U` limpia la consulta; `Esc` retrocede paso a paso (menú → consulta → ámbito → cerrar); `?` con entrada vacía muestra la guía rápida de atajos.
- **Superposición en Hyprland y sistema visual**:
  - Capa `gtk-layer-shell` con interactividad exclusiva de teclado, 720 px de ancho por defecto, transiciones de altura solo hacia abajo (250 ms) y cierre al hacer clic fuera o perder el foco.
  - Transformación de formas Material Design 3 Expressive (`20 px → 12 px` en filas seleccionadas y mosaicos fijados, `16 px → 10 px` en chips de ámbito) y tokens de tema sincronizados con el color base del escritorio Remya.

## ⌨️ Desarrollo

### Stack tecnológico y arquitectura

- **Frontend**: Flutter y Dart (fijado en `.fvmrc` a `3.47.8`) con `material_ui` (Material Design 3 Expressive) y `flutter_svg`.
  - `lib/app.dart` y `lib/main.dart`: composición de la aplicación, manejo de opciones CLI (`--clipboard`, `--preview=...`) y registro de licencias.
  - `lib/features/launcher/`: panel del lanzador, campo de búsqueda, chips de ámbito, filas de resultados y fijados, indicaciones de teclas, tarjeta de calculadora, vista previa de archivos y menú de acciones.
  - `lib/features/clipboard/`: página del portapapeles en vivo, cliente de socket Unix, vistas previas por tipo y acciones del portapapeles.
  - `lib/features/preview/`: ejemplos interactivos de diseño ensamblados con controles reutilizables.
- **Backend**: espacio de trabajo Rust (`edition = "2024"`, `resolver = "3"`) en la raíz del repositorio:
  - `lib/src/rust/clipboard/`: demonio observador de portapapeles Wayland (`remya-launcher-clipboard`), persistencia SQLite y servidor IPC.
  - `lib/src/rust/core/`: crate principal de Rust (reservado para lógica compartida del backend del lanzador).

### Requisitos previos y configuración

Asegúrate de tener instalados GTK 3, `gtk-layer-shell`, Python 3, `inter-font` (`sudo pacman -S inter-font` en Arch), FVM y el toolchain de Rust definido en `rust-toolchain.toml`.

```bash
fvm install
fvm flutter pub get
fvm flutter run -d linux
```

Para abrir una vista previa de diseño desde una compilación debug:

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### Validación e instalación local

Ejecuta formateo, análisis estático, pruebas y compilaciones debug y release secuenciales:

```bash
fvm dart format --output=none --set-exit-if-changed .
fvm flutter analyze
fvm flutter test
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
fvm flutter build linux --debug
fvm flutter build linux --release
```

Instala el paquete release, el acceso directo de escritorio y la unidad de usuario de systemd `top.remya.launcher.clipboard.service` bajo `$XDG_DATA_HOME`:

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

Usa `~/.local/bin/remya_launcher --clipboard` para vincularlo a `Super+V` en Hyprland.

## 📜 Licencia

Remya Launcher está licenciado bajo la [PolyForm Noncommercial License 1.0.0](../LICENSE). Los iconos Material Symbols y avisos de terceros se encuentran en `assets/licenses/`.
