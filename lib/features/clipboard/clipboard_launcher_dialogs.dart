part of 'clipboard_launcher.dart';

extension _LauncherDialogs on _ClipboardLauncherState {
  Future<void> _showAppInfo(String text) => showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('Package info'),
      content: SingleChildScrollView(child: SelectableText(text)),
      actions: [
        TextButton(
          autofocus: true,
          onPressed: () => Navigator.pop(context),
          child: const Text('Close'),
        ),
      ],
    ),
  );

  Future<String?> _askWorkspace() async {
    final value = await showDialog<String>(
      context: context,
      builder: (context) => const _WorkspaceDialog(),
    );
    if (mounted) _searchFocus.requestFocus();
    return value;
  }

  Future<void> _clear() => _perform(() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Clear clipboard history?'),
        content: const Text(
          'This removes all Remy\'a Launcher history, including pinned entries.',
        ),
        actions: [
          TextButton(
            autofocus: true,
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Clear history'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    await widget.client.clear();
    await _refresh();
    if (mounted) _rebuild(() => _status = 'Clipboard history cleared.');
  });

  Future<String?> _editText(String initial) async {
    if (!mounted) return null;
    return showDialog<String>(
      context: context,
      builder: (context) => _EditDialog(initial),
    );
  }
}

class _WorkspaceDialog extends StatefulWidget {
  const _WorkspaceDialog();

  @override
  State<_WorkspaceDialog> createState() => _WorkspaceDialogState();
}

class _WorkspaceDialogState extends State<_WorkspaceDialog> {
  final _form = GlobalKey<FormState>();
  String _value = '';

  void _submit() {
    if (_form.currentState!.validate()) Navigator.pop(context, _value.trim());
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Open on workspace'),
    content: Form(
      key: _form,
      child: TextFormField(
        autofocus: true,
        keyboardType: .number,
        decoration: const InputDecoration(labelText: 'Workspace number'),
        onChanged: (value) => _value = value,
        onFieldSubmitted: (_) => _submit(),
        validator: (value) => (int.tryParse(value?.trim() ?? '') ?? 0) > 0
            ? null
            : 'Enter a positive workspace number.',
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(onPressed: _submit, child: const Text('Open')),
    ],
  );
}

class _EditDialog extends StatefulWidget {
  const _EditDialog(this.initial);

  final String initial;

  @override
  State<_EditDialog> createState() => _EditDialogState();
}

class _EditDialogState extends State<_EditDialog> {
  late final _editor = TextEditingController(text: widget.initial);

  @override
  void dispose() {
    _editor.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Edit before pasting'),
    content: SizedBox(
      width: 480,
      child: TextField(
        autofocus: true,
        controller: _editor,
        minLines: 3,
        maxLines: 10,
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () => Navigator.pop(context, _editor.text),
        child: const Text('Paste'),
      ),
    ],
  );
}
