enum FilesSearchMode { empty, names, path, contents }

class FileEntry {
  FileEntry.fromJson(
    Map<String, dynamic> json, {
    bool contentFile = false,
    Map<String, dynamic>? hit,
  }) : id = json['id'] as String,
       name = json['name'] as String,
       path = json['path'] as String,
       displayPath = json['displayPath'] as String?,
       parent = json['parent'] as String?,
       kind = json['kind'] as String,
       group = json['group'] as String? ?? 'results',
       completion = json['completion'] as String?,
       role = json['role'] as String?,
       size = json['size'] as int?,
       modified = json['modified'] as int?,
       version = json['version'],
       executable = json['executable'] == true,
       pinned = json['pinned'] == true || json['group'] == 'pinned',
       line = (contentFile ? hit : json)?['line'] as int?,
       snippet = (contentFile ? hit : json)?['snippet'] as String?,
       hitCount = json['hitCount'] as int?;

  final String id, name, path, kind, group;
  final String? displayPath, parent, completion, role, snippet;
  final int? size, modified, line, hitCount;
  final Object? version;
  final bool executable, pinned;

  bool get isDirectory => kind == 'directory';
  bool get isPlace => group == 'places';
  String get title =>
      '$name${isDirectory && !isPlace && !name.endsWith('/') ? '/' : ''}';
  String get location => parent ?? _parentOf(displayPath ?? path);
  String get key => '$group:$id:${line ?? 0}';

  static String _parentOf(String path) {
    final slash = path.lastIndexOf('/');
    return slash <= 0 ? '/' : path.substring(0, slash);
  }
}

class FilesSearchResult {
  FilesSearchResult.fromJson(Map<String, dynamic> json)
    : mode = FilesSearchMode.values.byName(json['mode'] as String),
      entries = _entries(json),
      partial = json['partial'] == true,
      directoryId = json['directoryId'] as String?,
      directory = json['directory'] as String?,
      directoryVersion = json['directoryVersion'] as String?,
      createName = json['createName'] as String?,
      createPath = json['createPath'] as String?,
      createRelative = json['createRelative'] as String?,
      createDirectoryOnly = json['createDirectoryOnly'] == true,
      createWritable = json['createWritable'] != false,
      createAncestors = (json['createAncestors'] as List<dynamic>? ?? const [])
          .cast<String>(),
      diagnostic = json['diagnostic'] as String?,
      elapsedMs = json['elapsedMs'] as int?,
      matchedCount = json['matchedCount'] as int?,
      totalCount = json['totalCount'] as int?,
      hitCount = json['hitCount'] as int?,
      matchQuery = json['matchQuery'] as String?;

  static List<FileEntry> _entries(Map<String, dynamic> json) {
    final entries = (json['entries'] as List<dynamic>)
        .cast<Map<String, dynamic>>();
    final contents = json['mode'] == 'contents';
    return [
      for (final entry in entries) ...[
        FileEntry.fromJson(entry, contentFile: contents),
        if (contents)
          for (final hit in (entry['hits'] as List<dynamic>? ?? const []))
            FileEntry.fromJson(
              entry,
              contentFile: true,
              hit: hit as Map<String, dynamic>,
            ),
      ],
    ];
  }

  final FilesSearchMode mode;
  final List<FileEntry> entries;
  final bool partial, createDirectoryOnly, createWritable;
  final String? directoryId,
      directory,
      directoryVersion,
      createName,
      createPath,
      createRelative,
      diagnostic,
      matchQuery;
  final List<String> createAncestors;
  final int? elapsedMs, matchedCount, totalCount, hitCount;
}

enum FileChoiceKind {
  entry,
  createFile,
  createDirectory,
  contents,
  filter,
  more,
}

class FileChoice {
  const FileChoice.entry(FileEntry this.entry)
    : kind = .entry,
      label = '',
      detail = '';
  const FileChoice(this.kind, this.label, {this.detail = '', this.entry});

  final FileChoiceKind kind;
  final FileEntry? entry;
  final String label, detail;

  bool get isCreate => kind == .createFile || kind == .createDirectory;

  String get key => switch (kind) {
    .entry => entry!.key,
    _ => '${kind.name}:${entry?.id ?? label}',
  };
}

class FileGroup {
  const FileGroup(this.title, this.choices, {this.hint, this.tiles = false});

  final String title;
  final List<FileChoice> choices;
  final String? hint;
  final bool tiles;
}
