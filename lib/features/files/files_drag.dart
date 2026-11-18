import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:material_ui/material_ui.dart';

const _dragThreshold = 10.0;

/// Starts [onDrag] once when a primary mouse press drags past 10 logical pixels.
class FilesDragSource extends StatefulWidget {
  const FilesDragSource({required this.onDrag, required this.child, super.key});

  final VoidCallback? onDrag;
  final Widget child;

  @override
  State<FilesDragSource> createState() => _FilesDragSourceState();
}

class _FilesDragSourceState extends State<FilesDragSource> {
  int? _pointer;
  Offset? _downPosition;
  bool _started = false;

  bool get _enabled => widget.onDrag != null && !kIsWeb && Platform.isLinux;

  void _reset() {
    _pointer = null;
    _downPosition = null;
    _started = false;
  }

  @override
  void didUpdateWidget(covariant FilesDragSource oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!_enabled) {
      _reset();
    }
  }

  @override
  void dispose() {
    _reset();
    super.dispose();
  }

  void _onPointerDown(PointerDownEvent event) {
    if (!_enabled ||
        event.kind != .mouse ||
        event.buttons != kPrimaryMouseButton) {
      _reset();
      return;
    }
    _pointer = event.pointer;
    _downPosition = event.position;
    _started = false;
  }

  void _onPointerMove(PointerMoveEvent event) {
    if (_pointer != event.pointer || _started) return;
    final down = _downPosition;
    if (down == null ||
        event.kind != .mouse ||
        (event.buttons & kPrimaryMouseButton) == 0) {
      _reset();
      return;
    }
    if ((event.position - down).distance > _dragThreshold) {
      _started = true;
      widget.onDrag?.call();
    }
  }

  void _onPointerEnd(PointerEvent event) {
    if (_pointer == event.pointer) {
      _reset();
    }
  }

  void _onPointerHover(PointerHoverEvent event) {
    if (_pointer != null && (event.buttons & kPrimaryMouseButton) == 0) {
      _reset();
    }
  }

  @override
  Widget build(BuildContext context) {
    if (!_enabled) return widget.child;
    return Listener(
      onPointerDown: _onPointerDown,
      onPointerMove: _onPointerMove,
      onPointerUp: _onPointerEnd,
      onPointerCancel: _onPointerEnd,
      onPointerHover: _onPointerHover,
      child: widget.child,
    );
  }
}
