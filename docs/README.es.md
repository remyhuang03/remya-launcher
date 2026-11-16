<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">Un lanzador y gestor de portapapeles con Material Design 3 Expressive para Hyprland</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Hyprland-1793D1?style=flat-square&logo=archlinux&logoColor=white" alt="Platform: Arch Linux | Hyprland">
  <img src="https://img.shields.io/badge/Flutter-3.50.0-02569B?style=flat-square&logo=flutter&logoColor=white" alt="Flutter 3.50.0">
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

Remya Launcher (`top.remya.launcher`) es un lanzador de aplicaciones centrado en el teclado, selector rápido y gestor de historial del portapapeles en Wayland diseñado para Hyprland con Flutter, Rust y Material Design 3 Expressive.

- **Flujo centrado en el teclado**: diseñado para que tus manos permanezcan en la zona principal de teclas, con ámbitos por prefijo natural, selección rápida `Alt+1–9` e indicaciones contextuales en el pie.
- **Búsqueda de aplicaciones**: busca aplicaciones instaladas en All o pulsa `Ctrl+2` para Apps. Abre una nueva ventana de la aplicación con Enter o `Ctrl+Enter`. Expande sus ventanas y acciones de escritorio con `Ctrl+Right` y pulsa Enter sobre una ventana para enfocarla. Los candidatos de instalación provienen de los metadatos de paquetes locales.
- **Historial del portapapeles en Wayland integrado**: un observador en Rust en segundo plano guarda texto, código, enlaces, colores, archivos y miniaturas de imágenes copiados, con enmascarado de datos sensibles y almacenamiento de políticas de Remya Core.
- **Búsqueda web local**: busca en el historial del navegador y marcadores de los perfiles que habilites, completa URL o inicia una búsqueda con una palabra clave como `gh `.

> [!CAUTION]
> **Desarrollo temprano**
>
> La búsqueda de aplicaciones, Windows, la búsqueda web, Calculator y Clipboard usan datos en vivo. Los ámbitos Files y Emoji y la ejecución general de comandos siguen planificados. Usa `--preview` para explorar el [diseño en Figma](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) con datos de ejemplo.

## 🚀 Instalación

### Compilación desde el código fuente

Instala primero **Remya Core**. En Arch Linux, sigue [Requisitos previos y configuración](#requisitos-previos-y-configuración) y [Validación e instalación local](#validación-e-instalación-local) para compilar e instalar Launcher.

### Archivo de lanzamiento

Para un candidato Linux x86_64 compilado por el flujo de trabajo de CI en Linux del repositorio, descarga y descomprime el artefacto candidato. Contiene un archivo `remya-launcher-linux-x86_64-<commit>.tar.zst` y `SHA256SUMS`. Desde el directorio con esos archivos:

```bash
sha256sum -c SHA256SUMS
mkdir -p remya-launcher-candidate
tar --zstd -xf remya-launcher-linux-x86_64-*.tar.zst -C remya-launcher-candidate
cd remya-launcher-candidate
python tool/install.py
```

Abre la configuración de Launcher con `remya_launcher --settings`. Abre una página directamente con `remya_launcher --settings web`, `clipboard`, `apps`, `hidden-apps` o `material-theme`.

## ✨ Características

### Disponible actualmente

- **All y Apps (`Ctrl+2`)**: busca aplicaciones instaladas y ábrelas con `Enter`. En Apps, `Ctrl+Right` expande las ventanas abiertas y acciones de escritorio de una aplicación. Si no hay aplicaciones coincidentes, los candidatos de instalación provienen de los metadatos de paquetes locales. All también muestra cálculos en línea compatibles y hasta tres resultados web locales, además de una acción directa para entradas de URL.
- **Web (`Ctrl+4` / `?`)**:
  - Habilita perfiles de la familia Firefox o Chromium en **Launcher Settings → Web**. Sin perfiles habilitados, aún puedes abrir una URL escrita o buscar con un motor. Launcher lee el historial reciente y los marcadores de los perfiles habilitados y mantiene un índice local privado. Lee los favicons de los datos locales del navegador y muestra un monograma cuando no hay ninguno disponible.
  - En **Ajustes de Launcher → Web** puedes elegir DuckDuckGo o Google como buscador predeterminado. Las palabras clave solo cambian el buscador de la consulta actual.
  - La entrada vacía muestra sitios frecuentes (`Alt+1–6`) y páginas recientes. La búsqueda muestra marcadores e historial ordenados por relevancia y uso, seguidos de sugerencias de motores. `Tab` acepta el autocompletado de URL; `Enter` abre la URL escrita o la fila seleccionada; `Ctrl+Enter` busca en su lugar.
  - Escribe la palabra clave de un motor seguida de un espacio, como `gh ` para GitHub, `aw ` para ArchWiki o `yt ` para YouTube. Con un motor seleccionado, `Tab` alterna entre motores y `Backspace` elimina el token cuando la consulta está vacía.
  - `Ctrl+K` abre acciones para una página: abrir en un navegador compatible o ventana privada, copiar la URL o el enlace Markdown, buscar en el sitio, fijarla u ocultarla de Launcher. Launcher nunca modifica el historial ni los marcadores del navegador. Desactivar un perfil elimina su índice de Launcher.
- **Windows (`Ctrl+7` / `#`)**: filtra ventanas de Hyprland por título o clase. Pulsa `Enter` para enfocar una ventana o `Ctrl+Enter` para moverla al espacio de trabajo actual.

- **Historial del portapapeles en vivo (`remya_launcher --clipboard` / `Ctrl+6` / `;`)**:
  - Observador en segundo plano en Rust (`remya-launcher-clipboard`) ejecutándose como la unidad de usuario de systemd `top.remya.launcher.clipboard.service` junto con la sesión gráfica.
  - Conserva por defecto hasta 500 elementos no fijados durante 30 días en `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3` (por defecto `~/.local/state/remya/launcher/clipboard.sqlite3`); los elementos fijados quedan exentos de la limpieza automática.
  - Captura selecciones de hasta 64 MiB e imágenes decodificadas de hasta 24 megapíxeles (PNG, JPEG, WebP, GIF, BMP); las imágenes de más de 20 MB dentro de esos límites conservan una miniatura.
  - Enmascara automáticamente en memoria durante hasta 45 segundos los elementos marcados como sensibles por la aplicación de origen sin escribirlos en disco.
  - Expone operaciones de historial para el mismo usuario y `clear` en `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock`. La política se lee desde `top.remya.Core1` y se actualiza con `Changed`; mientras no esté disponible o no sea válida, la captura y la limpieza automática del historial se pausan.
  - Instala `remya/apps/remya-launcher.json` y una acción de escritorio `settings`. La unidad del portapapeles se inicia después de `top.remya.core.service` y la requiere como dependencia débil (`Wants`).
  - *Limitación conocida de Wayland*: Wayland data-control no identifica qué aplicación proporcionó una selección del portapapeles, por lo que las etiquetas de aplicación de origen y las listas de exclusión por aplicación del diseño no pueden implementarse de forma fiable.
- **Calculadora (`Ctrl+5` / `=`)**:
  - Evalúa expresiones básicas en línea en `All` y consultas completas en `Calc` (`Ctrl+5` o `=`) localmente mediante cálculo con `fend-core`, unidades tipadas, bases con ancho de bits y signo, FP32/FP16/BF16, zonas horarias e instantáneas de referencia del BCE obtenidas solo bajo actualización explícita, con autocompletado de catálogo e historial.
  - El soporte opcional de FP8 y la aritmética general de calendario siguen pospuestos.
- **Vistas previas interactivas de diseño (`--preview`)**:
  - Inspecciona todos los estados de interfaz planeados desde un paquete compilado usando `--preview=<name>`:
    - Estados del lanzador: `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, `clipboard`
    - Vistas previas por tipo de portapapeles: `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, `clipboard-hidden`

### Ámbitos y capacidades planificadas (especificación de diseño)

- **Files (`Ctrl+3` / `/` / `~`)**: autocompletado de rutas, búsqueda difusa de nombres de archivo y vistas previas para carpetas, texto e imágenes.
- **Emoji (`:`)** y **Run (`$` / `>`)**: búsqueda de emojis y símbolos y ejecución general de comandos. El ámbito Run en vivo actualmente ofrece la acción local de limpieza del portapapeles.

## ⌨️ Desarrollo

### Stack tecnológico y arquitectura

- **Frontend**: Flutter y Dart (fijado mediante `.fvmrc` a `3.50.0`) con `material_ui` (Material Design 3 Expressive) y `flutter_svg`.
  - `lib/app.dart` y `lib/main.dart`: composición de la aplicación, manejo de opciones CLI (`--clipboard`, `--settings`, `--preview=...`) y registro de licencias.
  - `lib/features/launcher/`: panel del lanzador, campo de búsqueda, chips de ámbito, filas de resultados y fijados, indicaciones de teclas, tarjeta de calculadora, vista previa de archivos y menú de acciones.
  - `lib/features/clipboard/`: página del portapapeles en vivo, cliente de socket Unix, vistas previas por tipo y acciones del portapapeles.
  - `lib/features/web/`: resultados web, acciones de teclado, configuración de perfiles y el cliente sidecar de Web.
  - `lib/features/preview/`: ejemplos interactivos de diseño ensamblados con controles reutilizables.
- **Backend**: espacio de trabajo Rust (`edition = "2024"`, `resolver = "3"`) en la raíz del repositorio:
  - `lib/src/rust/clipboard/`: demonio observador de portapapeles Wayland (`remya-launcher-clipboard`), persistencia SQLite y servidor IPC.
  - `lib/src/rust/apps/`: índice residente de aplicaciones, consultas de paquetes y acciones de ventanas en Hyprland.
  - `lib/src/rust/calculator/`: evaluación local, autocompletado e historial de la calculadora.
  - `lib/src/rust/web/`: sidecar de Web bajo demanda con lectores de navegadores, índice SQLite privado y envío al navegador.
  - `lib/src/rust/core/`: crate principal de Rust (reservado para lógica compartida del backend del lanzador).

### Requisitos previos y configuración

Coloca el repositorio clonado de Remya Core junto a este repositorio como `../remya-core` para que Flutter pueda resolver `remya_client` y `remya_ui`. Instala GTK 3, `gtk-layer-shell`, CMake, Ninja, Clang, Python 3, `inter-font`, FVM y el toolchain de Rust indicado en `rust-toolchain.toml`.

```bash
fvm install
fvm flutter pub get --enforce-lockfile
fvm flutter run -d linux
```

Para inspeccionar una vista previa de diseño desde una compilación debug:

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### Validación e instalación local

Ejecuta formateo, análisis estático, pruebas y compilaciones debug y release secuenciales:

```bash
fvm dart format --output=none --set-exit-if-changed lib test
fvm flutter analyze
fvm flutter test
python -m unittest tool.test_install
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
fvm flutter build linux --debug
fvm flutter build linux --release
```

Instala el paquete release, el acceso directo de escritorio y las unidades de usuario de systemd `top.remya.launcher.clipboard.service` y `top.remya.launcher.apps.service` bajo `$XDG_DATA_HOME`. Calculator y Web se ejecutan bajo demanda.

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

Vincula `~/.local/bin/remya_launcher --toggle` para abrir o cerrar el lanzador. Usa `~/.local/bin/remya_launcher --clipboard` para un atajo `Super+V` en Hyprland.

## 📜 Licencia

Remya Launcher está licenciado bajo la [PolyForm Noncommercial License 1.0.0](../LICENSE). Los iconos Material Symbols y avisos de terceros se encuentran en `assets/licenses/`.
