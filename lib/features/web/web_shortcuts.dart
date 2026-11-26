import 'dart:async';

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import 'web_client.dart';
import 'web_results.dart';
import 'web_search_controller.dart';

class WebShortcuts {
  const WebShortcuts({
    required this.web,
    required this.search,
    required this.changeSearch,
    required this.showEngines,
  });

  final WebSearchController web;
  final TextEditingController search;
  final void Function(String query, WebEngine? engine) changeSearch;
  final VoidCallback showEngines;
  static const _digits = [
    LogicalKeyboardKey.digit1,
    LogicalKeyboardKey.digit2,
    LogicalKeyboardKey.digit3,
    LogicalKeyboardKey.digit4,
    LogicalKeyboardKey.digit5,
    LogicalKeyboardKey.digit6,
  ];

  bool get _atEnd =>
      search.selection.isCollapsed &&
      search.selection.extentOffset == search.text.length &&
      search.value.composing.isCollapsed;

  KeyEventResult handle(KeyEvent event, {required bool compact}) {
    if (event is KeyUpEvent || !search.value.composing.isCollapsed) {
      return .ignored;
    }
    final key = event.logicalKey;
    final keys = HardwareKeyboard.instance;
    final ctrl = keys.isControlPressed;
    final alt = keys.isAltPressed;
    final shift = keys.isShiftPressed;
    final choice = web.selected;
    if (!ctrl &&
        !alt &&
        !shift &&
        (key == LogicalKeyboardKey.arrowDown ||
            key == LogicalKeyboardKey.arrowUp)) {
      web.move(key == LogicalKeyboardKey.arrowDown ? 1 : -1);
      return .handled;
    }
    if (event is! KeyDownEvent) return .ignored;

    if (!compact &&
        key == LogicalKeyboardKey.backspace &&
        !ctrl &&
        !alt &&
        search.text.isEmpty &&
        web.engine != null) {
      changeSearch('', null);
      return .handled;
    }
    if (!compact &&
        !ctrl &&
        !alt &&
        search.text.isEmpty &&
        web.engine == null &&
        (event.character == '?' || key == LogicalKeyboardKey.f1)) {
      showEngines();
      return .handled;
    }
    if (!ctrl &&
        !alt &&
        !shift &&
        key == LogicalKeyboardKey.arrowRight &&
        _atEnd) {
      if (web.completionSuffix(search.text) case final suffix?) {
        changeSearch('${search.text}$suffix', web.engine);
        return .handled;
      }
    }
    if (!ctrl && !alt && key == LogicalKeyboardKey.tab && _atEnd) {
      final engine = web.engine;
      if (!compact && engine != null && web.engines.length > 1) {
        final index = web.engines.indexWhere(
          (item) => item.engineId == engine.engineId,
        );
        changeSearch(
          search.text,
          web.engines[(index + (shift ? -1 : 1)) % web.engines.length],
        );
        return .handled;
      }
      if (!shift &&
          choice?.isDirect == true &&
          web.completionSuffix(search.text) == null) {
        searchSite(choice!);
        return .handled;
      }
    }
    if (!compact && alt && !ctrl && !shift) {
      final index = _digits.indexOf(key);
      final layout = web.layout;
      final quick = web.engine == null ? layout.frequent : layout.otherEngines;
      if (index >= 0 && index < quick.length) {
        unawaited(web.open(quick[index]));
        return .handled;
      }
    }
    if (alt) return .ignored;
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      final target = ctrl && !shift ? web.searchInstead : choice;
      if (target == null || (shift && !ctrl)) return .ignored;
      unawaited(
        ctrl && shift ? web.run(target, 'open.private') : web.open(target),
      );
      return .handled;
    }
    if (choice == null || choice.isSearch) return .ignored;
    if (ctrl && !shift && key == LogicalKeyboardKey.keyK) {
      unawaited(web.openMenu(choice));
      return .handled;
    }
    final action = switch (key) {
      LogicalKeyboardKey.keyC
          when ctrl && !shift && search.selection.isCollapsed =>
        'copy.url',
      LogicalKeyboardKey.keyM when ctrl && !shift => 'copy.markdown',
      LogicalKeyboardKey.keyD when ctrl && !shift && choice.entryId != null =>
        'pin.local',
      LogicalKeyboardKey.delete when shift && choice.entryId != null =>
        ctrl ? 'forget.localSite' : 'forget.localPage',
      _ => null,
    };
    if (action != null) {
      unawaited(web.run(choice, action));
      return .handled;
    }
    return .ignored;
  }

  KeyEventResult handleMenu(FocusNode node, KeyEvent event) {
    if (event is KeyUpEvent) return .ignored;
    final key = event.logicalKey;
    final keys = HardwareKeyboard.instance;
    final ctrl = keys.isControlPressed;
    final shift = keys.isShiftPressed;
    if (key == LogicalKeyboardKey.escape) {
      web.closeMenu();
      return .handled;
    }
    if (key == LogicalKeyboardKey.arrowDown ||
        key == LogicalKeyboardKey.arrowUp) {
      web.moveMenu(key == LogicalKeyboardKey.arrowDown ? 1 : -1);
      return .handled;
    }
    if (event is! KeyDownEvent || keys.isAltPressed) return .ignored;
    final actions = web.menuActions;
    final action = switch (key) {
      LogicalKeyboardKey.enter || LogicalKeyboardKey.numpadEnter =>
        ctrl && shift
            ? 'open.private'
            : !ctrl && !shift
            ? actions.elementAtOrNull(web.menuIndex)?.action
            : null,
      LogicalKeyboardKey.keyC
          when ctrl && !shift && web.menuFilter.selection.isCollapsed =>
        'copy.url',
      LogicalKeyboardKey.keyM when ctrl && !shift => 'copy.markdown',
      LogicalKeyboardKey.keyO when ctrl && !shift => 'open.browser',
      LogicalKeyboardKey.keyD when ctrl && !shift => 'pin.local',
      LogicalKeyboardKey.tab when !ctrl && !shift => 'search.site',
      LogicalKeyboardKey.delete when shift =>
        ctrl ? 'forget.localSite' : 'forget.localPage',
      _ => null,
    };
    if (action == null) return .ignored;
    final isBareEnter =
        (key == LogicalKeyboardKey.enter ||
            key == LogicalKeyboardKey.numpadEnter) &&
        !ctrl &&
        !shift;
    final match = isBareEnter
        ? actions.elementAtOrNull(web.menuIndex)
        : actions
              .where((item) => item.action == action && item.enabled)
              .firstOrNull;
    if (match?.enabled == true) runMenu(match!);
    return .handled;
  }

  void runMenu(WebAction action) {
    final choice = web.menuChoice;
    if (choice == null || !action.enabled) return;
    if (action.action == 'search.site') {
      searchSite(choice);
    } else {
      unawaited(web.run(choice, action.action, browserId: action.browserId));
    }
  }

  void searchSite(WebChoice choice) {
    final host = choice.hostname;
    if (host.isEmpty) return;
    final engine = web.engines
        .where(
          (item) =>
              item.hostname == host ||
              (item.hostname.isNotEmpty && host.endsWith('.${item.hostname}')),
        )
        .firstOrNull;
    changeSearch(engine == null ? 'site:$host ' : '', engine);
  }
}
