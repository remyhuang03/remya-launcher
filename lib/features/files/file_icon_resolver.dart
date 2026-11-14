import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Result of resolving a file or directory basename against the icon manifest.
@immutable
class FileIconMatch {
  const FileIconMatch({
    required this.iconId,
    required this.assetPath,
    this.isFallback = false,
  });

  final String iconId;
  final String? assetPath;
  final bool isFallback;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is FileIconMatch &&
          other.iconId == iconId &&
          other.assetPath == assetPath &&
          other.isFallback == isFallback;

  @override
  int get hashCode => Object.hash(iconId, assetPath, isFallback);

  @override
  String toString() =>
      'FileIconMatch(iconId: $iconId, assetPath: $assetPath, '
      'isFallback: $isFallback)';
}

/// Resolves file and directory basenames to bundled Material Icon Theme SVGs.
class FileIconResolver {
  const FileIconResolver._(
    this._base,
    this._light,
    this._highContrast,
    this._assets,
    this._onDiagnostic,
  );

  static const defaultAssetPath = 'assets/material_icon_theme/manifest.json';

  final _IconLayer _base;
  final _IconLayer _light;
  final _IconLayer _highContrast;
  final Map<String, String> _assets;
  final void Function(String message)? _onDiagnostic;

  /// Creates an empty resolver that always returns fallback matches.
  factory FileIconResolver.empty({
    void Function(String message)? onDiagnostic,
  }) => FileIconResolver._(
    const _IconLayer(),
    const _IconLayer(),
    const _IconLayer(),
    const {},
    onDiagnostic,
  );

  /// Parses an already-decoded icon manifest JSON map.
  factory FileIconResolver.fromJson(
    Map<String, dynamic> json, {
    void Function(String message)? onDiagnostic,
  }) {
    final schemaVersion = json['schemaVersion'];
    if (schemaVersion != null && schemaVersion != 1) {
      _emitDiagnostic(
        onDiagnostic,
        'Unsupported icon manifest schemaVersion: $schemaVersion.',
      );
    }

    final base = _parseLayer(json, 'root', onDiagnostic);
    final light = _parseOptionalLayer(json['light'], 'light', onDiagnostic);
    final highContrast = _parseOptionalLayer(
      json['highContrast'],
      'highContrast',
      onDiagnostic,
    );
    final assets = _parseStringMap(
      json['assets'],
      'assets',
      lowercaseKeys: false,
      onDiagnostic: onDiagnostic,
    );

    return FileIconResolver._(base, light, highContrast, assets, onDiagnostic);
  }

  /// Parses a raw JSON manifest string, returning an empty resolver on failure.
  factory FileIconResolver.fromJsonString(
    String source, {
    void Function(String message)? onDiagnostic,
  }) {
    try {
      final decoded = jsonDecode(source);
      if (decoded is Map<String, dynamic>) {
        return FileIconResolver.fromJson(decoded, onDiagnostic: onDiagnostic);
      }
      if (decoded is Map) {
        return FileIconResolver.fromJson(
          decoded.map((key, value) => MapEntry(key.toString(), value)),
          onDiagnostic: onDiagnostic,
        );
      }
      _emitDiagnostic(
        onDiagnostic,
        'Expected JSON object at root of icon manifest, '
        'got ${decoded.runtimeType}.',
      );
    } catch (error) {
      _emitDiagnostic(
        onDiagnostic,
        'Failed to parse icon manifest JSON: $error',
      );
    }
    return FileIconResolver.empty(onDiagnostic: onDiagnostic);
  }

  /// Loads and parses the bundled icon manifest asset without throwing.
  static Future<FileIconResolver> loadAsset({
    AssetBundle? bundle,
    String path = defaultAssetPath,
    void Function(String message)? onDiagnostic,
  }) async {
    try {
      final data = await (bundle ?? rootBundle).load(path);
      final source = utf8.decode(
        data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
      );
      return FileIconResolver.fromJsonString(
        source,
        onDiagnostic: onDiagnostic,
      );
    } catch (error) {
      _emitDiagnostic(
        onDiagnostic,
        'Failed to load icon manifest asset "$path": $error',
      );
      return FileIconResolver.empty(onDiagnostic: onDiagnostic);
    }
  }

  /// Resolves [basename] to an icon ID and bundled SVG asset path.
  FileIconMatch resolve({
    required String basename,
    required bool isDirectory,
    bool isExpanded = false,
    bool isRootFolder = false,
    Brightness? brightness,
    bool highContrast = false,
  }) {
    final normalized = basename.length > 1 && basename.endsWith('/')
        ? basename.substring(0, basename.length - 1)
        : basename;
    final lower = normalized.toLowerCase();
    final useLight = brightness == .light;

    final iconId = isDirectory
        ? _resolveDirectoryIconId(
            lower,
            isExpanded: isExpanded,
            isRootFolder: isRootFolder,
            useLight: useLight,
            highContrast: highContrast,
          )
        : _resolveFileIconId(
            lower,
            useLight: useLight,
            highContrast: highContrast,
          );

    final resolvedIconId = iconId ?? '';
    final assetPath = resolvedIconId.isEmpty ? null : _assets[resolvedIconId];
    if (resolvedIconId.isEmpty || assetPath == null || assetPath.isEmpty) {
      final fallbackId = resolvedIconId.isEmpty
          ? (isDirectory ? 'folder' : 'file')
          : resolvedIconId;
      _emitDiagnostic(
        _onDiagnostic,
        resolvedIconId.isEmpty
            ? 'No icon ID resolved for "$basename" '
                  '(isDirectory: $isDirectory); using fallback "$fallbackId".'
            : 'Missing asset path for icon ID "$resolvedIconId" '
                  '(basename: "$basename"); returning fallback.',
      );
      return FileIconMatch(
        iconId: fallbackId,
        assetPath: null,
        isFallback: true,
      );
    }

    return FileIconMatch(iconId: resolvedIconId, assetPath: assetPath);
  }

  String? _resolveDirectoryIconId(
    String lower, {
    required bool isExpanded,
    required bool isRootFolder,
    required bool useLight,
    required bool highContrast,
  }) {
    if (isRootFolder) {
      final rootMatch = _lookupMap(
        lower,
        useLight: useLight,
        highContrast: highContrast,
        select: (layer) => isExpanded
            ? layer.rootFoldersByNameExpanded
            : layer.rootFoldersByName,
      );
      if (rootMatch != null) return rootMatch;
    }

    final folderMatch = _lookupMap(
      lower,
      useLight: useLight,
      highContrast: highContrast,
      select: (layer) =>
          isExpanded ? layer.foldersByNameExpanded : layer.foldersByName,
    );
    if (folderMatch != null) return folderMatch;

    if (isRootFolder) {
      final rootDefault = _lookupValue(
        useLight: useLight,
        highContrast: highContrast,
        select: (layer) => isExpanded
            ? layer.defaultRootFolderExpanded
            : layer.defaultRootFolder,
      );
      if (rootDefault != null) return rootDefault;
    }

    return _lookupValue(
      useLight: useLight,
      highContrast: highContrast,
      select: (layer) =>
          isExpanded ? layer.defaultFolderExpanded : layer.defaultFolder,
    );
  }

  String? _resolveFileIconId(
    String lower, {
    required bool useLight,
    required bool highContrast,
  }) {
    final exactMatch = _lookupMap(
      lower,
      useLight: useLight,
      highContrast: highContrast,
      select: (layer) => layer.filesByName,
    );
    if (exactMatch != null) return exactMatch;

    for (var i = 0; i < lower.length - 1; i++) {
      if (lower.codeUnitAt(i) != 0x2e) continue;
      final extension = lower.substring(i + 1);
      final extensionMatch = _lookupMap(
        extension,
        useLight: useLight,
        highContrast: highContrast,
        select: (layer) => layer.filesByExtension,
      );
      if (extensionMatch != null) return extensionMatch;
    }

    return _lookupValue(
      useLight: useLight,
      highContrast: highContrast,
      select: (layer) => layer.defaultFile,
    );
  }

  String? _lookupMap(
    String key, {
    required bool useLight,
    required bool highContrast,
    required Map<String, String> Function(_IconLayer layer) select,
  }) {
    if (highContrast) {
      final match = select(_highContrast)[key];
      if (match != null) return match;
    }
    if (useLight) {
      final match = select(_light)[key];
      if (match != null) return match;
    }
    return select(_base)[key];
  }

  String? _lookupValue({
    required bool useLight,
    required bool highContrast,
    required String? Function(_IconLayer layer) select,
  }) {
    if (highContrast) {
      final match = select(_highContrast);
      if (match != null) return match;
    }
    if (useLight) {
      final match = select(_light);
      if (match != null) return match;
    }
    return select(_base);
  }

  static _IconLayer _parseOptionalLayer(
    Object? raw,
    String section,
    void Function(String message)? onDiagnostic,
  ) {
    if (raw == null) return const _IconLayer();
    if (raw is! Map) {
      _emitDiagnostic(
        onDiagnostic,
        'Expected object for "$section" in icon manifest, '
        'got ${raw.runtimeType}.',
      );
      return const _IconLayer();
    }
    return _parseLayer(raw, section, onDiagnostic);
  }

  static _IconLayer _parseLayer(
    Map<Object?, Object?> raw,
    String section,
    void Function(String message)? onDiagnostic,
  ) {
    final defaultsRaw = raw['defaults'];
    Map<Object?, Object?>? defaults;
    if (defaultsRaw is Map) {
      defaults = defaultsRaw;
    } else if (defaultsRaw != null) {
      _emitDiagnostic(
        onDiagnostic,
        'Expected object for "$section.defaults" in icon manifest, '
        'got ${defaultsRaw.runtimeType}.',
      );
    }

    return _IconLayer(
      defaultFile: _readNonEmptyString(
        defaults?['file'],
        '$section.defaults.file',
        onDiagnostic,
      ),
      defaultFolder: _readNonEmptyString(
        defaults?['folder'],
        '$section.defaults.folder',
        onDiagnostic,
      ),
      defaultFolderExpanded: _readNonEmptyString(
        defaults?['folderExpanded'],
        '$section.defaults.folderExpanded',
        onDiagnostic,
      ),
      defaultRootFolder: _readNonEmptyString(
        defaults?['rootFolder'],
        '$section.defaults.rootFolder',
        onDiagnostic,
      ),
      defaultRootFolderExpanded: _readNonEmptyString(
        defaults?['rootFolderExpanded'],
        '$section.defaults.rootFolderExpanded',
        onDiagnostic,
      ),
      filesByName: _parseStringMap(
        raw['filesByName'],
        '$section.filesByName',
        lowercaseKeys: true,
        onDiagnostic: onDiagnostic,
      ),
      filesByExtension: _parseStringMap(
        raw['filesByExtension'],
        '$section.filesByExtension',
        lowercaseKeys: true,
        onDiagnostic: onDiagnostic,
      ),
      foldersByName: _parseStringMap(
        raw['foldersByName'],
        '$section.foldersByName',
        lowercaseKeys: true,
        onDiagnostic: onDiagnostic,
      ),
      foldersByNameExpanded: _parseStringMap(
        raw['foldersByNameExpanded'],
        '$section.foldersByNameExpanded',
        lowercaseKeys: true,
        onDiagnostic: onDiagnostic,
      ),
      rootFoldersByName: _parseStringMap(
        raw['rootFoldersByName'],
        '$section.rootFoldersByName',
        lowercaseKeys: true,
        onDiagnostic: onDiagnostic,
      ),
      rootFoldersByNameExpanded: _parseStringMap(
        raw['rootFoldersByNameExpanded'],
        '$section.rootFoldersByNameExpanded',
        lowercaseKeys: true,
        onDiagnostic: onDiagnostic,
      ),
    );
  }

  static String? _readNonEmptyString(
    Object? raw,
    String field,
    void Function(String message)? onDiagnostic,
  ) {
    if (raw == null) return null;
    if (raw is String && raw.isNotEmpty) return raw;
    _emitDiagnostic(
      onDiagnostic,
      'Expected non-empty string for "$field" in icon manifest.',
    );
    return null;
  }

  static Map<String, String> _parseStringMap(
    Object? raw,
    String section, {
    required bool lowercaseKeys,
    void Function(String message)? onDiagnostic,
  }) {
    if (raw == null) return const {};
    if (raw is! Map) {
      _emitDiagnostic(
        onDiagnostic,
        'Expected object for "$section" in icon manifest, '
        'got ${raw.runtimeType}.',
      );
      return const {};
    }

    final result = <String, String>{};
    for (final entry in raw.entries) {
      final key = entry.key;
      final value = entry.value;
      if (key is! String || key.isEmpty || value is! String || value.isEmpty) {
        _emitDiagnostic(
          onDiagnostic,
          'Ignoring invalid entry in "$section": $key -> $value.',
        );
        continue;
      }
      result[lowercaseKeys ? key.toLowerCase() : key] = value;
    }
    return Map<String, String>.unmodifiable(result);
  }

  static void _emitDiagnostic(
    void Function(String message)? onDiagnostic,
    String message,
  ) {
    onDiagnostic?.call(message);
    assert(() {
      debugPrint('FileIconResolver: $message');
      return true;
    }());
  }
}

@immutable
class _IconLayer {
  const _IconLayer({
    this.defaultFile,
    this.defaultFolder,
    this.defaultFolderExpanded,
    this.defaultRootFolder,
    this.defaultRootFolderExpanded,
    this.filesByName = const {},
    this.filesByExtension = const {},
    this.foldersByName = const {},
    this.foldersByNameExpanded = const {},
    this.rootFoldersByName = const {},
    this.rootFoldersByNameExpanded = const {},
  });

  final String? defaultFile;
  final String? defaultFolder;
  final String? defaultFolderExpanded;
  final String? defaultRootFolder;
  final String? defaultRootFolderExpanded;
  final Map<String, String> filesByName;
  final Map<String, String> filesByExtension;
  final Map<String, String> foldersByName;
  final Map<String, String> foldersByNameExpanded;
  final Map<String, String> rootFoldersByName;
  final Map<String, String> rootFoldersByNameExpanded;
}
