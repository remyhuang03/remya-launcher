<p align="center">
  <img src="../assets/icons/app_icon-full.svg" width="128" alt="Remya Launcher">
</p>

<h1 align="center">Remya Launcher</h1>

<h3 align="center">Лаунчер и менеджер буфера обмена в стиле Material Design 3 Expressive для Hyprland</h3>

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
  <strong>Русский</strong> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.ja.md">日本語</a>
</p>

Remya Launcher (`top.remya.launcher`) — это ориентированный на клавиатуру лаунчер приложений, быстрый переключатель и менеджер истории буфера обмена Wayland для Hyprland, созданный на базе Flutter, Rust и Material Design 3 Expressive.

- **Клавиатурный рабочий процесс**: спроектирован так, чтобы руки оставались в основной зоне клавиш, с естественными префиксами областей, быстрым выбором `Alt+1–9` и контекстными подсказками клавиш в нижней панели.
- **Поиск приложений**: ищите установленные приложения в All или нажмите `Ctrl+2` для перехода в Apps. Открывайте новое окно приложения клавишей Enter или `Ctrl+Enter`. Раскрывайте открытые окна и действия рабочего стола клавишами `Ctrl+Right`, затем нажмите Enter на окне, чтобы сфокусироваться на нём. Кандидаты на установку берутся из локальных метаданных пакетов.
- **Встроенная история буфера обмена Wayland**: фоновый наблюдатель на Rust сохраняет скопированный текст, код, ссылки, цвета, файлы и миниатюры изображений с маскированием конфиденциальных данных и хранением политик в Remya Core.
- **Локальный веб-поиск**: поиск по истории браузера и закладкам из подключённых профилей, автодополнение URL или запуск поиска с помощью ключевого слова, например `gh `.

> [!CAUTION]
> **Ранняя разработка**
>
> Поиск приложений, Windows, веб-поиск, калькулятор и буфер обмена используют живые данные. Поиск эмодзи также доступен. Область Files и общее выполнение команд всё ещё находятся в планах. Используйте `--preview`, чтобы изучить [дизайн в Figma](https://www.figma.com/design/aXbt5J1Tf5nGRfJNkrPaWS/Remya-Launcher) с тестовыми данными.

## 🚀 Установка

### Сборка из исходников

Сначала установите **Remya Core**. В Arch Linux следуйте разделам [Требования и настройка](#требования-и-настройка) и [Проверка и локальная установка](#проверка-и-локальная-установка), чтобы собрать и установить Launcher.

### Архив релиза

Для кандидата сборки под Linux x86_64, созданного Linux CI-процессом репозитория, скачайте и распакуйте артефакт кандидата. Он содержит архив `remya-launcher-linux-x86_64-<commit>.tar.zst` и файл `SHA256SUMS`. Из каталога с этими файлами выполните:

```bash
sha256sum -c SHA256SUMS
mkdir -p remya-launcher-candidate
tar --zstd -xf remya-launcher-linux-x86_64-*.tar.zst -C remya-launcher-candidate
cd remya-launcher-candidate
python tool/install.py
```

Откройте настройки Launcher командой `remya_launcher --settings`. Чтобы открыть нужную страницу напрямую, используйте `remya_launcher --settings web`, `clipboard`, `apps`, `hidden-apps` или `material-theme`.

## ✨ Возможности

### Доступно сейчас

- **All и Apps (`Ctrl+2`)**: поиск установленных приложений и запуск по `Enter`. В Apps комбинация `Ctrl+Right` раскрывает открытые окна приложения и действия рабочего стола. Если подходящих приложений нет, кандидаты на установку берутся из локальных метаданных пакетов. В All также отображаются поддерживаемые встроенные вычисления, до трёх локальных результатов Web и прямое действие при вводе URL.
- **Web (`Ctrl+4` / `?`)**:
  - Включите профили семейства Firefox или Chromium в **Launcher Settings → Web**. Даже без включённых профилей вы можете открыть введённый URL или выполнить поиск через поисковую систему. Launcher считывает недавнюю историю и закладки из включённых профилей и поддерживает приватный локальный индекс. Фавиконки берутся из локальных данных браузера, а при их отсутствии отображается монограмма.
  - В **Настройки Launcher → Web** можно выбрать DuckDuckGo или Google как поисковую систему по умолчанию. Ключевые слова применяются только к текущему запросу.
  - Пустой ввод показывает частые сайты (`Alt+1–6`) и недавние страницы. Поиск отображает закладки и историю, отсортированные по релевантности и частоте использования, а затем предложения поисковых систем. `Tab` принимает автодополнение URL; `Enter` открывает введённый URL или выбранную строку; `Ctrl+Enter` вместо этого запускает поиск.
  - Введите ключевое слово поисковой системы и пробел, например `gh ` для GitHub, `aw ` для ArchWiki или `yt ` для YouTube. Когда поисковая система выбрана, `Tab` переключает системы по кругу, а `Backspace` удаляет токен при пустом запросе.
  - `Ctrl+K` открывает действия для страницы: открыть в поддерживаемом браузере или приватном окне, скопировать URL или Markdown-ссылку, выполнить поиск по сайту, закрепить или скрыть из Launcher. Launcher никогда не изменяет историю или закладки браузера. Отключение профиля удаляет его индекс из Launcher.
- **Windows (`Ctrl+7` / `#`)**: фильтрация окон Hyprland по заголовку или классу. Нажмите `Enter`, чтобы сфокусироваться на окне, или `Ctrl+Enter`, чтобы переместить его на текущее рабочее пространство.

- **Живая история буфера обмена (`remya_launcher --clipboard` / `Ctrl+6` / `;`)**:
  - Фоновый наблюдатель на Rust (`remya-launcher-clipboard`), работающий как пользовательский юнит systemd `top.remya.launcher.clipboard.service` в графической сессии.
  - По умолчанию сохраняет до 500 незакреплённых записей на срок до 30 дней в `$XDG_STATE_HOME/remya/launcher/clipboard.sqlite3` (по умолчанию `~/.local/state/remya/launcher/clipboard.sqlite3`); закреплённые элементы не удаляются автоматически.
  - Захватывает данные размером до 64 МиБ и декодированные изображения до 24 мегапикселей (PNG, JPEG, WebP, GIF, BMP); для изображений свыше 20 МБ в пределах этих лимитов сохраняется миниатюра.
  - Автоматически маскирует записи, отмеченные исходным приложением как конфиденциальные, храня их только в памяти до 45 секунд без записи на диск.
  - Предоставляет операции с историей для того же пользователя и `clear` через `$XDG_RUNTIME_DIR/remya-launcher/clipboard.sock`. Политика считывается из `top.remya.Core1` и обновляется по сигналу `Changed`; пока она недоступна или некорректна, захват и автоматическая очистка истории приостанавливаются.
  - Устанавливает `remya/apps/remya-launcher.json` и действие рабочего стола `settings`. Юнит буфера обмена запускается после `top.remya.core.service` и запрашивает его через `Wants`.
  - *Известное ограничение Wayland*: протокол Wayland data-control не сообщает, какое приложение предоставило содержимое буфера обмена, поэтому метки исходных приложений и чёрные списки по приложениям из дизайна не могут быть реализованы надёжно.
- **Калькулятор (`Ctrl+5` / `=`)**:
  - Вычисляет базовые выражения встроенно в `All` и полные запросы в `Calc` (`Ctrl+5` или `=`) локально с помощью математики `fend-core`, типизированных единиц измерения, систем счисления с разрядностью и знаковостью, FP32/FP16/BF16, часовых поясов и справочных снимков курсов ЕЦБ (загружаемых только при явном обновлении), с автодополнениями каталога и историей.
  - Опциональный FP8 и общая календарная арифметика остаются отложенными.
- **Emoji (`:`)**: Офлайн-поиск Emoji 18.0 по названию, ключевому слову, короткому коду или последовательности Unicode на семи языках. Enter копирует эмодзи, Ctrl+Вправо открывает варианты.
  - Вкладка **Symbols** открывает отдельный каталог символов. В **Настройки → Emoji** можно включить вставку по Ctrl+Enter и локальный список недавно использованных эмодзи.
- **Интерактивные превью дизайна (`--preview`)**:
  - Просматривайте все запланированные состояния интерфейса из собранного пакета через `--preview=<name>`:
    - Состояния лаунчера: `empty`, `results`, `calculator`, `files`, `web`, `actions`, `quick-pick`, `clipboard`
    - Варианты превью буфера обмена: `clipboard-text`, `clipboard-code`, `clipboard-image`, `clipboard-link`, `clipboard-color`, `clipboard-files`, `clipboard-hidden`

### Запланированные области и возможности (спецификация дизайна)

- **Files (`Ctrl+3` / `/` / `~`)**: автодополнение путей, нечёткий поиск по именам файлов и предпросмотр каталогов, текста и изображений.
- **Run (`>`)**: Очистка локальной истории буфера обмена.

## ⌨️ Разработка

### Стек технологий и архитектура

- **Фронтенд**: Flutter и Dart (версия `3.50.0` зафиксирована в `.fvmrc`) с `material_ui` (Material Design 3 Expressive) и `flutter_svg`.
  - `lib/app.dart` и `lib/main.dart`: композиция приложения, обработка CLI-флагов (`--clipboard`, `--settings`, `--preview=...`) и регистрация лицензий.
  - `lib/features/launcher/`: панель лаунчера, поле поиска, чипы областей, строки результатов и закреплений, подсказки клавиш, карточка калькулятора, предпросмотр файлов и меню действий.
  - `lib/features/clipboard/`: живой экран буфера обмена, клиент Unix-сокета, превью типов и действия с буфером.
  - `lib/features/web/`: веб-результаты, клавиатурные действия, настройки профилей и клиент веб-сайдкара.
  - `lib/features/preview/`: интерактивные примеры дизайна, собранные из переиспользуемых элементов управления.
- **Бэкенд**: рабочее пространство Rust (`edition = "2024"`, `resolver = "3"`) в корне репозитория:
  - `lib/src/rust/clipboard/`: демон-наблюдатель буфера обмена Wayland (`remya-launcher-clipboard`), хранилище SQLite и IPC-сервер.
  - `lib/src/rust/apps/`: резидентный индекс приложений, поиск пакетов и действия с окнами Hyprland.
  - `lib/src/rust/calculator/`: локальные вычисления, автодополнения и история калькулятора.
  - `lib/src/rust/web/`: веб-сайткар по требованию с ридерами браузеров, приватным индексом SQLite и запуском в браузере.
  - `lib/src/rust/core/`: базовый крейт Rust (зарезервирован для общей логики бэкенда лаунчера).

### Требования и настройка

Разместите копию репозитория Remya Core рядом с этим репозиторием в `../remya-core`, чтобы Flutter мог разрешить зависимости `remya_client` и `remya_ui`. Установите GTK 3, `gtk-layer-shell`, CMake, Ninja, Clang, Python 3, `inter-font`, FVM и тулчейн Rust из `rust-toolchain.toml`.

```bash
fvm install
fvm flutter pub get --enforce-lockfile
fvm flutter run -d linux
```

Запуск превью дизайна из отладочной сборки:

```bash
build/linux/x64/debug/bundle/remya_launcher --preview=files
```

### Проверка и локальная установка

Выполните форматирование, статический анализ, тесты и последовательную сборку debug и release версий:

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

Установите release-пакет, `.desktop`-файл и пользовательские юниты systemd `top.remya.launcher.clipboard.service` и `top.remya.launcher.apps.service` в `$XDG_DATA_HOME`. Калькулятор и Web запускаются по требованию.

```bash
python tool/install.py
~/.local/bin/remya_launcher --clipboard
```

Назначьте `~/.local/bin/remya_launcher --toggle` для открытия или закрытия лаунчера. Используйте `~/.local/bin/remya_launcher --clipboard` для привязки клавиш `Super+V` в Hyprland.

## 📜 Лицензия

Remya Launcher распространяется по лицензии [PolyForm Noncommercial License 1.0.0](../LICENSE). Иконки Material Symbols и уведомления о сторонних компонентах находятся в `assets/licenses/`.
