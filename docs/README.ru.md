<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">Лаунчер и менеджер буфера обмена в стиле Material Design 3 Expressive для Hyprland</h3>

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
  <strong>Русский</strong> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.ja.md">日本語</a>
</p>

Remya Launcher (`top.remya.launcher`) — это ориентированный на клавиатуру лаунчер приложений, быстрый переключатель и менеджер истории буфера обмена Wayland для Hyprland, созданный на базе Flutter, Rust и Material Design 3 Expressive.

- **Клавиатурный рабочий процесс**: спроектирован так, чтобы руки оставались в основной зоне клавиш, с естественными префиксами областей, быстрым выбором `Alt+1–9` и контекстными подсказками в нижней панели.
- **Встроенная история буфера обмена Wayland**: фоновый демон на Rust сохраняет скопированный текст, код, ссылки, цвета, файлы и миниатюры изображений с маскированием конфиденциальных данных и интеграцией с Remya Settings.
- **Material Design 3 Expressive**: плавный морфинг форм, типографика Inter и JetBrains Mono, а также оверлей `gtk-layer-shell`, закреплённый на 22% высоты экрана.

> [!CAUTION]
> **Early Development & Preview Only — Not Ready for Daily Use (Ранняя разработка и предварительный просмотр — не готово к ежедневному использованию)**
>
> Remya Launcher находится на раннем этапе активной разработки. На данный момент реализованы **калькулятор**, **живая история буфера обмена** (демон на Rust и живой экран Clipboard) и **интерактивные UI-превью** (`--preview`). Большинство остальных областей поиска, поисковых бэкендов и интеграций с окнами и действиями Hyprland, описанных ниже, основаны на запланированной [спецификации дизайна Remya Launcher в Figma](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) и ещё не реализованы.

## 🚀 Установка

### AUR (канал `dev`)

В Arch Linux установите пакет разработки, когда он станет доступен в AUR:

```bash
paru -S remya-launcher-git || yay -S remya-launcher-git
```

### Архив релиза

Готовые архивы релизов пока не опубликованы. После появления сборок на [GitHub Releases](https://github.com/remyhuang03/remya-launcher/releases) проверьте контрольную сумму, распакуйте архив и запустите локальный установщик:

```bash
sha256sum -c SHA256SUMS --ignore-missing
tar --zstd -xf remya_launcher-0.1.0+1-linux-x86_64.tar.zst
cd remya_launcher-0.1.0+1-linux-x86_64
python tool/install.py
```

При совместной установке компонентов рабочего стола Remya сначала установите **Remya Settings**, затем **Remya Bar**. Параметры хранения и захвата буфера обмена настраиваются в **Remya Settings → System → Clipboard**.

## ✨ Возможности

### Доступно сейчас

- **Живая история буфера обмена (`remya_launcher --clipboard` / `Ctrl+6` / `;`)**:
  - Фоновый наблюдатель на Rust (`remya-launcher-clipboard`), работающий как пользовательский юнит systemd `top.remya.launcher.clipboard.service` в графической сессии.
  - По умолчанию сохраняет до 500 незакреплённых записей на срок до 30 дней в `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3` (по умолчанию `~/.local/state/remya/launcher/clipboard.sqlite3`); закреплённые элементы не удаляются автоматически.
  - Захватывает данные размером до 64 МиБ и декодированные изображения до 24 мегапикселей (PNG, JPEG, WebP, GIF, BMP); для изображений свыше 20 МБ в пределах этих лимитов сохраняется миниатюра.
  - Записи, отмеченные исходным приложением как конфиденциальные, хранятся только в оперативной памяти с маскированием до 45 секунд и не записываются на диск.
  - Предоставляет IPC-интерфейс JSON через Unix-сокет `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock` для того же пользователя и взаимодействия с Remya Settings (`preferences`, `setPreference` для `enabled`/`maxItems`/`retentionDays` и `clear`).
  - *Известное ограничение Wayland*: протокол Wayland data-control не сообщает, какое приложение предоставило содержимое буфера обмена, поэтому метки исходных приложений и чёрные списки по приложениям из дизайна не могут быть реализованы надёжно.
- **Калькулятор (`Ctrl+5` / `=`)**:
  - Вычисляет базовые выражения встроенно в `All` и полные запросы в `Calc` (`Ctrl+5` или `=`) локально с помощью математики `fend-core`, типизированных единиц измерения, систем счисления с разрядностью и знаковостью, FP32/FP16/BF16, часовых поясов, справочных снимков курсов ЕЦБ (загружаемых только при явном обновлении), автодополнений каталога и истории.
  - Опциональный FP8 и общая календарная арифметика остаются отложенными.
- **Интерактивные превью дизайна (`--preview`)**:
  - Просматривайте все запланированные состояния интерфейса из собранного пакета через `--preview=<name>`:
    - Состояния лаунчера: `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, `clipboard`
    - Варианты превью буфера обмена: `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, `clipboard-hidden`

### Запланированные области и функции (спецификация дизайна)

Следующие возможности запланированы согласно [спецификации дизайна Remya Launcher](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher):

- **Естественные префиксные области (Scopes)**:
  - **All (`Ctrl+1`)**: смешанная выдача приложений, файлов, истории браузера и команд (до 3 элементов в группе с динамической сортировкой по лучшему совпадению) и распознавание URL.
  - **Apps (`Ctrl+2`)**: нечёткий поиск по XDG `.desktop` (имя, `GenericName`, `Keywords`, исполняемый файл) с учётом frecency и поддержкой дополнительных действий `.desktop` (например, новое приватное окно).
  - **Files (`Ctrl+3`, префикс `/` или `~`)**: нативное автодополнение путей (`Tab` по сегментам), нечёткий поиск по именам файлов (`fd` + недавние) и правая панель предпросмотра (содержимое каталогов, фрагменты текста, миниатюры изображений) с расширением окна до 880 px.
  - **Web (`Ctrl+4`, префикс `?`)**: локальная история браузера только для чтения (Firefox `places.sqlite` / Chromium `History`) с ранжированием по frecency и резервным поиском в сети (`Ctrl+Enter`).
  - **Clipboard (`Ctrl+6`, префикс `;`)**: двухпанельный режим шириной 960 px с фильтрами типов (`All · Text · Images · Links · Files · Colors` через `Ctrl+F`), группировкой `Pinned → Today → Yesterday → Earlier`, объединением дубликатов со счётчиком копирований, предпросмотром по типам (подсветка кода с номерами строк, палитры HEX/RGB/HSL/OKLCH, шахматный фон для изображений) и прямой вставкой в предыдущее активное окно (`Enter` или `Ctrl+Enter` для вторичного действия).
  - **Windows (`Ctrl+7`, префикс `#`)**: поиск окон через `hyprctl clients` по заголовку и классу с индикатором рабочего пространства; `Enter` фокусирует окно и переключает рабочий стол, `Ctrl+Enter` переносит окно на текущее рабочее пространство.
  - **Emoji (префикс `:`)** и **Run (префикс `$` / `>`)**: поиск эмодзи/символов по имени и псевдонимам, а также прямой запуск команд в оболочке или терминале.
- **Навигация с приоритетом клавиатуры**:
  - `↑`/`↓` или `Ctrl+N`/`Ctrl+P` для перехода по спискам; `Ctrl+↑`/`Ctrl+↓` для прыжков к началу предыдущей/следующей группы.
  - Удерживайте `Alt` для отображения номеров строк и нажмите `Alt+1–9` для мгновенного выполнения основного действия (`Alt+1–6` для закреплённых элементов на пустом экране), либо `Ctrl+Alt+1–9` для вторичного действия.
  - `→` или `Ctrl+K` открывает фильтруемое меню действий; `Ctrl+C` копирует значение; `Ctrl+D` закрепляет/открепляет; `Shift+Del` удаляет из Recent или истории.
  - `Ctrl+W` / `Ctrl+Backspace` удаляет слово или сегмент пути; `Ctrl+U` очищает запрос; `Esc` пошагово закрывает меню → запрос → область → лаунчер; `?` при пустом вводе открывает полную шпаргалку по клавишам.
- **Оверлей Hyprland и визуальная система**:
  - Оверлей `gtk-layer-shell` с эксклюзивным захватом клавиатуры, базовой шириной 720 px, изменением высоты только вниз (250 мс) и закрытием по клику снаружи или потере фокуса.
  - Морфинг форм Material Design 3 Expressive (`20 px → 12 px` для выбранных строк и закреплённых плиток, `16 px → 10 px` для чипов областей) и динамические цветовые токены, синхронизированные с темой рабочего стола Remya.

## ⌨️ Разработка

### Стек технологий и архитектура

- **Фронтенд**: Flutter и Dart (версия `3.47.8` зафиксирована в `.fvmrc`) с `material_ui` (Material Design 3 Expressive) и `flutter_svg`.
  - `lib/app.dart` и `lib/main.dart`: композиция приложения, обработка CLI-флагов (`--clipboard`, `--preview=...`) и регистрация лицензий.
  - `lib/features/launcher/`: панель лаунчера, поле поиска, чипы областей, строки результатов и закреплений, подсказки клавиш, карточка калькулятора, предпросмотр файлов и меню действий.
  - `lib/features/clipboard/`: живой экран буфера обмена, клиент Unix-сокета, превью типов и действия с буфером.
  - `lib/features/preview/`: интерактивные примеры дизайна на базе переиспользуемых компонентов.
- **Бэкенд**: рабочее пространство Rust (`edition = "2024"`, `resolver = "3"`) в корне репозитория:
  - `lib/src/rust/clipboard/`: демон-наблюдатель буфера обмена Wayland (`remya-launcher-clipboard`), хранилище SQLite и IPC-сервер.
  - `lib/src/rust/core/`: базовый крейт Rust (зарезервирован для общей логики бэкенда лаунчера).

### Требования и настройка

Убедитесь, что в системе установлены GTK 3, `gtk-layer-shell`, Python 3, `inter-font` (в Arch: `sudo pacman -S inter-font`), FVM и тулчейн Rust из `rust-toolchain.toml`.

```bash
fvm install
fvm flutter pub get
fvm flutter run -d linux
```

Запуск превью дизайна из отладочной сборки:

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### Проверка и локальная установка

Выполните форматирование, статический анализ, тесты и последовательную сборку debug и release версий:

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

Установите release-пакет, `.desktop`-файл и пользовательский юнит systemd `top.remya.launcher.clipboard.service` в `$XDG_DATA_HOME`:

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

Используйте `~/.local/bin/remya_launcher --clipboard` для привязки к `Super+V` в Hyprland.

## 📜 Лицензия

Remya Launcher распространяется по лицензии [PolyForm Noncommercial License 1.0.0](../LICENSE). Иконки Material Symbols и уведомления о сторонних компонентах находятся в `assets/licenses/`.
