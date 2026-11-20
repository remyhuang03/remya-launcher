import 'dart:async';

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';

import '../apps/apps_client.dart';
import '../clipboard/clipboard_client.dart';
import '../clipboard/clipboard_launcher.dart';
import '../clipboard/clipboard_platform.dart';
import '../emoji/emoji_settings.dart';
import '../files/files_client.dart';
import '../files/files_preferences.dart';
import '../web/web_client.dart';
import 'startup_mode.dart';

class ResidentLauncher extends StatefulWidget {
  const ResidentLauncher({required this.clipboard, super.key});

  final ClipboardClient clipboard;

  @override
  State<ResidentLauncher> createState() => _ResidentLauncherState();
}

class _ResidentSession {
  _ResidentSession(this.platform, this.clipboard);

  final ClipboardPlatform platform;
  final bool clipboard;
  final apps = AppsClient();
  final web = WebClient();
  final files = FilesClient();
}

class _ResidentLauncherState extends State<ResidentLauncher> {
  static const _window = MethodChannel('top.remya.launcher/window');
  final _client = SettingsClient();
  StreamSubscription<bool>? _startup;
  StreamSubscription<bool>? _coreAvailability;
  _ResidentSession? _session;
  int _generation = 0;
  int _clipboardRequest = 0;

  @override
  void initState() {
    super.initState();
    _coreAvailability = _client.serviceAvailableChanges.listen((available) {
      if (available) _watchStartupMode();
    }, onError: (Object _) {});
    _watchStartupMode();
    _window.setMethodCallHandler(_handleRequest);
    unawaited(_window.invokeMethod<bool>('readyForRequests'));
  }

  void _watchStartupMode() {
    unawaited(_startup?.cancel());
    final preferences = AppPreferences(launcherPreferencesId, client: _client);
    _startup = preferences
        .watch(residentStartupKey)
        .map(readResidentStartup)
        .listen((resident) {
          if (!resident) {
            unawaited(_window.invokeMethod<void>('disableResidency'));
          }
        }, onError: (Object _) {});
  }

  Future<Object?> _handleRequest(MethodCall call) async {
    switch (call.method) {
      case 'prepareOpen':
        final generation = ++_generation;
        bool isCurrent() => mounted && generation == _generation;
        final platform = await ClipboardPlatform.capture(isCurrent: isCurrent);
        if (!isCurrent()) return false;
        setState(
          () => _session = _ResidentSession(platform, call.arguments == true),
        );
        // Build the new session while hidden so the old frame cannot reappear.
        final binding = WidgetsBinding.instance;
        binding.scheduleWarmUpFrame();
        await binding.endOfFrame;
        return isCurrent();
      case 'showClipboard':
        setState(() => _clipboardRequest++);
      case 'endSession':
        _generation++;
        Navigator.of(context).popUntil((route) => route.isFirst);
        setState(() => _session = null);
        WidgetsBinding.instance.scheduleWarmUpFrame();
    }
    return null;
  }

  @override
  void dispose() {
    _window.setMethodCallHandler(null);
    unawaited(_startup?.cancel());
    unawaited(_coreAvailability?.cancel());
    unawaited(_client.close());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final session = _session;
    return ClipboardLauncher(
      key: ObjectKey(session),
      client: widget.clipboard,
      platform: session?.platform ?? const ClipboardPlatform(),
      startInClipboard: session?.clipboard ?? false,
      listenForWindowRequests: false,
      clipboardRequest: _clipboardRequest,
      appsClient: session?.apps,
      webClient: session?.web,
      filesClient: session?.files,
      filesPreferences: FilesPreferences(client: _client),
      loadEmojiPreferences: () => EmojiPreferences.read(_client),
    );
  }
}
