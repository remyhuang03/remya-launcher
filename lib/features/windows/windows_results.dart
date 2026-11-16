import 'dart:async';
import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../apps/apps_icon.dart';
import '../launcher/launcher_controls.dart';
import '../launcher/launcher_panel.dart';
import '../launcher/launcher_results.dart';
import 'windows_controller.dart';

class WindowsResults extends StatelessWidget {
  const WindowsResults(this.controller, {super.key});

  final WindowsController controller;

  @override
  Widget build(BuildContext context) {
    final c = controller;
    if (c.error case final error?) {
      return Padding(
        padding: const .all(12),
        child: Row(
          spacing: 8,
          children: [
            Expanded(child: Text(error)),
            TextButton(onPressed: c.retry, child: const Text('Retry')),
          ],
        ),
      );
    }
    if (!c.ready && c.windows.isEmpty) {
      return const LinearProgressIndicator(semanticsLabel: 'Loading windows');
    }
    if (c.windows.isEmpty) {
      return Padding(
        padding: const .all(12),
        child: Text(
          c.query.trim().isEmpty ? 'No open windows.' : 'No matching windows.',
        ),
      );
    }
    final style = Theme.of(context).textTheme.bodySmall;
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: [
        LauncherSectionHeader(
          'OPEN WINDOWS',
          height: 28,
          trailing: Text('Hyprland · recent first', style: style),
        ),
        Flexible(
          child: SizedBox(
            height: math.min(c.windows.length, 8) * WindowsController.rowExtent,
            child: ListView.builder(
              controller: c.scroll,
              padding: .zero,
              itemExtent: WindowsController.rowExtent,
              itemCount: c.windows.length,
              itemBuilder: (context, index) => _WindowRow(c, index),
            ),
          ),
        ),
      ],
    );
  }
}

class _WindowRow extends StatelessWidget {
  const _WindowRow(this.controller, this.index);

  final WindowsController controller;
  final int index;

  @override
  Widget build(BuildContext context) {
    final c = controller;
    final window = c.windows[index];
    final colors = Theme.of(context).colorScheme;
    final selected = window.id == c.selectedId;
    final name = window.appName ?? window.windowClass;
    final error = selected ? c.actionError : null;
    return Padding(
      padding: const .only(bottom: 2),
      child: LauncherResultRow(
        title: launcherHighlight(
          window.title.isEmpty ? name : window.title,
          c.query.trim(),
          colors.primary,
        ),
        subtitle: [
          name,
          if (window.appName != null && window.windowClass.isNotEmpty)
            window.windowClass,
        ].join(' · '),
        subtitleSpan: error == null
            ? null
            : TextSpan(
                text: error,
                style: TextStyle(color: colors.error),
              ),
        leading: Align(
          alignment: .centerLeft,
          child: AppsIcon(
            id: window.appId ?? window.windowClass,
            name: name,
            icon: window.icon,
          ),
        ),
        trailing: Row(
          mainAxisSize: .min,
          spacing: 8,
          children: [
            if (c.showIndices && index < 9) LauncherKeycap('${index + 1}'),
            Text(
              'Workspace ${window.workspace}',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            if (selected) const LauncherKeycap('↵'),
          ],
        ),
        selected: selected,
        onPressed: c.busy
            ? null
            : () {
                c.select(window);
                unawaited(c.run(window));
              },
      ),
    );
  }
}

LauncherFooter windowsFooter(WindowsController c, {required bool singleLine}) =>
    LauncherFooter(
      singleLine: singleLine,
      leading: [
        if (c.selected != null) ...const [
          LauncherKeyHint(['↵'], label: 'Focus'),
          LauncherKeyHint(['Ctrl', '↵'], label: 'Move here'),
        ],
        const LauncherKeyHint(['Esc'], label: 'Back'),
      ],
      trailing: [
        if (c.windows.length > 1) ...const [
          LauncherKeyHint(['↑', '↓'], label: 'Navigate'),
          LauncherKeyHint(['Alt', '1–9'], label: 'Focus row'),
        ],
      ],
    );
