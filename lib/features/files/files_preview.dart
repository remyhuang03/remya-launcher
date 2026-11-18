import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import '../launcher/launcher_preview.dart';
import 'file_icon_tile.dart';
import 'files_models.dart';
import 'files_results.dart';

const _maxImageBytes = 4 * 1024 * 1024;
const _maxBase64Length = 8 * 1024 * 1024;

class FilesPreview extends StatefulWidget {
  const FilesPreview({
    required this.choice,
    required this.result,
    this.data,
    this.loading = false,
    this.compact = false,
    super.key,
  });

  final FileChoice? choice;
  final FilesSearchResult? result;
  final Map<String, dynamic>? data;
  final bool loading;
  final bool compact;

  @override
  State<FilesPreview> createState() => _FilesPreviewState();
}

class _FilesPreviewState extends State<FilesPreview> {
  Map<String, dynamic>? _staleData;
  Map<String, dynamic>? _decodedData;
  String? _decodedBase64;
  Uint8List? _decodedBytes;
  bool _imageTooLarge = false;

  @override
  void initState() {
    super.initState();
    _syncDecodedImage();
  }

  @override
  void didUpdateWidget(covariant FilesPreview oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.choice?.key != oldWidget.choice?.key &&
        widget.data != null &&
        identical(widget.data, oldWidget.data)) {
      _staleData = widget.data;
    } else if (!identical(widget.data, _staleData)) {
      _staleData = null;
    }
    _syncDecodedImage();
  }

  Map<String, dynamic>? get _effectiveData {
    if (widget.loading) return null;
    final data = widget.data;
    if (data == null || identical(data, _staleData)) return null;
    final entry = widget.choice?.entry;
    final dataPath = _nonEmptyString(data['path']);
    if (entry != null && dataPath != null && dataPath != entry.path) {
      return null;
    }
    return data;
  }

  void _syncDecodedImage() {
    final data = _effectiveData;
    if (data == null) {
      _decodedData = null;
      _decodedBase64 = null;
      _decodedBytes = null;
      _imageTooLarge = false;
      return;
    }
    if (identical(data, _decodedData)) return;
    _decodedData = data;
    final raw = _nonEmptyString(data['imageBase64']);
    if (raw == null) {
      _decodedBase64 = null;
      _decodedBytes = null;
      _imageTooLarge = false;
      return;
    }
    if (raw == _decodedBase64) return;
    _decodedBase64 = raw;
    if (raw.length > _maxBase64Length) {
      _decodedBytes = null;
      _imageTooLarge = true;
      return;
    }
    try {
      final bytes = base64Decode(raw);
      if (bytes.isEmpty) {
        _decodedBytes = null;
        _imageTooLarge = false;
      } else if (bytes.length > _maxImageBytes) {
        _decodedBytes = null;
        _imageTooLarge = true;
      } else {
        _decodedBytes = bytes;
        _imageTooLarge = false;
      }
    } on Object {
      _decodedBytes = null;
      _imageTooLarge = false;
    }
  }

  @override
  Widget build(BuildContext context) {
    final choice = widget.choice;
    final centered =
        choice == null ||
        (choice.entry == null &&
            choice.kind != .createFile &&
            choice.kind != .createDirectory);
    final body = switch (choice) {
      null => _EmptyPreview(diagnostic: widget.result?.diagnostic),
      FileChoice(kind: .createFile || .createDirectory) => _CreatePreview(
        choice: choice,
        result: widget.result,
        compact: widget.compact,
      ),
      FileChoice(entry: final entry?) => _buildEntryPreview(context, entry),
      final other => _ActionChoicePreview(choice: other),
    };
    return ClipRRect(
      borderRadius: .circular(20),
      child: LauncherPreviewFrame(
        compact: widget.compact,
        child: ClipRect(
          child: OverflowBox(
            minHeight: 0,
            maxHeight: double.infinity,
            fit: .deferToChild,
            alignment: centered ? .center : AlignmentDirectional.topStart,
            child: body,
          ),
        ),
      ),
    );
  }

  Widget _buildEntryPreview(BuildContext context, FileEntry entry) {
    final data = _effectiveData;
    final kind = _nonEmptyString(data?['kind']);
    final hasImage = _nonEmptyString(data?['imageBase64']) != null;
    if (data == null) {
      return _LoadingOrMetadataPreview(
        entry: entry,
        data: null,
        loading: widget.loading,
        compact: widget.compact,
      );
    }
    if (kind == 'text' && data['text'] is String) {
      return _TextPreview(entry: entry, data: data, compact: widget.compact);
    }
    if (kind == 'image' || kind == 'document' || (kind == null && hasImage)) {
      return _ImagePreview(
        entry: entry,
        data: data,
        bytes: _decodedBytes,
        tooLarge: _imageTooLarge,
        compact: widget.compact,
      );
    }
    return _LoadingOrMetadataPreview(
      entry: entry,
      data: data,
      loading: widget.loading,
      compact: widget.compact,
    );
  }
}

class _EmptyPreview extends StatelessWidget {
  const _EmptyPreview({this.diagnostic});

  final String? diagnostic;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final message =
        _nonEmptyString(diagnostic) ?? 'Select a file or folder to preview';
    return Padding(
      padding: const .symmetric(horizontal: 8, vertical: 16),
      child: Column(
        mainAxisSize: .min,
        spacing: 8,
        children: [
          LauncherIcon(.description, size: 28, color: colors.onSurfaceVariant),
          Text(message, textAlign: .center, style: theme.textTheme.bodySmall),
        ],
      ),
    );
  }
}

class _ActionChoicePreview extends StatelessWidget {
  const _ActionChoicePreview({required this.choice});

  final FileChoice choice;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final icon = switch (choice.kind) {
      .contents => LauncherIconName.search,
      .filter => LauncherIconName.tune,
      .more => LauncherIconName.more,
      _ => LauncherIconName.description,
    };
    return Padding(
      padding: const .symmetric(horizontal: 8, vertical: 16),
      child: Column(
        mainAxisSize: .min,
        spacing: 6,
        children: [
          LauncherIcon(icon, size: 28, color: colors.onSurfaceVariant),
          if (choice.label.isNotEmpty)
            Text(
              choice.label,
              textAlign: .center,
              maxLines: 2,
              overflow: .ellipsis,
              style: theme.textTheme.bodyMedium?.copyWith(fontWeight: .w500),
            ),
          if (choice.detail.isNotEmpty)
            Text(
              choice.detail,
              textAlign: .center,
              maxLines: 3,
              overflow: .ellipsis,
              style: theme.textTheme.bodySmall,
            ),
        ],
      ),
    );
  }
}

class _LoadingOrMetadataPreview extends StatelessWidget {
  const _LoadingOrMetadataPreview({
    required this.entry,
    required this.data,
    required this.loading,
    required this.compact,
  });

  final FileEntry entry;
  final Map<String, dynamic>? data;
  final bool loading;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final spacing = compact ? 8.0 : 12.0;
    final kind = _nonEmptyString(data?['kind']);
    final isDirectory =
        kind == 'directory' || (data == null && entry.isDirectory);
    final name = _nonEmptyString(data?['name']) ?? entry.name;
    final mime = _nonEmptyString(data?['mime']);
    final note =
        _nonEmptyString(data?['note']) ?? _nonEmptyString(data?['diagnostic']);
    final children = _parseChildren(data?['children']);
    final hasChildren = data != null && data!['children'] is List;
    final childCount = _asInt(data?['childCount']);
    final hiddenCount = _asInt(data?['hiddenCount']);
    final truncated = data?['truncated'] == true;
    final hidden = data?['hidden'] == true;
    final size = isDirectory ? null : (_asInt(data?['size']) ?? entry.size);
    final modified = _asInt(data?['modified']) ?? entry.modified;
    final path =
        entry.displayPath ?? _nonEmptyString(data?['path']) ?? entry.path;

    final String summary;
    if (isDirectory) {
      if (childCount != null) {
        summary = '$childCount ${childCount == 1 ? 'item' : 'items'}';
      } else if (hasChildren) {
        if (truncated) {
          summary = '${children.length}+ items';
        } else if (children.isEmpty) {
          summary = 'Empty folder';
        } else {
          summary =
              '${children.length} ${children.length == 1 ? 'item' : 'items'}';
        }
      } else {
        summary = entry.isPlace ? 'Place' : 'Folder';
      }
    } else if (mime != null) {
      summary = entry.executable ? '$mime · Executable' : mime;
    } else if (entry.kind == 'symlink') {
      summary = 'Symbolic link';
    } else if (entry.executable) {
      summary = 'Executable file';
    } else {
      summary = 'File';
    }

    final shownChildren = children.take(5).toList(growable: false);
    final int? remainingCount;
    if (childCount != null && childCount > shownChildren.length) {
      remainingCount = childCount - shownChildren.length;
    } else if (childCount == null &&
        !truncated &&
        children.length > shownChildren.length) {
      remainingCount = children.length - shownChildren.length;
    } else {
      remainingCount = null;
    }

    final metadata = <String, String>{
      if (kind != null && kind != 'directory' && kind != 'metadata')
        'Type': kind,
      if (size != null) 'Size': ?_formatSize(size),
      if (modified != null) 'Modified': ?_formatModified(modified),
      if (hiddenCount != null && hiddenCount > 0)
        'Hidden': '$hiddenCount'
      else if (hidden)
        'Hidden': 'Yes',
    };

    final snippet = entry.snippet;
    final showSnippet =
        snippet != null && snippet.isNotEmpty && (loading || data == null);

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: spacing,
      children: [
        Align(
          alignment: AlignmentDirectional.centerStart,
          child: FileIconTile(
            basename: name,
            isDirectory: isDirectory,
            size: 56,
          ),
        ),
        _PreviewTitle(title: name, subtitle: summary),
        if (loading)
          LinearProgressIndicator(
            minHeight: 2,
            borderRadius: .circular(2),
            semanticsLabel: 'Loading preview',
          ),
        if (showSnippet)
          LauncherPreviewContent(
            child: LauncherPreviewCode(
              snippet,
              firstLine: math.max(1, entry.line ?? 1),
              highlightedLine: entry.line,
              showRemainder: false,
            ),
          )
        else if (isDirectory && hasChildren)
          _DirectoryEntriesList(
            entries: shownChildren,
            remainingCount: remainingCount,
            truncatedWithoutTotal:
                remainingCount == null && truncated && children.isNotEmpty,
          ),
        if (note != null)
          LauncherPreviewContent(
            child: Text(note, style: theme.textTheme.bodySmall),
          ),
        if (metadata.isNotEmpty || path.isNotEmpty)
          _PreviewFooter(metadata: metadata, path: path),
      ],
    );
  }
}

class _TextPreview extends StatelessWidget {
  const _TextPreview({
    required this.entry,
    required this.data,
    required this.compact,
  });

  final FileEntry entry;
  final Map<String, dynamic> data;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final spacing = compact ? 8.0 : 12.0;
    final name = _nonEmptyString(data['name']) ?? entry.name;
    final mime = _nonEmptyString(data['mime']);
    final text = data['text'] as String;
    final startLine = math.max(1, _asInt(data['startLine']) ?? 1);
    final selectedLine = _asInt(data['selectedLine']) ?? entry.line;
    final truncated = data['truncated'] == true;
    final size = _asInt(data['size']) ?? entry.size;
    final modified = _asInt(data['modified']) ?? entry.modified;
    final note =
        _nonEmptyString(data['note']) ?? _nonEmptyString(data['diagnostic']);
    final path =
        entry.displayPath ?? _nonEmptyString(data['path']) ?? entry.path;

    final lineCount = text.isEmpty ? 0 : math.min(8, text.split('\n').length);
    final String lineInfo;
    if (lineCount == 0) {
      lineInfo = 'Empty';
    } else if (!truncated && startLine == 1) {
      lineInfo = '$lineCount ${lineCount == 1 ? 'line' : 'lines'}';
    } else {
      final endLine = startLine + lineCount - 1;
      lineInfo = 'Lines $startLine–$endLine${truncated ? '+' : ''}';
    }

    final subtitleParts = <String>[
      ?mime,
      if (selectedLine != null) 'Line $selectedLine',
      lineInfo,
    ];

    final metadata = <String, String>{
      if (size != null) 'Size': ?_formatSize(size),
      if (modified != null) 'Modified': ?_formatModified(modified),
      if (data['hidden'] == true) 'Hidden': 'Yes',
    };

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: spacing,
      children: [
        _CompactPreviewHeader(
          basename: name,
          isDirectory: false,
          title: name,
          subtitle: subtitleParts.join(' · '),
        ),
        if (text.isNotEmpty)
          LauncherPreviewContent(
            child: LauncherPreviewCode(
              text,
              firstLine: startLine,
              highlightedLine: selectedLine,
              showRemainder: false,
            ),
          )
        else
          LauncherPreviewContent(
            child: Text('Empty file', style: theme.textTheme.bodySmall),
          ),
        if (note != null)
          Text(
            note,
            maxLines: 3,
            overflow: .ellipsis,
            style: theme.textTheme.bodySmall,
          ),
        if (metadata.isNotEmpty || path.isNotEmpty)
          _PreviewFooter(metadata: metadata, path: path),
      ],
    );
  }
}

class _ImagePreview extends StatelessWidget {
  const _ImagePreview({
    required this.entry,
    required this.data,
    required this.bytes,
    required this.tooLarge,
    required this.compact,
  });

  final FileEntry entry;
  final Map<String, dynamic> data;
  final Uint8List? bytes;
  final bool tooLarge;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final spacing = compact ? 8.0 : 12.0;
    final name = _nonEmptyString(data['name']) ?? entry.name;
    final mime = _nonEmptyString(data['mime']);
    final width = _asInt(data['width']);
    final height = _asInt(data['height']);
    final hasDimensions =
        width != null && height != null && width > 0 && height > 0;
    final dimensions = hasDimensions ? '$width × $height' : null;
    final subtitle = dimensions ?? mime ?? 'Image';
    final size = _asInt(data['size']) ?? entry.size;
    final modified = _asInt(data['modified']) ?? entry.modified;
    final note =
        _nonEmptyString(data['note']) ?? _nonEmptyString(data['diagnostic']);
    final path =
        entry.displayPath ?? _nonEmptyString(data['path']) ?? entry.path;

    final cacheWidth = (248 * MediaQuery.devicePixelRatioOf(context))
        .round()
        .clamp(248, 768);

    final metadata = <String, String>{
      if (hasDimensions && mime != null) 'MIME': mime,
      if (size != null) 'Size': ?_formatSize(size),
      if (modified != null) 'Modified': ?_formatModified(modified),
      if (data['hidden'] == true) 'Hidden': 'Yes',
    };

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: spacing,
      children: [
        _CompactPreviewHeader(
          basename: name,
          isDirectory: false,
          title: name,
          subtitle: subtitle,
        ),
        if (bytes case final imageBytes?)
          LauncherPreviewImageFrame(
            maxHeight: 148,
            child: Center(
              heightFactor: 1,
              child: Image.memory(
                imageBytes,
                fit: .contain,
                cacheWidth: cacheWidth,
                semanticLabel: name,
                errorBuilder: (context, error, stackTrace) =>
                    const _ImageFallback('Image preview unavailable'),
              ),
            ),
          )
        else
          LauncherPreviewContent(
            child: _ImageFallback(
              tooLarge
                  ? 'Image preview exceeds 4 MiB limit'
                  : 'Image preview unavailable',
            ),
          ),
        if (note != null)
          Text(
            note,
            maxLines: 3,
            overflow: .ellipsis,
            style: theme.textTheme.bodySmall,
          ),
        if (metadata.isNotEmpty || path.isNotEmpty)
          _PreviewFooter(metadata: metadata, path: path),
      ],
    );
  }
}

class _ImageFallback extends StatelessWidget {
  const _ImageFallback(this.message);

  final String message;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Padding(
      padding: const .symmetric(vertical: 12, horizontal: 8),
      child: Column(
        mainAxisSize: .min,
        spacing: 6,
        children: [
          LauncherIcon(.image, size: 24, color: colors.onSurfaceVariant),
          Text(message, textAlign: .center, style: theme.textTheme.bodySmall),
        ],
      ),
    );
  }
}

class _CreatePreview extends StatelessWidget {
  const _CreatePreview({
    required this.choice,
    required this.result,
    required this.compact,
  });

  final FileChoice choice;
  final FilesSearchResult? result;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final spacing = compact ? 8.0 : 12.0;
    final isDirectory = choice.kind == .createDirectory;
    final name = _nonEmptyString(result?.createName) ?? _fallbackCreateName();
    final createPath =
        _nonEmptyString(result?.createPath) ?? _nonEmptyString(choice.detail);
    final ancestors = result?.createAncestors ?? const <String>[];
    final writable = result?.createWritable ?? true;
    final baseFolder =
        _nonEmptyString(result?.directory) ??
        _inferExistingBase(createPath, result?.createRelative);
    final displayName = isDirectory && !name.endsWith('/') ? '$name/' : name;

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: spacing,
      children: [
        Align(
          alignment: AlignmentDirectional.centerStart,
          child: FileIconTile(
            basename: name,
            isDirectory: isDirectory,
            size: 56,
          ),
        ),
        _PreviewTitle(
          title: displayName,
          subtitle: isDirectory ? 'New folder' : 'New file',
        ),
        LauncherPreviewContent(
          child: _CreateTree(
            baseFolder: baseFolder,
            ancestors: ancestors,
            targetName: displayName,
            isDirectory: isDirectory,
          ),
        ),
        if (!writable)
          Container(
            padding: const .symmetric(horizontal: 10, vertical: 8),
            decoration: BoxDecoration(
              color: colors.errorContainer,
              borderRadius: .circular(12),
            ),
            child: Row(
              crossAxisAlignment: .start,
              spacing: 8,
              children: [
                Padding(
                  padding: const .only(top: 1),
                  child: LauncherIcon(
                    .warning,
                    size: 16,
                    color: colors.onErrorContainer,
                  ),
                ),
                Expanded(
                  child: Text(
                    'Destination folder is not writable.',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: colors.onErrorContainer,
                    ),
                  ),
                ),
              ],
            ),
          ),
        if (createPath != null) ...[
          Divider(height: 1, thickness: 1, color: colors.outlineVariant),
          _CondensedPathText(createPath),
        ],
      ],
    );
  }

  String _fallbackCreateName() {
    final label = choice.label.trim();
    const folderPrefix = 'Create folder ';
    const filePrefix = 'Create ';
    final stripped = label.startsWith(folderPrefix)
        ? label.substring(folderPrefix.length)
        : label.startsWith(filePrefix)
        ? label.substring(filePrefix.length)
        : label;
    final clean = stripped.endsWith('/')
        ? stripped.substring(0, stripped.length - 1)
        : stripped;
    return clean.isEmpty ? 'Untitled' : clean;
  }

  String? _inferExistingBase(String? fullPath, String? relative) {
    if (fullPath == null || relative == null || relative.isEmpty) return null;
    if (!fullPath.endsWith(relative)) return null;
    final cut = fullPath.length - relative.length;
    if (cut <= 0) return null;
    final prefix = fullPath.substring(0, cut);
    if (prefix.length > 1 && prefix.endsWith('/')) {
      return prefix.substring(0, prefix.length - 1);
    }
    return prefix;
  }
}

class _CreateTree extends StatelessWidget {
  const _CreateTree({
    required this.baseFolder,
    required this.ancestors,
    required this.targetName,
    required this.isDirectory,
  });

  final String? baseFolder;
  final List<String> ancestors;
  final String targetName;
  final bool isDirectory;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final rows = <Widget>[];
    var depth = 0;

    if (baseFolder != null) {
      rows.add(
        _TreeRow(
          depth: depth,
          icon: .folder,
          label: _condensePath(baseFolder!),
          color: colors.onSurfaceVariant,
        ),
      );
      depth++;
    }

    for (final ancestor in ancestors) {
      rows.add(
        _TreeRow(
          depth: depth,
          icon: .folder,
          label: ancestor.endsWith('/') ? ancestor : '$ancestor/',
          color: colors.onSurfaceVariant,
        ),
      );
      depth++;
    }

    rows.add(
      _TreeRow(
        depth: depth,
        icon: isDirectory ? .folder : .description,
        label: targetName,
        color: colors.primary,
        style: theme.textTheme.bodySmall?.copyWith(
          color: colors.onSurface,
          fontWeight: .w600,
        ),
      ),
    );

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: 4,
      children: rows,
    );
  }
}

class _TreeRow extends StatelessWidget {
  const _TreeRow({
    required this.depth,
    required this.icon,
    required this.label,
    required this.color,
    this.style,
  });

  final int depth;
  final LauncherIconName icon;
  final String label;
  final Color color;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: .only(left: math.min(depth, 4) * 12.0),
      child: Row(
        spacing: 6,
        children: [
          LauncherIcon(icon, size: 14, color: color),
          Expanded(
            child: Text(
              label,
              maxLines: 1,
              overflow: .ellipsis,
              style: style ?? theme.textTheme.bodySmall,
            ),
          ),
        ],
      ),
    );
  }
}

class _PreviewTitle extends StatelessWidget {
  const _PreviewTitle({required this.title, required this.subtitle});

  final String title;
  final String subtitle;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .start,
      spacing: 2,
      children: [
        Text(
          title,
          maxLines: 2,
          overflow: .ellipsis,
          style: const TextStyle(
            fontSize: 16,
            fontWeight: .w600,
            height: 20 / 16,
          ),
        ),
        Text(
          subtitle,
          maxLines: 2,
          overflow: .ellipsis,
          style: theme.textTheme.bodySmall,
        ),
      ],
    );
  }
}

class _CompactPreviewHeader extends StatelessWidget {
  const _CompactPreviewHeader({
    required this.basename,
    required this.isDirectory,
    required this.title,
    required this.subtitle,
  });

  final String basename;
  final bool isDirectory;
  final String title;
  final String subtitle;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      crossAxisAlignment: .start,
      spacing: 10,
      children: [
        FileIconTile(
          basename: basename,
          isDirectory: isDirectory,
          size: 28,
          iconSize: 18,
        ),
        Expanded(
          child: Column(
            mainAxisSize: .min,
            crossAxisAlignment: .start,
            spacing: 2,
            children: [
              Text(
                title,
                maxLines: 2,
                overflow: .ellipsis,
                style: const TextStyle(
                  fontSize: 15,
                  fontWeight: .w600,
                  height: 20 / 15,
                ),
              ),
              Text(
                subtitle,
                maxLines: 2,
                overflow: .ellipsis,
                style: theme.textTheme.bodySmall,
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _DirectoryEntriesList extends StatelessWidget {
  const _DirectoryEntriesList({
    required this.entries,
    required this.remainingCount,
    required this.truncatedWithoutTotal,
  });

  final List<_DirectoryChild> entries;
  final int? remainingCount;
  final bool truncatedWithoutTotal;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    if (entries.isEmpty) {
      return Text('Empty folder', style: theme.textTheme.bodySmall);
    }
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: 6,
      children: [
        for (final item in entries)
          Row(
            spacing: 8,
            children: [
              LauncherIcon(
                item.directory ? .folder : .description,
                size: 16,
                color: item.directory ? colors.primary : null,
              ),
              Expanded(
                child: Text(
                  item.name,
                  maxLines: 1,
                  overflow: .ellipsis,
                  style: theme.textTheme.bodySmall,
                ),
              ),
            ],
          ),
        if (remainingCount case final remaining? when remaining > 0)
          Text(
            '+$remaining more',
            style: theme.textTheme.bodySmall?.copyWith(fontSize: 11),
          )
        else if (truncatedWithoutTotal)
          Text(
            'More items…',
            style: theme.textTheme.bodySmall?.copyWith(fontSize: 11),
          ),
      ],
    );
  }
}

class _PreviewFooter extends StatelessWidget {
  const _PreviewFooter({required this.metadata, required this.path});

  final Map<String, String> metadata;
  final String path;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: 8,
      children: [
        if (metadata.isNotEmpty || path.isNotEmpty)
          Divider(height: 1, thickness: 1, color: colors.outlineVariant),
        if (metadata.isNotEmpty) LauncherPreviewMetadata(metadata),
        if (path.isNotEmpty) _CondensedPathText(path),
      ],
    );
  }
}

class _CondensedPathText extends StatelessWidget {
  const _CondensedPathText(this.path);

  final String path;

  @override
  Widget build(BuildContext context) => Text(
    _condensePath(path),
    maxLines: 2,
    overflow: .ellipsis,
    style: Theme.of(context).textTheme.bodySmall?.copyWith(fontSize: 11),
  );
}

class _DirectoryChild {
  const _DirectoryChild(this.name, this.directory);

  final String name;
  final bool directory;
}

List<_DirectoryChild> _parseChildren(Object? raw) {
  if (raw is! List) return const [];
  final result = <_DirectoryChild>[];
  for (final item in raw) {
    if (item is Map) {
      final name = _nonEmptyString(item['name']);
      if (name != null) {
        result.add(_DirectoryChild(name, item['directory'] == true));
      }
    }
  }
  return result;
}

String? _formatSize(int bytes) {
  if (bytes < 0) return null;
  final formatted = filesSize(bytes);
  return formatted.isEmpty ? null : formatted;
}

String? _formatModified(int seconds) {
  if (seconds <= 0) return null;
  final formatted = filesModified(seconds);
  return formatted.isEmpty ? null : formatted;
}

int? _asInt(Object? value) =>
    value is int ? value : (value is num ? value.toInt() : null);

String? _nonEmptyString(Object? value) =>
    value is String && value.isNotEmpty ? value : null;

String _condensePath(String path) {
  if (path.length <= 38) return path;
  final isHome = path == '~' || path.startsWith('~/');
  final isRoot = path.startsWith('/');
  final rawSegments = path.split('/').where((s) => s.isNotEmpty).toList();
  if (isHome && rawSegments.isNotEmpty && rawSegments.first == '~') {
    rawSegments.removeAt(0);
  }
  if (rawSegments.length <= 3) return path;
  final prefix = isHome
      ? '~/${rawSegments.first}'
      : isRoot
      ? '/${rawSegments.first}'
      : rawSegments.first;
  final tail = rawSegments.sublist(rawSegments.length - 2).join('/');
  return '$prefix/…/$tail';
}
