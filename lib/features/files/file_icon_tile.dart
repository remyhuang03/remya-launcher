import 'package:flutter_svg/flutter_svg.dart';
import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import 'file_icon_palette.dart';
import 'file_icon_resolver.dart';

/// Shared cached loader for bundled Material Icon Theme manifest and seeds.
class FileIconAssets {
  FileIconAssets({required this.resolver, required this.palette});

  final FileIconResolver resolver;
  final FileIconPalette palette;

  static FileIconAssets? _cached;
  static Future<FileIconAssets>? _loading;

  /// Returns already-loaded assets synchronously, or `null` before load completes.
  static FileIconAssets? get cached => _cached;

  /// Loads and caches the bundled manifest and seeds once across the app.
  static Future<FileIconAssets> ensureLoaded({AssetBundle? bundle}) {
    if (_cached case final existing?) return Future.value(existing);
    if (bundle == null && _loading != null) return _loading!;
    final future = _load(bundle: bundle);
    if (bundle == null) {
      _loading = future;
    }
    return future;
  }

  static Future<FileIconAssets> _load({AssetBundle? bundle}) async {
    final results = await Future.wait<Object>([
      FileIconResolver.loadAsset(bundle: bundle),
      FileIconSeeds.loadAsset(bundle: bundle),
    ]);
    final loaded = FileIconAssets(
      resolver: results[0] as FileIconResolver,
      palette: FileIconPalette(seeds: results[1] as FileIconSeeds),
    );
    if (bundle == null) {
      _cached = loaded;
    }
    return loaded;
  }
}

/// Renders a file or folder's Material Icon Theme SVG inside a harmonized
/// Material 3 container without altering the icon's multicolor artwork.
class FileIconTile extends StatefulWidget {
  const FileIconTile({
    required this.basename,
    required this.isDirectory,
    this.isExpanded = false,
    this.isRootFolder = false,
    this.selected = false,
    this.size = 40,
    this.iconSize,
    this.resolver,
    this.palette,
    super.key,
  });

  final String basename;
  final bool isDirectory;
  final bool isExpanded;
  final bool isRootFolder;
  final bool selected;
  final double size;
  final double? iconSize;
  final FileIconResolver? resolver;
  final FileIconPalette? palette;

  static const defaultFileAsset = 'assets/material_icon_theme/icons/file.svg';
  static const defaultFolderAsset =
      'assets/material_icon_theme/icons/folder.svg';
  static const defaultFolderOpenAsset =
      'assets/material_icon_theme/icons/folder-open.svg';

  @override
  State<FileIconTile> createState() => _FileIconTileState();
}

class _FileIconTileState extends State<FileIconTile> {
  Future<FileIconAssets>? _assetsFuture;

  @override
  void initState() {
    super.initState();
    _ensureAssetsIfNeeded();
  }

  @override
  void didUpdateWidget(covariant FileIconTile oldWidget) {
    super.didUpdateWidget(oldWidget);
    _ensureAssetsIfNeeded();
  }

  void _ensureAssetsIfNeeded() {
    if (widget.resolver != null && widget.palette != null) return;
    if (FileIconAssets.cached != null) return;
    _assetsFuture ??= FileIconAssets.ensureLoaded();
  }

  String _defaultIconId() => widget.isDirectory
      ? (widget.isExpanded ? 'folder-open' : 'folder')
      : 'file';

  String _defaultAssetPath() => widget.isDirectory
      ? (widget.isExpanded
            ? FileIconTile.defaultFolderOpenAsset
            : FileIconTile.defaultFolderAsset)
      : FileIconTile.defaultFileAsset;

  @override
  Widget build(BuildContext context) {
    final cached = FileIconAssets.cached;
    if ((widget.resolver != null && widget.palette != null) || cached != null) {
      return _buildTile(
        context,
        resolver: widget.resolver ?? cached?.resolver,
        palette: widget.palette ?? cached?.palette,
      );
    }
    return FutureBuilder<FileIconAssets>(
      future: _assetsFuture,
      builder: (context, snapshot) {
        final loaded = snapshot.data;
        return _buildTile(
          context,
          resolver: widget.resolver ?? loaded?.resolver,
          palette: widget.palette ?? loaded?.palette,
        );
      },
    );
  }

  Widget _buildTile(
    BuildContext context, {
    required FileIconResolver? resolver,
    required FileIconPalette? palette,
  }) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final highContrast = MediaQuery.maybeHighContrastOf(context) ?? false;
    final match = resolver?.resolve(
      basename: widget.basename,
      isDirectory: widget.isDirectory,
      isExpanded: widget.isExpanded,
      isRootFolder: widget.isRootFolder,
      brightness: theme.brightness,
      highContrast: highContrast,
    );
    final defaultId = _defaultIconId();
    final defaultAsset = _defaultAssetPath();
    final effectiveIconId = match == null || match.isFallback
        ? defaultId
        : match.iconId;
    final resolvedAsset =
        match != null && !match.isFallback && match.assetPath != null
        ? match.assetPath!
        : defaultAsset;

    final containerColor = palette != null
        ? palette.containerColorForContext(
            context,
            iconId: effectiveIconId,
            selected: widget.selected,
          )
        : (widget.selected
              ? colors.surfaceContainer
              : colors.surfaceContainerHighest);
    final artworkSize = widget.iconSize ?? widget.size * 0.6;
    final uiFallback = ExcludeSemantics(
      child: LauncherIcon(
        widget.isDirectory ? .folder : .description,
        size: widget.size * 0.5,
      ),
    );

    Widget buildSvg(String assetPath, {required bool allowDefaultRetry}) =>
        SvgPicture.asset(
          assetPath,
          width: artworkSize,
          height: artworkSize,
          fit: .contain,
          excludeFromSemantics: true,
          errorBuilder: (context, error, stackTrace) {
            assert(() {
              debugPrint('FileIconTile: failed to render "$assetPath": $error');
              return true;
            }());
            if (allowDefaultRetry && assetPath != defaultAsset) {
              return buildSvg(defaultAsset, allowDefaultRetry: false);
            }
            return uiFallback;
          },
        );

    return SizedBox.square(
      dimension: widget.size,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: containerColor,
          borderRadius: .circular(12),
        ),
        child: Center(
          child: buildSvg(
            resolvedAsset,
            allowDefaultRetry: resolvedAsset != defaultAsset,
          ),
        ),
      ),
    );
  }
}
