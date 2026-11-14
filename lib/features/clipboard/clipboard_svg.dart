import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_svg/flutter_svg.dart';
import 'package:material_ui/material_ui.dart';

const _maxSvgFileBytes = 10 * 1024 * 1024;

bool isSvgFilePath(String path) => path.toLowerCase().endsWith('.svg');

Future<Uint8List?> loadSvgFileBytes(String path) async {
  if (!isSvgFilePath(path)) return null;
  try {
    final file = File(path);
    final stat = await file.stat();
    if (stat.type != FileSystemEntityType.file ||
        stat.size > _maxSvgFileBytes) {
      return null;
    }
    final bytes = await file.readAsBytes();
    return bytes.isEmpty ? null : bytes;
  } on FileSystemException {
    return null;
  }
}

class ClipboardSvgFile extends StatefulWidget {
  const ClipboardSvgFile({
    required this.path,
    required this.fit,
    required this.fallback,
    this.wrapper,
    this.semanticLabel,
    this.excludeFromSemantics = false,
    super.key,
  });

  final String path;
  final BoxFit fit;
  final Widget fallback;
  final Widget Function(BuildContext context, Widget child)? wrapper;
  final String? semanticLabel;
  final bool excludeFromSemantics;

  @override
  State<ClipboardSvgFile> createState() => _ClipboardSvgFileState();
}

class _ClipboardSvgFileState extends State<ClipboardSvgFile> {
  late Future<Uint8List?> _bytes;

  @override
  void initState() {
    super.initState();
    _bytes = loadSvgFileBytes(widget.path);
  }

  @override
  void didUpdateWidget(covariant ClipboardSvgFile oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.path != widget.path) {
      _bytes = loadSvgFileBytes(widget.path);
    }
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<Uint8List?>(
    future: _bytes,
    builder: (context, snapshot) {
      final bytes = snapshot.data;
      if (bytes == null) return widget.fallback;
      final picture = SvgPicture.memory(
        bytes,
        fit: widget.fit,
        semanticsLabel: widget.semanticLabel,
        excludeFromSemantics: widget.excludeFromSemantics,
        errorBuilder: (context, error, stackTrace) => widget.fallback,
      );
      return widget.wrapper?.call(context, picture) ?? picture;
    },
  );
}
