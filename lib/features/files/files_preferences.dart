import 'package:remya_client/remya_client.dart';

/// A validated object preference value together with Core's revision token.
typedef FilesPreferenceSnapshot = ({
  Map<String, dynamic> value,
  String revision,
});

/// Core-backed Files preferences for Remy'a Launcher.
class FilesPreferences {
  FilesPreferences({SettingsClient? client})
    : _preferences = AppPreferences(appId, client: client);

  static const appId = 'remya-launcher';
  static const searchId = 'files-search';
  static const historyId = 'files-history';
  static const previewId = 'files-preview';
  static const actionsId = 'files-actions';

  static const ids = [searchId, historyId, previewId, actionsId];

  static const fileProviders = ['memy', 'xbel'];
  static const directoryProviders = ['zoxide', 'memy', 'xbel'];
  static const externalWriteModes = [
    'read_only',
    'launcher',
    'delegate_editor',
  ];

  static const maxRoots = 64;
  static const maxExclusions = 256;
  static const maxRootLength = 4096;
  static const maxExclusionLength = 512;
  static const maxActionValueLength = 512;
  static const minBytesLimit = 1024;
  static const maxContentsBytes = 10485760;
  static const minResultLimit = 20;
  static const maxResultLimit = 500;
  static const maxRetentionDays = 3650;
  static const maxHistoryRecords = 10000;
  static const maxTextPreviewBytes = 134217728;
  static const maxImagePreviewBytes = 536870912;

  static const Map<String, Map<String, dynamic>> defaults = {
    searchId: {
      'roots': <String>['~'],
      'exclusions': <String>[],
      'respect_ignore': true,
      'show_hidden': false,
      'contents_enabled': true,
      'contents_max_bytes': 2097152,
      'result_limit': 200,
    },
    historyId: {
      'enabled': false,
      'record_local': true,
      'read_system_xbel': false,
      'external_enabled': false,
      'file_provider': 'memy',
      'directory_provider': 'zoxide',
      'external_write_mode': 'read_only',
      'retention_days': 90,
      'max_records_per_kind': 10000,
    },
    previewId: {
      'show_preview': true,
      'text_max_bytes': 10485760,
      'image_max_bytes': 52428800,
    },
    actionsId: {'editor': '', 'terminal': '', 'file_manager': ''},
  };

  static final _controlChars = RegExp(r'[\x00-\x1f\x7f-\x9f]');
  static final _localPathPattern = RegExp(r'^(/.*|~(/.*)?)$');
  static final _actionTargetPattern = RegExp(
    r'^(?:/[^;|&$`<>]*[^\s/;|&$`<>]|[^\s/;|&$`<>]+)?$',
  );

  final AppPreferences _preferences;

  /// Reads all Files preference sections, returning conservative in-memory
  /// defaults for any section that fails to read or validate without writing.
  Future<Map<String, Map<String, dynamic>>> read() async {
    final sections = await Future.wait([
      for (final id in ids) _readSectionOrDefault(id),
    ]);
    return {for (var i = 0; i < ids.length; i++) ids[i]: sections[i]};
  }

  /// Reads a single Files preference section and its revision from Core.
  Future<FilesPreferenceSnapshot> readSection(String id) async {
    _requireKnownId(id);
    return _parseReply(id, await _preferences.get(id), requireRevision: true);
  }

  /// Watches Core change notifications for a single Files preference section.
  Stream<Map<String, dynamic>> watch(String id) {
    _requireKnownId(id);
    return _preferences.watch(id);
  }

  /// Updates [key] in section [id] using an AppPreferences.get baseline and
  /// Compare-And-Swap Replace with its revision (never overwriting conflicts).
  Future<void> setValue(String id, String key, Object value) async {
    validateField(id, key, value);
    final baseline = await readSection(id);
    await replaceSectionValue(
      id,
      key,
      value,
      revision: baseline.revision,
      current: baseline.value,
    );
  }

  /// Replaces [key] in section [id] against the caller-captured [revision].
  Future<FilesPreferenceSnapshot> replaceSectionValue(
    String id,
    String key,
    Object value, {
    required String revision,
    required Map<String, dynamic> current,
  }) async {
    validateField(id, key, value);
    if (revision.isEmpty) {
      throw const FormatException('Missing preference revision.');
    }
    final updated = copySection(current)..[key] = _copyValue(value);
    validateSection(id, updated);
    final reply = await _preferences.replace(id, updated, revision);
    return _parseReply(id, reply, requireRevision: true);
  }

  Future<void> close() => _preferences.close();

  Future<Map<String, dynamic>> _readSectionOrDefault(String id) async {
    try {
      final reply = await _preferences.get(id);
      return _parseReply(id, reply, requireRevision: false).value;
    } on Object {
      return copySection(defaults[id]!);
    }
  }

  static void _requireKnownId(String id) {
    if (!ids.contains(id)) {
      throw ArgumentError.value(id, 'id', 'Unknown Files preference id');
    }
  }

  static FilesPreferenceSnapshot _parseReply(
    String id,
    Map<String, dynamic> reply, {
    required bool requireRevision,
  }) {
    final value = reply['value'];
    if (reply['id'] != '$appId.$id' || reply['availability'] != 'available') {
      throw FormatException('Remy\'a Core returned an invalid $id value.');
    }
    validateSection(id, value);
    final revision = reply['revision'];
    if (requireRevision && (revision is! String || revision.isEmpty)) {
      throw FormatException('Remy\'a Core did not return a revision for $id.');
    }
    return (
      value: copySection(value as Map),
      revision: revision is String ? revision : '',
    );
  }

  /// Validates a complete object value against the schema for [id].
  static void validateSection(String id, Object? value) {
    _requireKnownId(id);
    final schemaDefaults = defaults[id]!;
    if (value is! Map ||
        value.length != schemaDefaults.length ||
        !schemaDefaults.keys.every(value.containsKey)) {
      throw FormatException('Remy\'a Core returned an invalid $id value.');
    }
    for (final key in schemaDefaults.keys) {
      validateField(id, key, value[key]);
    }
  }

  /// Validates one [key]/[value] pair within section [id].
  static void validateField(String id, String key, Object? value) {
    _requireKnownId(id);
    final valid = switch ((id, key)) {
      (searchId, 'roots') => _isValidStringList(
        value,
        minItems: 1,
        maxItems: maxRoots,
        itemValid: isValidRootPath,
      ),
      (searchId, 'exclusions') => _isValidStringList(
        value,
        minItems: 0,
        maxItems: maxExclusions,
        itemValid: isValidExclusion,
      ),
      (searchId, 'respect_ignore' || 'show_hidden' || 'contents_enabled') =>
        value is bool,
      (searchId, 'contents_max_bytes') => _isIntInRange(
        value,
        minBytesLimit,
        maxContentsBytes,
      ),
      (searchId, 'result_limit') => _isIntInRange(
        value,
        minResultLimit,
        maxResultLimit,
      ),
      (
        historyId,
        'enabled' || 'record_local' || 'read_system_xbel' || 'external_enabled',
      ) =>
        value is bool,
      (historyId, 'file_provider') => fileProviders.contains(value),
      (historyId, 'directory_provider') => directoryProviders.contains(value),
      (historyId, 'external_write_mode') => externalWriteModes.contains(value),
      (historyId, 'retention_days') => _isIntInRange(
        value,
        1,
        maxRetentionDays,
      ),
      (historyId, 'max_records_per_kind') => _isIntInRange(
        value,
        1,
        maxHistoryRecords,
      ),
      (previewId, 'show_preview') => value is bool,
      (previewId, 'text_max_bytes') => _isIntInRange(
        value,
        minBytesLimit,
        maxTextPreviewBytes,
      ),
      (previewId, 'image_max_bytes') => _isIntInRange(
        value,
        minBytesLimit,
        maxImagePreviewBytes,
      ),
      (actionsId, 'editor' || 'terminal' || 'file_manager') =>
        isValidActionTarget(value),
      _ => throw ArgumentError.value(key, 'key', 'Unknown key for $id'),
    };
    if (!valid) {
      throw FormatException('Invalid value for $id.$key.');
    }
  }

  static bool isValidRootPath(Object? value) =>
      value is String &&
      value.isNotEmpty &&
      value.length <= maxRootLength &&
      !_controlChars.hasMatch(value) &&
      _localPathPattern.hasMatch(value);

  static bool isValidExclusion(Object? value) =>
      value is String &&
      value.isNotEmpty &&
      value.length <= maxExclusionLength &&
      !_controlChars.hasMatch(value) &&
      _localPathPattern.hasMatch(value);

  static bool isValidActionTarget(Object? value) =>
      value is String &&
      value.length <= maxActionValueLength &&
      !_controlChars.hasMatch(value) &&
      _actionTargetPattern.hasMatch(value);

  static bool _isIntInRange(Object? value, int min, int max) =>
      value is int && value >= min && value <= max;

  static bool _isValidStringList(
    Object? value, {
    required int minItems,
    required int maxItems,
    required bool Function(Object?) itemValid,
  }) =>
      value is List &&
      value.length >= minItems &&
      value.length <= maxItems &&
      value.every(itemValid) &&
      value.cast<String>().toSet().length == value.length;

  /// Deep-copies a validated Files preference section so callers cannot mutate
  /// [defaults] or stored maps.
  static Map<String, dynamic> copySection(Map<dynamic, dynamic> source) => {
    for (final entry in source.entries)
      entry.key as String: _copyValue(entry.value),
  };

  static dynamic _copyValue(Object? value) => switch (value) {
    final List list => list.cast<String>().toList(),
    _ => value,
  };
}
