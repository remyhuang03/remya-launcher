import 'dart:async';

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../apps/apps_client.dart';

/// Lists open Hyprland windows for the Windows scope and focuses or moves them.
class WindowsController extends ChangeNotifier {
  WindowsController({required this.client, required this.onClose});

  static const _digits = [
    LogicalKeyboardKey.digit1,
    LogicalKeyboardKey.digit2,
    LogicalKeyboardKey.digit3,
    LogicalKeyboardKey.digit4,
    LogicalKeyboardKey.digit5,
    LogicalKeyboardKey.digit6,
    LogicalKeyboardKey.digit7,
    LogicalKeyboardKey.digit8,
    LogicalKeyboardKey.digit9,
  ];

  static const rowExtent = 58.0;

  final AppsClient? client;
  final Future<void> Function() onClose;
  final scroll = ScrollController();

  List<LauncherWindow> windows = const [];
  String query = '';
  String? error, actionError, selectedId;
  bool active = false, ready = false, busy = false, showIndices = false;
  Timer? _debounce, _poll;
  int _revision = 0;
  bool _disposed = false;

  LauncherWindow? get selected =>
      windows.where((window) => window.id == selectedId).firstOrNull;

  void update({required bool active, String query = ''}) {
    if (client == null) return;
    if (active == this.active && query == this.query) return;
    _debounce?.cancel();
    _poll?.cancel();
    _revision++;
    if (active != this.active) windows = const [];
    this.active = active;
    this.query = query;
    ready = false;
    error = actionError = null;
    if (active) {
      _debounce = Timer(const Duration(milliseconds: 30), _refresh);
      // Windows open, close and retitle while the launcher is visible.
      _poll = Timer.periodic(const Duration(seconds: 2), (_) {
        if (!busy && _debounce?.isActive != true) unawaited(_refresh());
      });
    }
    notifyListeners();
  }

  Future<void> retry() async {
    error = null;
    notifyListeners();
    await _refresh();
  }

  Future<void> _refresh() async {
    final revision = _revision;
    try {
      final next = await client!.windows(query.trim());
      if (_disposed || revision != _revision) return;
      final queryChanged = !ready;
      windows = next;
      ready = true;
      error = null;
      if (queryChanged) {
        selectedId = next.firstOrNull?.id;
        if (scroll.hasClients) scroll.jumpTo(0);
      } else if (selected == null) {
        selectedId = next.firstOrNull?.id;
      }
    } on Object catch (failure) {
      if (_disposed || revision != _revision) return;
      error = failure is AppsException ? failure.message : 'Windows failed.';
    }
    notifyListeners();
  }

  void select(LauncherWindow window) {
    if (selectedId == window.id) return;
    selectedId = window.id;
    actionError = null;
    notifyListeners();
    if (!scroll.hasClients) return;
    final top = windows.indexOf(window) * rowExtent;
    final position = scroll.position;
    if (top < position.pixels) {
      scroll.jumpTo(top);
    } else if (top + rowExtent > position.pixels + position.viewportDimension) {
      scroll.jumpTo(top + rowExtent - position.viewportDimension);
    }
  }

  /// Focuses [window] on its workspace, or moves it to the current one first.
  Future<void> run(LauncherWindow window, {bool here = false}) async {
    if (busy) return;
    busy = true;
    actionError = null;
    selectedId = window.id;
    notifyListeners();
    try {
      await client!.perform(
        here ? 'windowHere' : 'window',
        appId: window.appId,
        query: query.trim(),
        target: window.id,
      );
      if (!_disposed) await onClose();
    } on Object catch (failure) {
      if (_disposed) return;
      actionError = failure is AppsException
          ? failure.message
          : 'Could not reach the window.';
      unawaited(_refresh());
    } finally {
      busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  KeyEventResult handleKey(KeyEvent event) {
    if (!active) return .ignored;
    final keys = HardwareKeyboard.instance;
    final key = event.logicalKey;
    final ctrl = keys.isControlPressed;
    final alt = keys.isAltPressed;
    if (showIndices != alt) {
      showIndices = alt;
      notifyListeners();
    }
    if (event is KeyUpEvent || keys.isShiftPressed) return .ignored;
    final index = windows.indexWhere((window) => window.id == selectedId);
    if (!ctrl &&
        !alt &&
        (key == LogicalKeyboardKey.arrowUp ||
            key == LogicalKeyboardKey.arrowDown)) {
      if (windows.isNotEmpty) {
        final delta = key == LogicalKeyboardKey.arrowUp ? -1 : 1;
        select(windows[(index + delta).clamp(0, windows.length - 1)]);
      }
      return .handled;
    }
    if (event is! KeyDownEvent) return .ignored;
    if (alt && !ctrl) {
      final quick = _digits.indexOf(key);
      if (quick < 0 || quick >= windows.length) return .ignored;
      unawaited(run(windows[quick]));
      return .handled;
    }
    if (!alt &&
        (key == LogicalKeyboardKey.enter ||
            key == LogicalKeyboardKey.numpadEnter)) {
      if (selected case final window?) unawaited(run(window, here: ctrl));
      return .handled;
    }
    return .ignored;
  }

  @override
  void dispose() {
    _disposed = true;
    _debounce?.cancel();
    _poll?.cancel();
    scroll.dispose();
    super.dispose();
  }
}
