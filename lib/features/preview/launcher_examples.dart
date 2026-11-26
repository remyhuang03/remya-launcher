import 'package:flutter_svg/flutter_svg.dart';
import 'package:material_ui/material_ui.dart';

import '../clipboard/clipboard_examples.dart';
import '../clipboard/clipboard_preview.dart';
import '../files/file_icon_tile.dart';
import '../launcher/launcher_controls.dart';
import '../launcher/launcher_details.dart';
import '../launcher/launcher_filters.dart';
import '../launcher/launcher_panel.dart';
import '../launcher/launcher_results.dart';

enum LauncherExample {
  empty,
  results,
  calculator,
  files,
  web,
  actions,
  quickPick,
  clipboard,
}

class LauncherExampleView extends StatelessWidget {
  const LauncherExampleView({
    required this.example,
    this.clipboardType = 'command',
    super.key,
  });

  final LauncherExample example;
  final String clipboardType;

  @override
  Widget build(BuildContext context) => switch (example) {
    .empty => _empty(),
    .results => _results(),
    .calculator => _calculator(),
    .files => _files(),
    .web => _web(),
    .actions => _files(showActions: true),
    .quickPick => _results(quickPick: true),
    .clipboard => _clipboard(),
  };

  Widget _empty() => LauncherPanel(
    scope: .all,
    search: const LauncherSearchField(
      autofocus: true,
      trailing: Row(
        spacing: 4,
        children: [
          _Meta('prefix', size: 11),
          LauncherKeyHint(['/', '=', '?', ';']),
        ],
      ),
    ),
    body: _Sections(
      spacing: 8,
      children: [
        const LauncherSectionHeader(
          'PINNED',
          trailing: Row(
            spacing: 4,
            children: [
              _Meta('Alt +', size: 11),
              LauncherKeycap('1'),
              _Meta('…'),
              LauncherKeycap('6'),
            ],
          ),
        ),
        const _PinnedApps(),
        const LauncherSectionHeader(
          'RECENT',
          trailing: LauncherKeyHint(['⇧', 'Del'], label: 'forget'),
        ),
        _Sections(
          children: [
            _fileRow(
              'launcher-spec.md',
              '~/proj/remya-bar/docs',
              isDirectory: false,
              meta: 'File · 12 min ago',
            ),
            _row(
              'end-4/dots-hyprland — GitHub',
              'github.com/end-4/dots-hyprland',
              .public,
              meta: 'Web · 1 h ago',
            ),
            _row(
              '128 GiB in GB = 137.44 GB',
              'Calculator',
              .calculate,
              meta: 'Calc · today',
            ),
            _row(
              'hyprctl reload',
              'Ran in kitty',
              .terminal,
              meta: 'Command · yesterday',
            ),
          ],
        ),
      ],
    ),
    footer: const LauncherFooter(
      leading: [
        LauncherKeyHint(['↑', '↓'], label: 'Navigate'),
        LauncherKeyHint(['↵'], label: 'Open'),
        LauncherKeyHint(['Ctrl', 'Tab'], label: 'Scope'),
      ],
      trailing: [
        LauncherKeyHint(['Ctrl', ','], label: 'Launcher Settings'),
      ],
    ),
  );

  Widget _results({bool quickPick = false}) => LauncherPanel(
    scope: .all,
    search: const LauncherSearchField(
      initialQuery: 'code',
      autofocus: true,
      trailing: _Meta('5 results'),
    ),
    body: _Sections(
      children: [
        _group('APPS', '2', quickPick),
        _row(
          'Visual Studio Code',
          'Code editor · opened 20 min ago',
          .apps,
          leading: const _ExampleAppIcon('vscode', size: 32),
          match: 'Code',
          action: 'Open',
          pick: quickPick ? 1 : null,
        ),
        _row(
          'Antigravity',
          'Agentic code editor',
          .apps,
          leading: const _ExampleAppIcon('antigravity', size: 24),
          meta: 'App',
          pick: quickPick ? 2 : null,
        ),
        _group('FILES', '2', quickPick),
        _fileRow(
          'code',
          '~/code',
          isDirectory: true,
          meta: 'Folder · 41 items',
          match: 'code',
          pick: quickPick ? 3 : null,
        ),
        _fileRow(
          'code-review.md',
          '~/proj/notes',
          isDirectory: false,
          meta: '2 days ago',
          match: 'code',
          pick: quickPick ? 4 : null,
        ),
        _group('WEB HISTORY', '1', quickPick),
        _row(
          'Visual Studio Code — Keyboard shortcuts for Linux',
          'code.visualstudio.com/docs/getstarted/keybindings',
          .public,
          meta: '6 visits',
          match: 'Code',
          pick: quickPick ? 5 : null,
        ),
      ],
    ),
    footer: quickPick
        ? const LauncherFooter(
            leading: [
              LauncherKeyHint(['Alt', '1–5'], label: 'Open result'),
            ],
            trailing: [
              LauncherKeyHint(['Alt', '⇧', 'n'], label: 'Secondary action'),
            ],
          )
        : const LauncherFooter(
            leading: [
              LauncherKeyHint(['↵'], label: 'Open'),
              LauncherKeyHint(['Ctrl', '↵'], label: 'New window'),
              LauncherKeyHint(['Ctrl', 'K'], label: 'Actions'),
            ],
            trailing: [
              LauncherKeyHint(['Alt'], label: 'Quick pick'),
              LauncherKeyHint(['→'], label: 'Complete'),
            ],
          ),
  );

  Widget _calculator() => LauncherPanel(
    scope: .calculator,
    search: const LauncherSearchField(
      scope: .calculator,
      initialQuery: '3.5 GiB in MB',
      autofocus: true,
      trailing: _Meta('units, hex, %, dates', size: 11),
    ),
    body: _Sections(
      spacing: 8,
      children: [
        const LauncherAnswerCard(
          expression: '3.5 GiB  →  MB',
          value: '3,758.10',
          unit: 'MB',
          alternatives: [
            '3,584 MiB',
            '3.76 GB',
            '3.758 × 10⁹ B',
            '0xE0000000 B',
          ],
        ),
        _Sections(
          children: [
            const LauncherSectionHeader(
              'HISTORY',
              trailing: LauncherKeyHint(['↑'], label: 'recall into field'),
            ),
            _row(
              '1920 × 1080 × 4 / 1024²  =  7.91',
              '4K RGBA framebuffer in MiB · 2 h ago',
              .calculate,
            ),
            _row('0x1F4  =  500', 'hex → dec · yesterday', .calculate),
            _row('(42 − 7) × 1.08  =  37.8', 'yesterday', .calculate),
          ],
        ),
      ],
    ),
    footer: const LauncherFooter(
      leading: [
        LauncherKeyHint(['↵'], label: 'Copy result'),
        LauncherKeyHint(['Ctrl', '↵'], label: 'Copy expression'),
        LauncherKeyHint(['Tab'], label: 'Next unit'),
      ],
      trailing: [
        LauncherKeyHint(['Esc'], label: 'Leave Calc'),
      ],
    ),
  );

  Widget _files({bool showActions = false}) => LauncherPanel(
    width: 880,
    scope: .files,
    search: const LauncherSearchField(
      scope: .files,
      initialQuery: '~/proj/remya-bar/docs/',
      autofocus: true,
      trailing: LauncherKeyHint(['→'], label: 'complete'),
    ),
    body: LauncherSplitBody(
      previewWidth: 248,
      results: _Sections(
        children: [
          const LauncherSectionHeader(
            '~/proj/remya-bar (sample)',
            height: 26,
            trailing: _Meta('4 items', size: 11),
          ),
          _fileRow(
            'docs/',
            '~/proj/remya-bar/docs',
            isDirectory: true,
            match: 'do',
            action: 'Open',
          ),
          _fileRow(
            'dock-layout.qml',
            '~/proj/remya-bar/src/bar',
            isDirectory: false,
            match: 'do',
            meta: '6.2 KB · 3 h ago',
          ),
          _fileRow(
            'docker-compose.yml',
            '~/proj/remya-bar',
            isDirectory: false,
            match: 'do',
            meta: '812 B · last week',
          ),
          _fileRow(
            'file_icon_tile.dart',
            '~/proj/remya-launcher/lib/features/files',
            isDirectory: false,
            meta: '4.8 KB · 1 h ago',
          ),
          const LauncherSectionHeader(
            'ALSO MATCHING (SAMPLE)',
            height: 26,
            trailing: _Meta('static preview', size: 11),
          ),
          _fileRow(
            'lib.rs',
            '~/proj/remya-core/crates/service/src',
            isDirectory: false,
            meta: '3.1 KB · yesterday',
          ),
          _fileRow(
            'fuse_attention.py',
            '~/proj/ml/kernels',
            isDirectory: false,
            meta: '1.9 KB · 2 days ago',
          ),
          _fileRow(
            '.editorconfig',
            '~/proj/remya-bar',
            isDirectory: false,
            meta: '268 B · last month',
          ),
          _fileRow(
            'dock-preview.png',
            '~/Pictures/Screenshots',
            isDirectory: false,
            match: 'do',
            meta: '1.4 MB · today',
          ),
        ],
      ),
      preview: Opacity(
        opacity: showActions ? .35 : 1,
        child: const LauncherFilePreview(
          title: 'docs',
          summary: '3 items · modified 2 h ago',
          path: '~/proj/remya-bar/docs',
          entries: ['launcher-spec.md', 'keyboard-map.md', 'ai-quota.md'],
        ),
      ),
    ),
    footer: showActions
        ? const LauncherFooter(
            leading: [
              LauncherKeyHint(['↑', '↓'], label: 'Choose'),
              LauncherKeyHint(['↵'], label: 'Run action'),
              LauncherKeyHint(['Esc'], label: 'Back to results'),
            ],
          )
        : const LauncherFooter(
            leading: [
              LauncherKeyHint(['→'], label: 'Complete'),
              LauncherKeyHint(['↵'], label: 'Open'),
              LauncherKeyHint(['Ctrl', '↵'], label: 'Terminal here'),
              LauncherKeyHint(['Ctrl', 'O'], label: 'Reveal'),
            ],
            trailing: [
              LauncherKeyHint(['Ctrl', 'Space'], label: 'Preview'),
            ],
          ),
    actions: showActions
        ? const LauncherActionsMenu(
            title: 'docs/',
            actions: [
              LauncherAction('Open', .openNew, ['↵']),
              LauncherAction('Open terminal here', .terminal, ['Ctrl', '↵']),
              LauncherAction('Reveal in file manager', .folder, ['Ctrl', 'O']),
              LauncherAction('Copy path', .copy, ['Ctrl', 'C']),
              LauncherAction('Open in editor', .code, ['Ctrl', 'E']),
              LauncherAction('Pin to launcher', .star, ['Ctrl', 'D']),
              LauncherAction('Move to trash', .trash, [
                'Ctrl',
                'Del',
              ], destructive: true),
            ],
          )
        : null,
  );

  Widget _web() => LauncherPanel(
    scope: .web,
    search: const LauncherSearchField(
      scope: .web,
      initialQuery: 'dots hypr',
      autofocus: true,
      trailing: _Meta('3 in history'),
    ),
    body: _Sections(
      children: [
        const LauncherSectionHeader(
          'HISTORY',
          height: 26,
          trailing: _Meta('Firefox · frecency', size: 11),
        ),
        _row(
          'end-4/dots-hyprland — GitHub',
          'github.com/end-4/dots-hyprland',
          .public,
          match: 'dots|hypr',
          action: 'Open',
        ),
        _row(
          'Hyprland Wiki — Dispatchers',
          'wiki.hyprland.org/Configuring/Dispatchers',
          .public,
          match: 'Hypr',
          meta: '9 visits',
        ),
        _row(
          'dots-hyprland Discussions · Quickshell port',
          'github.com/end-4/dots-hyprland/discussions',
          .public,
          match: 'dots|hypr',
          meta: 'last week',
        ),
        const LauncherSectionHeader(
          'SEARCH',
          trailing: LauncherKeyHint(['Ctrl', '↵']),
        ),
        _row(
          'Search the web for “dots hypr”',
          'DuckDuckGo · change in settings',
          .search,
        ),
      ],
    ),
    footer: const LauncherFooter(
      leading: [
        LauncherKeyHint(['↵'], label: 'Open'),
        LauncherKeyHint(['Ctrl', '↵'], label: 'Search instead'),
        LauncherKeyHint(['Ctrl', 'C'], label: 'Copy URL'),
      ],
      trailing: [
        LauncherKeyHint(['⇧', 'Del'], label: 'Forget'),
      ],
    ),
  );

  Widget _clipboard() => LauncherPanel(
    width: 960,
    scope: .clipboard,
    search: const LauncherSearchField(
      scope: .clipboard,
      placeholder: 'Filter clipboard…',
      autofocus: true,
      trailing: _Meta('31 items'),
    ),
    body: LauncherSplitBody(
      previewWidth: 320,
      results: _Sections(
        children: [
          const LauncherFilters(
            shortcut: LauncherKeyHint(['Ctrl', 'F']),
            counts: {
              'All': 31,
              'Text': 18,
              'Images': 4,
              'Links': 6,
              'Files': 2,
              'Colors': 1,
            },
            selected: 'All',
          ),
          const LauncherSectionHeader(
            'PINNED',
            trailing: LauncherKeyHint(['Ctrl', 'D'], label: 'pin / unpin'),
          ),
          _row(
            'Best regards, Huang Lixing — Nanjing University',
            'Text · 2 lines · pinned',
            .text,
            meta: 'Pinned',
            action: clipboardType == 'text' ? 'Paste' : null,
          ),
          const LauncherSectionHeader('TODAY', height: 26),
          _row(
            'pnpm add -D @types/node',
            'Command · 1 line · from kitty',
            .terminal,
            monospace: true,
            action: clipboardType == 'command' ? 'Paste' : null,
          ),
          _row(
            'Screenshot 1920 × 1080',
            'Image · PNG · 1.2 MB · from Remy\'a Screen Capture',
            .image,
            leading: const ClipboardImageThumbnail(square: true),
            meta: '5 min',
            action: clipboardType == 'image' ? 'Paste' : null,
          ),
          _row(
            'github.com/steipete/CodexBar',
            'Link · from Firefox · visited 6×',
            .public,
            meta: '12 min',
            action: clipboardType == 'link' ? 'Paste' : null,
          ),
          _row(
            '#6750A4',
            'Color · rgb(103 80 164) · from Figma',
            .palette,
            leading: const _ColorSwatch(),
            meta: '40 min',
            monospace: true,
            action: clipboardType == 'color' ? 'Paste' : null,
          ),
          _row(
            '3 files — launcher-spec.md, keyboard-map.md, +1',
            'Files · text/uri-list · from Nautilus',
            .folder,
            meta: '1 h',
            action: clipboardType == 'files' ? 'Paste' : null,
          ),
          _row(
            '••••••••••••',
            'Hidden · from KeePassXC · clears in 0:42',
            .lock,
            meta: '1 h',
            action: clipboardType == 'hidden' ? 'Paste' : null,
          ),
          _row(
            'def fuse_attention(q, k, v, mask=None):',
            'Code · Python · 14 lines · from VS Code',
            .code,
            meta: '2 h',
            monospace: true,
            action: clipboardType == 'code' ? 'Paste' : null,
          ),
          const LauncherSectionHeader(
            'YESTERDAY',
            height: 26,
            trailing: _Meta('23 more'),
          ),
        ],
      ),
      preview: clipboardPreviewExample(clipboardType),
    ),
    footer: const LauncherFooter(
      leading: [
        LauncherKeyHint(['↵'], label: 'Paste'),
        LauncherKeyHint(['Ctrl', '↵'], label: 'Run in terminal'),
        LauncherKeyHint(['Ctrl', 'D'], label: 'Pin'),
        LauncherKeyHint(['⇧', 'Del'], label: 'Delete'),
      ],
      trailing: [
        LauncherKeyHint(['Ctrl', 'Space'], label: 'Reveal hidden'),
      ],
    ),
  );
}

class _Sections extends StatelessWidget {
  const _Sections({required this.children, this.spacing = 2});

  final List<Widget> children;
  final double spacing;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: .stretch,
    spacing: spacing,
    children: children,
  );
}

Widget _group(String title, String shortcut, bool quickPick) =>
    LauncherSectionHeader(
      title,
      trailing: Opacity(
        opacity: quickPick ? .45 : 1,
        child: LauncherKeyHint(['Ctrl', shortcut]),
      ),
    );

Widget _fileRow(
  String title,
  String subtitle, {
  required bool isDirectory,
  String? meta,
  String? action,
  String? match,
  int? pick,
}) => _row(
  title,
  subtitle,
  isDirectory ? .folder : .description,
  leading: FileIconTile(
    basename: title,
    isDirectory: isDirectory,
    selected: action != null,
  ),
  meta: meta,
  action: action,
  match: match,
  pick: pick,
);

Widget _row(
  String title,
  String subtitle,
  LauncherIconName symbol, {
  Widget? leading,
  String? meta,
  String? action,
  String? match,
  int? pick,
  bool monospace = false,
}) => Builder(
  builder: (context) {
    final colors = Theme.of(context).colorScheme;
    return LauncherResultRow(
      title: match == null
          ? TextSpan(text: title)
          : TextSpan(
              children: _highlight(title, RegExp(match), colors.primary),
            ),
      subtitle: subtitle,
      leading: leading ?? LauncherSymbolTile(symbol, selected: action != null),
      selected: action != null,
      monospaced: monospace,
      trailing: Row(
        mainAxisSize: .min,
        spacing: 8,
        children: [
          if (action case final action?)
            Row(
              spacing: 8,
              children: [
                Text(
                  action,
                  style: TextStyle(
                    fontSize: 12,
                    color: colors.onSecondaryContainer,
                  ),
                ),
                const LauncherKeycap('↵'),
              ],
            )
          else if (meta case final meta?)
            _Meta(meta),
          if (pick case final pick?) LauncherKeycap('$pick'),
        ],
      ),
    );
  },
);

List<TextSpan> _highlight(String text, RegExp pattern, Color color) {
  final spans = <TextSpan>[];
  var offset = 0;
  for (final match in pattern.allMatches(text)) {
    spans.add(TextSpan(text: text.substring(offset, match.start)));
    spans.add(
      TextSpan(
        text: text.substring(match.start, match.end),
        style: TextStyle(color: color, fontWeight: .w600),
      ),
    );
    offset = match.end;
  }
  spans.add(TextSpan(text: text.substring(offset)));
  return spans;
}

class _Meta extends StatelessWidget {
  const _Meta(this.text, {this.size = 12});

  final String text;
  final double size;

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: Theme.of(context).textTheme.bodySmall?.copyWith(fontSize: size),
  );
}

class _PinnedApps extends StatelessWidget {
  const _PinnedApps();

  static const apps = [
    ('Code', 'vscode'),
    ('kitty', 'kitty'),
    ('Chrome', 'chrome'),
    ('Telegram', 'telegram'),
    ('Antigravity', 'antigravity'),
    ('Tasks', 'tasks'),
  ];

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) => SingleChildScrollView(
      scrollDirection: .horizontal,
      padding: const .symmetric(horizontal: 4),
      child: Row(
        spacing: 8,
        children: [
          for (final (index, (label, icon)) in apps.indexed)
            SizedBox(
              width: ((constraints.maxWidth - 48) / 6).clamp(
                88,
                double.infinity,
              ),
              child: LauncherPinnedTile(
                label: label,
                icon: _ExampleAppIcon(
                  icon,
                  size: icon == 'vscode' || icon == 'chrome' ? 36 : 24,
                ),
                shortcut: '${index + 1}',
                selected: index == 0,
              ),
            ),
        ],
      ),
    ),
  );
}

class _ExampleAppIcon extends StatelessWidget {
  const _ExampleAppIcon(this.name, {required this.size});

  final String name;
  final double size;

  @override
  Widget build(BuildContext context) => Center(
    child: name == 'chrome'
        ? SizedBox.square(
            dimension: size,
            child: FittedBox(
              child: SvgPicture.asset('assets/preview/chrome.svg'),
            ),
          )
        : Image.asset(
            'assets/preview/$name.png',
            width: size,
            height: size,
            fit: .contain,
          ),
  );
}

class _ColorSwatch extends StatelessWidget {
  const _ColorSwatch();

  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: BoxDecoration(
      color: const Color(0xff6750a4),
      shape: .circle,
      border: Border.all(color: Theme.of(context).colorScheme.outlineVariant),
    ),
  );
}
