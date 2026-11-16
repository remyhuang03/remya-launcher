part of 'clipboard_launcher.dart';

extension _WebLauncher on _ClipboardLauncherState {
  Widget? _webSearchHint(BuildContext context) {
    if (_web.completionSuffix(_search.text) != null) {
      return const LauncherKeyHint(['Tab'], label: 'accept');
    }
    if (_web.engine != null && _search.text.isEmpty) {
      return const LauncherKeyHint(['⌫'], label: 'remove engine');
    }
    final result = _web.result;
    if (result == null) return null;
    final browsers = result.sourceSummary
        .map((source) => source.label)
        .join(', ');
    final count = result.matchedCount;
    final label = [
      if (browsers.isNotEmpty) browsers,
      if (_search.text.isNotEmpty) '$count ${count == 1 ? 'match' : 'matches'}',
    ].join(' · ');
    return label.isEmpty
        ? null
        : ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 150),
            child: Text(
              label,
              overflow: .ellipsis,
              style: Theme.of(context).textTheme.bodySmall,
            ),
          );
  }

  Widget _webBody() => WebResults(
    layout: _web.layout,
    selectedKey: _inWeb || _webActiveInAll ? _web.selected?.key : null,
    onOpen: (choice) => unawaited(_web.open(choice)),
    onSelect: _web.select,
    favicon: _web.favicon,
    tileColumns: _tileColumns,
    onSettings: () => unawaited(_web.openSettings()),
    loading: _web.loading,
    error: _web.error,
    onRetry: _web.retry,
    scrollable: false,
  );

  Future<bool> _confirmWebForget(WebChoice choice, bool site) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(site ? 'Hide this site?' : 'Hide this page?'),
        content: Text(
          site
              ? '${choice.hostname} will be hidden in Launcher. Your browsers keep their own history and bookmarks.'
              : 'This page will be hidden in Launcher. Your browsers keep their own history and bookmarks.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Hide'),
          ),
        ],
      ),
    );
    if (mounted) _searchFocus.requestFocus();
    return confirmed == true;
  }

  Future<void> _showWebEngines() async {
    await _web.loadEngines();
    if (!mounted || !_inWeb) return;
    final engine = await showDialog<WebEngine>(
      context: context,
      builder: (context) => SimpleDialog(
        title: const Text('Web search keywords'),
        children: [
          for (final engine in _web.engines)
            SimpleDialogOption(
              onPressed: () => Navigator.pop(context, engine),
              child: Row(
                spacing: 12,
                children: [LauncherKeycap(engine.keyword), Text(engine.label)],
              ),
            ),
          if (_web.engines.isEmpty)
            const Padding(
              padding: .all(16),
              child: Text('Search engines are unavailable.'),
            ),
        ],
      ),
    );
    if (!mounted || !_inWeb) return;
    if (engine != null) _setScope(.web, initialQuery: '', webEngine: engine);
    _searchFocus.requestFocus();
  }
}
