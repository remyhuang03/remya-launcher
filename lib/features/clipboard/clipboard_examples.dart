import 'package:material_ui/material_ui.dart';

import '../files/file_icon_tile.dart';
import '../launcher/launcher_controls.dart';
import 'clipboard_preview.dart';

Widget clipboardPreviewExample(String type) => switch (type) {
  'text' => const ClipboardPreview(
    type: 'Text',
    summary: '4 lines · HTML also available',
    content: _PreviewContent(
      child: Text(
        'Meeting notes — Nov 9\n'
        '• Launcher v2 review on Thu\n'
        '• AI quota pill: dual ring approved\n'
        '• Ask about clipboard retention',
        style: TextStyle(fontSize: 13, height: 20 / 13),
      ),
    ),
    metadata: {
      'Source': 'Obsidian',
      'Copied': '13:05 · 1 h ago',
      'MIME': 'text/plain, text/html',
    },
    hints: [
      LauncherKeyHint(['↵'], label: 'Paste (keeps formatting)'),
      LauncherKeyHint(['Ctrl', '↵'], label: 'Paste as plain text'),
      LauncherKeyHint(['Ctrl', 'E'], label: 'Edit before pasting'),
    ],
  ),
  'code' => const ClipboardPreview(
    type: 'Code',
    summary: 'Python · 14 lines',
    content: _CodeExample(),
    metadata: {'Source': 'VS Code', 'Copied': '12:10 · 2 h ago'},
    hints: [
      LauncherKeyHint(['↵'], label: 'Paste'),
      LauncherKeyHint(['Ctrl', '↵'], label: 'Open in editor'),
      LauncherKeyHint(['Ctrl', 'E'], label: 'Edit before pasting'),
    ],
  ),
  'image' => const ClipboardPreview(
    type: 'Image',
    summary: '1920 × 1080 · PNG',
    content: ClipboardImageThumbnail(),
    metadata: {
      'Source': 'Remy\'a Screen Capture',
      'Size': '1.2 MB',
      'Copied': '14:27 · 5 min ago',
    },
    hints: [
      LauncherKeyHint(['↵'], label: 'Paste image'),
      LauncherKeyHint(['Ctrl', '↵'], label: 'Save as…'),
      LauncherKeyHint(['Ctrl', 'E'], label: 'Open in editor'),
    ],
  ),
  'link' => const ClipboardPreview(
    type: 'Link',
    summary: 'github.com',
    content: _LinkExample(),
    metadata: {'Source': 'Firefox', 'Copied': '14:20 · 12 min ago'},
    hints: [
      LauncherKeyHint(['↵'], label: 'Paste URL'),
      LauncherKeyHint(['Ctrl', '↵'], label: 'Open in browser'),
      LauncherKeyHint(['Ctrl', 'E'], label: 'Paste as Markdown link'),
    ],
  ),
  'color' => const ClipboardPreview(
    type: 'Color',
    summary: 'HEX',
    content: _ColorExample(),
    metadata: {'Source': 'Figma', 'Copied': '13:52 · 40 min ago'},
    hints: [
      LauncherKeyHint(['↵'], label: 'Paste as copied'),
      LauncherKeyHint(['Alt', '1–4'], label: 'Paste in that format'),
    ],
  ),
  'files' => const ClipboardPreview(
    type: 'Files',
    summary: '3 items · 48 KB',
    content: _FilesExample(),
    metadata: {'Source': 'Nautilus', 'Copied': '13:30 · 1 h ago'},
    hints: [
      LauncherKeyHint(['↵'], label: 'Paste files'),
      LauncherKeyHint(['Ctrl', '↵'], label: 'Paste paths as text'),
      LauncherKeyHint(['Ctrl', 'O'], label: 'Reveal in file manager'),
    ],
  ),
  'hidden' => const ClipboardPreview(
    type: 'Hidden',
    summary: 'password manager hint',
    content: _HiddenExample(),
    metadata: {
      'Source': 'KeePassXC',
      'Clears in': '0:42',
      'Stored on disk': 'No',
    },
    hints: [
      LauncherKeyHint(['↵'], label: 'Paste'),
      LauncherKeyHint(['⇧', 'Del'], label: 'Clear now'),
    ],
  ),
  'command' => const ClipboardPreview(
    type: 'Command',
    summary: '1 line · 23 chars',
    content: _PreviewContent(
      child: Text(
        'pnpm add -D @types/node',
        style: TextStyle(
          fontFamily: 'JetBrains Mono',
          fontFamilyFallback: ['JetBrainsMono Nerd Font Mono', 'monospace'],
          fontSize: 13,
          height: 20 / 13,
        ),
      ),
    ),
    metadata: {
      'Source': 'kitty',
      'Copied': '14:32 · just now',
      'MIME': 'text/plain;charset=utf-8',
      'Times copied': '3',
    },
    hints: [
      LauncherKeyHint(['↵'], label: 'Paste into previous window'),
      LauncherKeyHint(['Ctrl', '↵'], label: 'Run in terminal'),
      LauncherKeyHint(['Ctrl', 'C'], label: 'Copy only'),
      LauncherKeyHint(['Ctrl', 'E'], label: 'Edit before pasting'),
    ],
  ),
  _ => throw ArgumentError.value(type, 'type', 'Unknown clipboard preview'),
};

const _mono = TextStyle(
  fontFamily: 'JetBrains Mono',
  fontFamilyFallback: ['JetBrainsMono Nerd Font Mono', 'monospace'],
  fontSize: 12,
  height: 18 / 12,
);

class _PreviewContent extends StatelessWidget {
  const _PreviewContent({required this.child, this.padding = const .all(12)});

  final Widget child;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) => Container(
    padding: padding,
    decoration: BoxDecoration(
      color: Theme.of(context).colorScheme.surfaceContainer,
      borderRadius: .circular(12),
    ),
    clipBehavior: .antiAlias,
    child: child,
  );
}

class _CodeExample extends StatelessWidget {
  const _CodeExample();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final keyword = TextStyle(color: colors.primary);
    final function = TextStyle(color: colors.tertiary);
    final lines = [
      TextSpan(
        children: [
          TextSpan(text: 'def', style: keyword),
          const TextSpan(text: ' '),
          TextSpan(text: 'fuse_attention', style: function),
          const TextSpan(text: '(q, k, v, mask=None):'),
        ],
      ),
      const TextSpan(text: '    s = q @ k.transpose(-2, -1)'),
      const TextSpan(text: '    s = s * q.shape[-1] ** -0.5'),
      TextSpan(
        children: [
          const TextSpan(text: '    '),
          TextSpan(text: 'if', style: keyword),
          const TextSpan(text: ' mask is not None:'),
        ],
      ),
      const TextSpan(text: '        s = s.masked_fill(mask, -inf)'),
      TextSpan(
        children: [
          const TextSpan(text: '    '),
          TextSpan(text: 'return', style: keyword),
          const TextSpan(text: ' softmax(s, -1) @ v'),
        ],
      ),
    ];
    return _PreviewContent(
      child: Column(
        crossAxisAlignment: .start,
        spacing: 6,
        children: [
          for (final (index, line) in lines.indexed)
            Row(
              spacing: 10,
              children: [
                Text(
                  ' ${index + 1}',
                  style: _mono.copyWith(color: colors.onSurfaceVariant),
                ),
                Expanded(
                  child: Text.rich(
                    line,
                    style: _mono,
                    maxLines: 1,
                    softWrap: false,
                    overflow: .clip,
                  ),
                ),
              ],
            ),
          Text(
            '+ 8 more lines',
            style: theme.textTheme.bodySmall?.copyWith(
              fontSize: 11,
              height: 14 / 11,
            ),
          ),
        ],
      ),
    );
  }
}

class _LinkExample extends StatelessWidget {
  const _LinkExample();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return _PreviewContent(
      child: Column(
        crossAxisAlignment: .start,
        spacing: 6,
        children: [
          const Text(
            'steipete/CodexBar — GitHub',
            style: TextStyle(fontSize: 14, height: 20 / 14, fontWeight: .w600),
          ),
          Text(
            'https://github.com/steipete/CodexBar',
            style: _mono.copyWith(color: theme.colorScheme.primary),
          ),
          Text(
            'In history · visited 6× · last today 14:20',
            style: theme.textTheme.bodySmall?.copyWith(
              fontSize: 11,
              height: 14 / 11,
            ),
          ),
        ],
      ),
    );
  }
}

class _ColorExample extends StatelessWidget {
  const _ColorExample();

  static const _formats = [
    ('HEX', '#6750A4'),
    ('RGB', 'rgb(103 80 164)'),
    ('HSL', 'hsl(256 34% 48%)'),
    ('OKLCH', 'oklch(0.49 0.13 297)'),
  ];

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: .stretch,
      spacing: 12,
      children: [
        Container(
          height: 72,
          decoration: BoxDecoration(
            color: const Color(0xff6750a4),
            borderRadius: .circular(12),
          ),
        ),
        _PreviewContent(
          child: Column(
            spacing: 6,
            children: [
              for (final (index, (name, value)) in _formats.indexed)
                Row(
                  spacing: 8,
                  children: [
                    SizedBox(
                      width: 48,
                      child: Text(
                        name,
                        style: theme.textTheme.bodySmall?.copyWith(
                          fontSize: 11,
                          fontWeight: .w600,
                        ),
                      ),
                    ),
                    Expanded(child: Text(value, style: _mono)),
                    LauncherKeycap('${index + 1}'),
                  ],
                ),
            ],
          ),
        ),
      ],
    );
  }
}

class _FilesExample extends StatelessWidget {
  const _FilesExample();

  @override
  Widget build(BuildContext context) => const _PreviewContent(
    child: Column(
      spacing: 6,
      children: [
        _FileLine('launcher-spec.md', '~/proj/remya-bar/docs'),
        _FileLine('keyboard-map.md', '~/proj/remya-bar/docs'),
        Opacity(
          opacity: .5,
          child: _FileLine(
            'dock-preview.png',
            '~/Pictures/Screenshots · no longer exists',
            icon: .image,
          ),
        ),
      ],
    ),
  );
}

class _FileLine extends StatelessWidget {
  const _FileLine(this.name, this.path, {this.icon = .description});

  final String name;
  final String path;
  final LauncherIconName icon;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      spacing: 8,
      children: [
        FileIconTile(
          basename: name,
          isDirectory: icon == .folder,
          size: 28,
          iconSize: 18,
        ),
        Expanded(
          child: Column(
            crossAxisAlignment: .start,
            children: [
              Text(
                name,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurface,
                  fontWeight: .w500,
                ),
              ),
              Text(
                path,
                style: theme.textTheme.bodySmall?.copyWith(
                  fontSize: 11,
                  height: 14 / 11,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _HiddenExample extends StatelessWidget {
  const _HiddenExample();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return _PreviewContent(
      padding: const .symmetric(horizontal: 12, vertical: 20),
      child: Column(
        spacing: 6,
        children: [
          const LauncherIcon(.lock, size: 28),
          const Text(
            '••••••••••••',
            style: TextStyle(fontSize: 16, height: 22 / 16),
          ),
          Text(
            'Hold Ctrl + Space to reveal',
            style: theme.textTheme.bodySmall?.copyWith(
              fontSize: 11,
              height: 14 / 11,
            ),
          ),
          LinearProgressIndicator(
            value: 150 / 264,
            minHeight: 4,
            borderRadius: .circular(2),
            color: theme.colorScheme.primary,
            backgroundColor: theme.colorScheme.secondaryContainer,
            trackGap: 4,
            stopIndicatorRadius: 0,
            semanticsLabel: 'Time remaining',
          ),
        ],
      ),
    );
  }
}
