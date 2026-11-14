import 'package:flutter_test/flutter_test.dart';
import 'package:remya_launcher/features/files/file_icon_resolver.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const fixtureManifest = <String, dynamic>{
    'schemaVersion': 1,
    'defaults': {
      'file': 'file',
      'folder': 'folder',
      'folderExpanded': 'folder-open',
      'rootFolder': 'folder-root',
      'rootFolderExpanded': 'folder-root-open',
    },
    'filesByName': {
      'dockerfile': 'docker',
      'docker-compose.yml': 'docker',
      'readme.md': 'readme',
      '.editorconfig': 'editorconfig',
      'broken-entry': 'missing-asset-id',
    },
    'filesByExtension': {
      'test.ts': 'test-ts',
      'ts': 'typescript',
      'd.ts': 'typescript-def',
      'env': 'tune',
    },
    'foldersByName': {
      'src': 'folder-src',
      'docs': 'folder-docs',
      'broken-folder': 'missing-folder-asset',
    },
    'foldersByNameExpanded': {
      'src': 'folder-src-open',
      'docs': 'folder-docs-open',
    },
    'rootFoldersByName': {'workspace': 'folder-workspace'},
    'rootFoldersByNameExpanded': {'workspace': 'folder-workspace-open'},
    'light': {
      'defaults': {'file': 'file_light'},
      'filesByName': {'readme.md': 'readme_light'},
      'filesByExtension': {'ts': 'typescript_light'},
      'foldersByName': {'docs': 'folder-docs_light'},
    },
    'highContrast': {
      'filesByName': {'readme.md': 'readme_hc'},
      'foldersByNameExpanded': {'docs': 'folder-docs-open_hc'},
    },
    'assets': {
      'file': 'assets/material_icon_theme/icons/file.svg',
      'file_light': 'assets/material_icon_theme/icons/file_light.svg',
      'folder': 'assets/material_icon_theme/icons/folder.svg',
      'folder-open': 'assets/material_icon_theme/icons/folder-open.svg',
      'folder-root': 'assets/material_icon_theme/icons/folder-root.svg',
      'folder-root-open':
          'assets/material_icon_theme/icons/folder-root-open.svg',
      'docker': 'assets/material_icon_theme/icons/docker.svg',
      'readme': 'assets/material_icon_theme/icons/readme.svg',
      'readme_light': 'assets/material_icon_theme/icons/readme_light.svg',
      'readme_hc': 'assets/material_icon_theme/icons/readme_hc.svg',
      'editorconfig': 'assets/material_icon_theme/icons/editorconfig.svg',
      'test-ts': 'assets/material_icon_theme/icons/test-ts.svg',
      'typescript': 'assets/material_icon_theme/icons/typescript.svg',
      'typescript_light':
          'assets/material_icon_theme/icons/typescript_light.svg',
      'typescript-def': 'assets/material_icon_theme/icons/typescript-def.svg',
      'tune': 'assets/material_icon_theme/icons/tune.svg',
      'folder-src': 'assets/material_icon_theme/icons/folder-src.svg',
      'folder-src-open': 'assets/material_icon_theme/icons/folder-src-open.svg',
      'folder-docs': 'assets/material_icon_theme/icons/folder-docs.svg',
      'folder-docs_light':
          'assets/material_icon_theme/icons/folder-docs_light.svg',
      'folder-docs-open':
          'assets/material_icon_theme/icons/folder-docs-open.svg',
      'folder-docs-open_hc':
          'assets/material_icon_theme/icons/folder-docs-open_hc.svg',
      'folder-workspace':
          'assets/material_icon_theme/icons/folder-workspace.svg',
      'folder-workspace-open':
          'assets/material_icon_theme/icons/folder-workspace-open.svg',
    },
  };

  group('FileIconResolver (fixture)', () {
    late List<String> diagnostics;
    late FileIconResolver resolver;

    setUp(() {
      diagnostics = [];
      resolver = FileIconResolver.fromJson(
        fixtureManifest,
        onDiagnostic: diagnostics.add,
      );
    });

    test('matches exact filenames and case-insensitive basenames', () {
      expect(
        resolver.resolve(basename: 'Dockerfile', isDirectory: false),
        const FileIconMatch(
          iconId: 'docker',
          assetPath: 'assets/material_icon_theme/icons/docker.svg',
        ),
      );
      expect(
        resolver.resolve(basename: 'docker-compose.yml', isDirectory: false),
        const FileIconMatch(
          iconId: 'docker',
          assetPath: 'assets/material_icon_theme/icons/docker.svg',
        ),
      );
      expect(
        resolver.resolve(basename: 'README.MD', isDirectory: false),
        const FileIconMatch(
          iconId: 'readme',
          assetPath: 'assets/material_icon_theme/icons/readme.svg',
        ),
      );
      expect(
        resolver.resolve(basename: 'SRC', isDirectory: true),
        const FileIconMatch(
          iconId: 'folder-src',
          assetPath: 'assets/material_icon_theme/icons/folder-src.svg',
        ),
      );
      expect(diagnostics, isEmpty);
    });

    test('strips trailing slash on directories except root "/"', () {
      expect(
        resolver.resolve(basename: 'docs/', isDirectory: true),
        const FileIconMatch(
          iconId: 'folder-docs',
          assetPath: 'assets/material_icon_theme/icons/folder-docs.svg',
        ),
      );
      expect(
        resolver.resolve(basename: '/', isDirectory: true),
        const FileIconMatch(
          iconId: 'folder',
          assetPath: 'assets/material_icon_theme/icons/folder.svg',
        ),
      );
    });

    test('matches longest compound extension before shorter extension', () {
      expect(
        resolver.resolve(basename: 'component.test.ts', isDirectory: false),
        const FileIconMatch(
          iconId: 'test-ts',
          assetPath: 'assets/material_icon_theme/icons/test-ts.svg',
        ),
      );
      expect(
        resolver.resolve(basename: 'types.d.ts', isDirectory: false),
        const FileIconMatch(
          iconId: 'typescript-def',
          assetPath: 'assets/material_icon_theme/icons/typescript-def.svg',
        ),
      );
      expect(
        resolver.resolve(basename: 'index.ts', isDirectory: false),
        const FileIconMatch(
          iconId: 'typescript',
          assetPath: 'assets/material_icon_theme/icons/typescript.svg',
        ),
      );
    });

    test('matches dotfiles via filesByName and filesByExtension', () {
      expect(
        resolver.resolve(basename: '.editorconfig', isDirectory: false),
        const FileIconMatch(
          iconId: 'editorconfig',
          assetPath: 'assets/material_icon_theme/icons/editorconfig.svg',
        ),
      );
      expect(
        resolver.resolve(basename: '.env', isDirectory: false),
        const FileIconMatch(
          iconId: 'tune',
          assetPath: 'assets/material_icon_theme/icons/tune.svg',
        ),
      );
    });

    test('falls back to default file icon for unknown extensions', () {
      expect(
        resolver.resolve(basename: 'dock-layout.qml', isDirectory: false),
        const FileIconMatch(
          iconId: 'file',
          assetPath: 'assets/material_icon_theme/icons/file.svg',
        ),
      );
      expect(diagnostics, isEmpty);
    });

    test('resolves collapsed/expanded regular and root folders with fallbacks', () {
      expect(
        resolver.resolve(
          basename: 'docs',
          isDirectory: true,
          isExpanded: false,
        ),
        const FileIconMatch(
          iconId: 'folder-docs',
          assetPath: 'assets/material_icon_theme/icons/folder-docs.svg',
        ),
      );
      expect(
        resolver.resolve(basename: 'docs', isDirectory: true, isExpanded: true),
        const FileIconMatch(
          iconId: 'folder-docs-open',
          assetPath: 'assets/material_icon_theme/icons/folder-docs-open.svg',
        ),
      );
      expect(
        resolver.resolve(
          basename: 'unknown-dir',
          isDirectory: true,
          isExpanded: false,
        ),
        const FileIconMatch(
          iconId: 'folder',
          assetPath: 'assets/material_icon_theme/icons/folder.svg',
        ),
      );
      expect(
        resolver.resolve(
          basename: 'unknown-dir',
          isDirectory: true,
          isExpanded: true,
        ),
        const FileIconMatch(
          iconId: 'folder-open',
          assetPath: 'assets/material_icon_theme/icons/folder-open.svg',
        ),
      );

      // Named root folder
      expect(
        resolver.resolve(
          basename: 'workspace',
          isDirectory: true,
          isRootFolder: true,
          isExpanded: false,
        ),
        const FileIconMatch(
          iconId: 'folder-workspace',
          assetPath: 'assets/material_icon_theme/icons/folder-workspace.svg',
        ),
      );
      expect(
        resolver.resolve(
          basename: 'workspace',
          isDirectory: true,
          isRootFolder: true,
          isExpanded: true,
        ),
        const FileIconMatch(
          iconId: 'folder-workspace-open',
          assetPath:
              'assets/material_icon_theme/icons/folder-workspace-open.svg',
        ),
      );

      // Root folder falling back to foldersByName when not in rootFoldersByName
      expect(
        resolver.resolve(
          basename: 'src',
          isDirectory: true,
          isRootFolder: true,
          isExpanded: false,
        ),
        const FileIconMatch(
          iconId: 'folder-src',
          assetPath: 'assets/material_icon_theme/icons/folder-src.svg',
        ),
      );

      // Unknown root folder falling back to rootFolder/rootFolderExpanded
      expect(
        resolver.resolve(
          basename: 'my-project',
          isDirectory: true,
          isRootFolder: true,
          isExpanded: false,
        ),
        const FileIconMatch(
          iconId: 'folder-root',
          assetPath: 'assets/material_icon_theme/icons/folder-root.svg',
        ),
      );
      expect(
        resolver.resolve(
          basename: 'my-project',
          isDirectory: true,
          isRootFolder: true,
          isExpanded: true,
        ),
        const FileIconMatch(
          iconId: 'folder-root-open',
          assetPath: 'assets/material_icon_theme/icons/folder-root-open.svg',
        ),
      );
    });

    test('applies light and highContrast partial overlays while preserving base', () {
      // Light overlay on readme, file default, ts extension, and docs folder
      expect(
        resolver.resolve(
          basename: 'README.md',
          isDirectory: false,
          brightness: .light,
        ),
        const FileIconMatch(
          iconId: 'readme_light',
          assetPath: 'assets/material_icon_theme/icons/readme_light.svg',
        ),
      );
      expect(
        resolver.resolve(
          basename: 'index.ts',
          isDirectory: false,
          brightness: .light,
        ),
        const FileIconMatch(
          iconId: 'typescript_light',
          assetPath: 'assets/material_icon_theme/icons/typescript_light.svg',
        ),
      );
      // Base entry preserved when light has no override for test.ts or docker
      expect(
        resolver.resolve(
          basename: 'component.test.ts',
          isDirectory: false,
          brightness: .light,
        ),
        const FileIconMatch(
          iconId: 'test-ts',
          assetPath: 'assets/material_icon_theme/icons/test-ts.svg',
        ),
      );
      expect(
        resolver.resolve(
          basename: 'Dockerfile',
          isDirectory: false,
          brightness: .light,
        ),
        const FileIconMatch(
          iconId: 'docker',
          assetPath: 'assets/material_icon_theme/icons/docker.svg',
        ),
      );
      // Light default file
      expect(
        resolver.resolve(
          basename: 'dock-layout.qml',
          isDirectory: false,
          brightness: .light,
        ),
        const FileIconMatch(
          iconId: 'file_light',
          assetPath: 'assets/material_icon_theme/icons/file_light.svg',
        ),
      );
      // Dark brightness does not activate light overlay
      expect(
        resolver.resolve(
          basename: 'README.md',
          isDirectory: false,
          brightness: .dark,
        ),
        const FileIconMatch(
          iconId: 'readme',
          assetPath: 'assets/material_icon_theme/icons/readme.svg',
        ),
      );

      // highContrast takes precedence over light, while preserving light & base
      expect(
        resolver.resolve(
          basename: 'README.md',
          isDirectory: false,
          brightness: .light,
          highContrast: true,
        ),
        const FileIconMatch(
          iconId: 'readme_hc',
          assetPath: 'assets/material_icon_theme/icons/readme_hc.svg',
        ),
      );
      expect(
        resolver.resolve(
          basename: 'docs',
          isDirectory: true,
          isExpanded: true,
          brightness: .light,
          highContrast: true,
        ),
        const FileIconMatch(
          iconId: 'folder-docs-open_hc',
          assetPath: 'assets/material_icon_theme/icons/folder-docs-open_hc.svg',
        ),
      );
      expect(
        resolver.resolve(
          basename: 'docs',
          isDirectory: true,
          isExpanded: false,
          brightness: .light,
          highContrast: true,
        ),
        const FileIconMatch(
          iconId: 'folder-docs_light',
          assetPath: 'assets/material_icon_theme/icons/folder-docs_light.svg',
        ),
      );
      expect(
        resolver.resolve(
          basename: 'Dockerfile',
          isDirectory: false,
          brightness: .light,
          highContrast: true,
        ),
        const FileIconMatch(
          iconId: 'docker',
          assetPath: 'assets/material_icon_theme/icons/docker.svg',
        ),
      );
    });

    test('returns fallback with diagnostic for missing asset ID', () {
      final fileMatch = resolver.resolve(
        basename: 'broken-entry',
        isDirectory: false,
      );
      expect(
        fileMatch,
        const FileIconMatch(
          iconId: 'missing-asset-id',
          assetPath: null,
          isFallback: true,
        ),
      );

      final folderMatch = resolver.resolve(
        basename: 'broken-folder',
        isDirectory: true,
      );
      expect(
        folderMatch,
        const FileIconMatch(
          iconId: 'missing-folder-asset',
          assetPath: null,
          isFallback: true,
        ),
      );
      expect(diagnostics, hasLength(2));
    });

    test(
      'handles empty resolver and malformed JSON without throwing',
      () async {
        final emptyDiagnostics = <String>[];
        final empty = FileIconResolver.empty(
          onDiagnostic: emptyDiagnostics.add,
        );
        expect(
          empty.resolve(basename: 'main.dart', isDirectory: false),
          const FileIconMatch(
            iconId: 'file',
            assetPath: null,
            isFallback: true,
          ),
        );
        expect(
          empty.resolve(basename: 'src', isDirectory: true),
          const FileIconMatch(
            iconId: 'folder',
            assetPath: null,
            isFallback: true,
          ),
        );
        expect(emptyDiagnostics, hasLength(2));

        final malformedDiagnostics = <String>[];
        final badSyntax = FileIconResolver.fromJsonString(
          '{not valid json',
          onDiagnostic: malformedDiagnostics.add,
        );
        expect(
          badSyntax.resolve(basename: 'a.txt', isDirectory: false),
          const FileIconMatch(
            iconId: 'file',
            assetPath: null,
            isFallback: true,
          ),
        );
        expect(malformedDiagnostics, hasLength(2));

        final nonObjectDiagnostics = <String>[];
        final nonObject = FileIconResolver.fromJsonString(
          '["not", "an", "object"]',
          onDiagnostic: nonObjectDiagnostics.add,
        );
        expect(
          nonObject.resolve(basename: 'a.txt', isDirectory: false).isFallback,
          isTrue,
        );
        expect(nonObjectDiagnostics, hasLength(2));

        final bundleDiagnostics = <String>[];
        final missingAsset = await FileIconResolver.loadAsset(
          path: 'assets/missing.json',
          onDiagnostic: bundleDiagnostics.add,
        );
        expect(
          missingAsset.resolve(basename: 'dir', isDirectory: true),
          const FileIconMatch(
            iconId: 'folder',
            assetPath: null,
            isFallback: true,
          ),
        );
        expect(bundleDiagnostics, hasLength(2));
      },
    );
  });

  group('FileIconResolver (bundled manifest integration)', () {
    test(
      'loads rootBundle manifest and resolves real project entries',
      () async {
        final diagnostics = <String>[];
        final resolver = await FileIconResolver.loadAsset(
          onDiagnostic: diagnostics.add,
        );

        expect(diagnostics, isEmpty);

        expect(
          resolver.resolve(basename: 'lib.rs', isDirectory: false),
          const FileIconMatch(
            iconId: 'rust',
            assetPath: 'assets/material_icon_theme/icons/rust.svg',
          ),
        );
        expect(
          resolver.resolve(basename: 'main.dart', isDirectory: false),
          const FileIconMatch(
            iconId: 'dart',
            assetPath: 'assets/material_icon_theme/icons/dart.svg',
          ),
        );
        expect(
          resolver.resolve(basename: 'docker-compose.yml', isDirectory: false),
          const FileIconMatch(
            iconId: 'docker',
            assetPath: 'assets/material_icon_theme/icons/docker.svg',
          ),
        );
        expect(
          resolver.resolve(basename: 'fuse_attention.py', isDirectory: false),
          const FileIconMatch(
            iconId: 'python',
            assetPath: 'assets/material_icon_theme/icons/python.svg',
          ),
        );
        expect(
          resolver.resolve(basename: '.editorconfig', isDirectory: false),
          const FileIconMatch(
            iconId: 'editorconfig',
            assetPath: 'assets/material_icon_theme/icons/editorconfig.svg',
          ),
        );
        expect(
          resolver.resolve(basename: 'dock-layout.qml', isDirectory: false),
          const FileIconMatch(
            iconId: 'file',
            assetPath: 'assets/material_icon_theme/icons/file.svg',
          ),
        );
        expect(
          resolver.resolve(basename: 'dock-preview.png', isDirectory: false),
          const FileIconMatch(
            iconId: 'image',
            assetPath: 'assets/material_icon_theme/icons/image.svg',
          ),
        );
        expect(
          resolver.resolve(basename: 'docs/', isDirectory: true),
          const FileIconMatch(
            iconId: 'folder-docs',
            assetPath: 'assets/material_icon_theme/icons/folder-docs.svg',
          ),
        );
        expect(
          resolver.resolve(basename: 'component.test.ts', isDirectory: false),
          const FileIconMatch(
            iconId: 'test-ts',
            assetPath: 'assets/material_icon_theme/icons/test-ts.svg',
          ),
        );
        expect(diagnostics, isEmpty);
      },
    );
  });
}
