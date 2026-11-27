part of 'files_settings.dart';

bool _listEquals(List<String> a, List<String> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

String _formatBytesLabel(int bytes) {
  const kib = 1024;
  const mib = 1024 * 1024;
  if (bytes >= mib) {
    final formatted = bytes % mib == 0
        ? '${bytes ~/ mib}'
        : (bytes / mib).toStringAsFixed(1);
    return '$formatted MiB ($bytes B)';
  }
  if (bytes >= kib) {
    final formatted = bytes % kib == 0
        ? '${bytes ~/ kib}'
        : (bytes / kib).toStringAsFixed(1);
    return '$formatted KiB ($bytes B)';
  }
  return '$bytes B';
}

enum _ListField {
  roots(
    'roots',
    'Search roots',
    'Folders searched by Launcher, one absolute path (/...) or ~/... path '
        'per line.',
    'Folders (one per line)',
    'Enter one absolute path (/...) or ~/... path per line.',
    minItems: 1,
    maxItems: FilesPreferences.maxRoots,
  ),
  exclusions(
    'exclusions',
    'Excluded paths',
    'Literal local paths skipped during search, one absolute path (/...) or '
        '~/... path per line (no glob or relative syntax).',
    'Excluded paths (one per line)',
    'Enter one literal absolute path (/...) or ~/... path per line (for '
        'example, ~/Projects/build). Leave empty for none.',
    minItems: 0,
    maxItems: FilesPreferences.maxExclusions,
  );

  const _ListField(
    this.key,
    this.title,
    this.description,
    this.inputLabel,
    this.helperText, {
    required this.minItems,
    required this.maxItems,
  });

  final String key;
  final String title;
  final String description;
  final String inputLabel;
  final String helperText;
  final int minItems;
  final int maxItems;

  String summary(List<String> items) => switch (this) {
    roots => items.length == 1 ? items.single : '${items.length} folders',
    exclusions =>
      items.isEmpty
          ? 'None'
          : items.length == 1
          ? items.single
          : '${items.length} paths',
  };

  String? validateItems(List<String> items) {
    if (items.length < minItems) {
      return 'Enter at least one root path.';
    }
    if (items.length > maxItems) {
      return 'Enter at most $maxItems entries.';
    }
    if (items.toSet().length != items.length) {
      return 'Remove duplicate entries.';
    }
    for (final item in items) {
      final ok = switch (this) {
        roots => FilesPreferences.isValidRootPath(item),
        exclusions => FilesPreferences.isValidExclusion(item),
      };
      if (!ok) {
        return switch (this) {
          roots =>
            'Each root must be an absolute path (/...) or ~/... path '
                'without control characters.',
          exclusions =>
            'Each excluded path must be a literal absolute path (/...) or '
                '~/... path of 1 to ${FilesPreferences.maxExclusionLength} '
                'characters without control characters.',
        };
      }
    }
    return null;
  }
}

enum _NumberField {
  contentsMaxBytes(
    'contents_max_bytes',
    'Content search size limit',
    'Skip searching file contents for files larger than this byte limit.',
    'Size in bytes',
    FilesPreferences.minBytesLimit,
    FilesPreferences.maxContentsBytes,
    isBytes: true,
  ),
  resultLimit(
    'result_limit',
    'Result limit',
    'Maximum number of file results returned per search query.',
    'Maximum results',
    FilesPreferences.minResultLimit,
    FilesPreferences.maxResultLimit,
    unit: 'results',
  ),
  retentionDays(
    'retention_days',
    'Retention period',
    'How many days local file and folder history entries are kept.',
    'Days',
    1,
    FilesPreferences.maxRetentionDays,
    unit: 'days',
  ),
  maxRecordsPerKind(
    'max_records_per_kind',
    'Maximum records per kind',
    'Maximum number of file entries and directory entries stored separately.',
    'Maximum records',
    1,
    FilesPreferences.maxHistoryRecords,
    unit: 'records',
  ),
  textMaxBytes(
    'text_max_bytes',
    'Text preview size limit',
    'Maximum file size in bytes loaded into the text preview pane.',
    'Size in bytes',
    FilesPreferences.minBytesLimit,
    FilesPreferences.maxTextPreviewBytes,
    isBytes: true,
  ),
  imageMaxBytes(
    'image_max_bytes',
    'Image preview size limit',
    'Maximum file size in bytes decoded for the image preview pane.',
    'Size in bytes',
    FilesPreferences.minBytesLimit,
    FilesPreferences.maxImagePreviewBytes,
    isBytes: true,
  );

  const _NumberField(
    this.key,
    this.title,
    this.description,
    this.inputLabel,
    this.min,
    this.max, {
    this.unit = '',
    this.isBytes = false,
  });

  final String key;
  final String title;
  final String description;
  final String inputLabel;
  final int min;
  final int max;
  final String unit;
  final bool isBytes;

  String format(int value) =>
      isBytes ? _formatBytesLabel(value) : '$value $unit';

  String helperText(int current) => isBytes
      ? 'Enter $min to $max bytes (current: ${_formatBytesLabel(current)}).'
      : 'Enter $min to $max $unit.';
}

enum _TextField {
  editor(
    'editor',
    'Editor',
    'Executable name or absolute pathname (no arguments or shell command). '
        'Leave empty to use VISUAL/EDITOR, then the default text app.',
    'Editor executable or path',
    'Enter an executable name (for example, nvim) or an absolute pathname '
        '(for example, /usr/bin/hx). Leave empty to use VISUAL/EDITOR, then '
        'the default text app.',
    'Enter an executable name (without spaces) or an absolute path (/...) '
        'without shell operators or control characters.',
  ),
  terminal(
    'terminal',
    'Terminal',
    'Executable name or absolute pathname (no arguments or shell command). '
        'Leave empty for automatic selection.',
    'Terminal executable or path',
    'Enter an executable name (for example, kitty) or an absolute pathname '
        '(for example, /usr/bin/foot). Leave empty for automatic.',
    'Enter an executable name (without spaces) or an absolute path (/...) '
        'without shell operators or control characters.',
  ),
  fileManager(
    'file_manager',
    'File manager',
    'Executable name or absolute pathname (no arguments or shell command). '
        'Leave empty for automatic selection.',
    'File manager executable or path',
    'Enter an executable name (for example, nautilus) or an absolute pathname '
        '(for example, /usr/bin/dolphin). Leave empty for automatic.',
    'Enter an executable name (without spaces) or an absolute path (/...) '
        'without shell operators or control characters.',
  );

  const _TextField(
    this.key,
    this.title,
    this.description,
    this.inputLabel,
    this.helperText,
    this.invalidMessage,
  );

  final String key;
  final String title;
  final String description;
  final String inputLabel;
  final String helperText;
  final String invalidMessage;
}

/// A tonal picker trigger that keeps long values from filling the row.
class _ValueButton extends StatelessWidget {
  const _ValueButton({required this.label, required this.onPressed});

  final String label;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) => ConstrainedBox(
    constraints: const BoxConstraints(maxWidth: 260),
    child: SettingsButton(label: label, onPressed: onPressed),
  );
}

class _LineListDialog extends StatefulWidget {
  const _LineListDialog({required this.spec, required this.current});

  final _ListField spec;
  final List<String> current;

  @override
  State<_LineListDialog> createState() => _LineListDialogState();
}

class _LineListDialogState extends State<_LineListDialog> {
  final _form = GlobalKey<FormState>();
  late String _text = widget.current.join('\n');

  List<String> _parse(String raw) => raw
      .split('\n')
      .map((line) => line.trim())
      .where((line) => line.isNotEmpty)
      .toList();

  void _submit() {
    if (_form.currentState!.validate()) {
      Navigator.pop(context, _parse(_text));
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.spec.title),
    content: SizedBox(
      width: 440,
      child: Form(
        key: _form,
        child: TextFormField(
          autofocus: true,
          initialValue: _text,
          minLines: 4,
          maxLines: 10,
          keyboardType: .multiline,
          decoration: InputDecoration(
            labelText: widget.spec.inputLabel,
            helperText: widget.spec.helperText,
            helperMaxLines: 3,
          ),
          onChanged: (value) => _text = value,
          validator: (text) => widget.spec.validateItems(_parse(text ?? '')),
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(onPressed: _submit, child: const Text('Save')),
    ],
  );
}

class _NumberDialog extends StatefulWidget {
  const _NumberDialog({required this.spec, required this.current});

  final _NumberField spec;
  final int current;

  @override
  State<_NumberDialog> createState() => _NumberDialogState();
}

class _NumberDialogState extends State<_NumberDialog> {
  final _form = GlobalKey<FormState>();
  late int? _value = widget.current;

  void _submit() {
    if (_form.currentState!.validate()) Navigator.pop(context, _value);
  }

  @override
  Widget build(BuildContext context) {
    final spec = widget.spec;
    return AlertDialog(
      title: Text(spec.title),
      content: Form(
        key: _form,
        child: TextFormField(
          autofocus: true,
          initialValue: '${widget.current}',
          keyboardType: .number,
          decoration: InputDecoration(
            labelText: spec.inputLabel,
            helperText: spec.helperText(widget.current),
            helperMaxLines: 2,
          ),
          onChanged: (text) => _value = int.tryParse(text.trim()),
          onFieldSubmitted: (_) => _submit(),
          validator: (text) {
            final value = int.tryParse(text?.trim() ?? '');
            return value == null || value < spec.min || value > spec.max
                ? 'Enter a whole number from ${spec.min} to ${spec.max}.'
                : null;
          },
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Save')),
      ],
    );
  }
}

class _TextDialog extends StatefulWidget {
  const _TextDialog({required this.spec, required this.current});

  final _TextField spec;
  final String current;

  @override
  State<_TextDialog> createState() => _TextDialogState();
}

class _TextDialogState extends State<_TextDialog> {
  final _form = GlobalKey<FormState>();
  late String _text = widget.current;

  void _submit() {
    if (_form.currentState!.validate()) {
      Navigator.pop(context, _text.trim());
    }
  }

  @override
  Widget build(BuildContext context) {
    final spec = widget.spec;
    return AlertDialog(
      title: Text(spec.title),
      content: SizedBox(
        width: 440,
        child: Form(
          key: _form,
          child: TextFormField(
            autofocus: true,
            initialValue: _text,
            keyboardType: .text,
            decoration: InputDecoration(
              labelText: spec.inputLabel,
              helperText: spec.helperText,
              helperMaxLines: 3,
            ),
            onChanged: (value) => _text = value,
            onFieldSubmitted: (_) => _submit(),
            validator: (text) =>
                FilesPreferences.isValidActionTarget((text ?? '').trim())
                ? null
                : spec.invalidMessage,
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Save')),
      ],
    );
  }
}
