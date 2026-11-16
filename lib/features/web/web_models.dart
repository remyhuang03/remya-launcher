List<T> _list<T>(Object? value, T Function(Map<String, dynamic>) decode) =>
    value == null
    ? const []
    : (value as List)
          .cast<Map<String, dynamic>>()
          .map(decode)
          .toList(growable: false);

class WebSource {
  const WebSource({required this.browserId, required this.profileId});

  WebSource.fromJson(Map<String, dynamic> json)
    : browserId = json['browserId'] as String,
      profileId = json['profileId'] as String;

  final String browserId, profileId;
}

class WebSourceSummary {
  const WebSourceSummary({
    required this.browserId,
    required this.label,
    required this.count,
  });

  WebSourceSummary.fromJson(Map<String, dynamic> json)
    : browserId = json['browserId'] as String,
      label = json['label'] as String,
      count = json['count'] as int;

  final String browserId, label;
  final int count;
}

class WebFavicon {
  const WebFavicon({
    this.status = 'monogram',
    this.iconKey,
    this.fallbackText = '',
  });

  WebFavicon.fromJson(Map<String, dynamic> json)
    : status = json['status'] as String? ?? 'monogram',
      iconKey = json['iconKey'] as String?,
      fallbackText = json['fallbackText'] as String? ?? '';

  final String status, fallbackText;
  final String? iconKey;
}

class WebResult {
  const WebResult({
    this.entryId,
    required this.title,
    required this.url,
    this.hostname = '',
    this.origin,
    this.folderPath = const [],
    this.sources = const [],
    this.visitedCount = 0,
    this.lastVisitedMs,
    this.favicon = const WebFavicon(),
    this.engineId,
  });

  factory WebResult.fromJson(Map<String, dynamic> json) {
    final url = (json['url'] ?? json['representativeUrl']) as String;
    return WebResult(
      entryId: json['entryId'] as String?,
      title: json['title'] as String,
      url: url,
      hostname: json['hostname'] as String? ?? Uri.tryParse(url)?.host ?? '',
      origin: json['origin'] as String?,
      folderPath: (json['folderPath'] as List?)?.cast<String>() ?? const [],
      sources: _list(json['sources'], WebSource.fromJson),
      visitedCount: json['visitedCount'] as int? ?? 0,
      lastVisitedMs: json['lastVisitedMs'] as int?,
      favicon: WebFavicon.fromJson(
        json['favicon'] as Map<String, dynamic>? ?? const {},
      ),
      engineId: json['engineId'] as String?,
    );
  }

  final String? entryId, origin, engineId;
  final String title, url, hostname;
  final List<String> folderPath;
  final List<WebSource> sources;
  final int visitedCount;
  final int? lastVisitedMs;
  final WebFavicon favicon;
}

class WebGroup {
  const WebGroup({required this.kind, this.items = const []});

  WebGroup.fromJson(Map<String, dynamic> json)
    : kind = json['kind'] as String,
      items = _list(json['items'], WebResult.fromJson);

  final String kind;
  final List<WebResult> items;
}

class WebCompletion {
  const WebCompletion({required this.suggestedUrl, this.sourceEntryId});

  WebCompletion.fromJson(Map<String, dynamic> json)
    : suggestedUrl = json['suggestedUrl'] as String,
      sourceEntryId = json['sourceEntryId'] as String?;

  final String suggestedUrl;
  final String? sourceEntryId;
}

class WebInput {
  const WebInput({
    this.kind = 'search',
    this.directNavigationTarget,
    this.completion,
    this.engineId,
    this.query,
  });

  WebInput.fromJson(Map<String, dynamic> json)
    : kind = json['kind'] as String? ?? 'search',
      directNavigationTarget = json['directNavigationTarget'] as String?,
      completion = switch (json['completion']) {
        final Map<String, dynamic> value => WebCompletion.fromJson(value),
        _ => null,
      },
      engineId = json['engineId'] as String?,
      query = json['query'] as String?;

  final String kind;
  final String? directNavigationTarget, engineId, query;
  final WebCompletion? completion;
}

class WebSearchResult {
  const WebSearchResult({
    this.matchedCount = 0,
    this.sourceSummary = const [],
    this.groups = const [],
    this.frequentSites = const [],
    this.recent = const [],
    this.input = const WebInput(),
    this.stale = false,
    this.enabledProfileCount,
  });

  WebSearchResult.fromJson(Map<String, dynamic> json)
    : matchedCount = json['matchedCount'] as int? ?? 0,
      sourceSummary = _list(json['sourceSummary'], WebSourceSummary.fromJson),
      groups = _list(json['groups'], WebGroup.fromJson),
      frequentSites = _list(json['frequentSites'], WebResult.fromJson),
      recent = _list(json['recent'], WebResult.fromJson),
      input = WebInput.fromJson(
        json['input'] as Map<String, dynamic>? ?? const {},
      ),
      stale = (json['sync'] as Map<String, dynamic>?)?['stale'] == true,
      enabledProfileCount =
          (json['sync'] as Map<String, dynamic>?)?['enabledProfileCount']
              as int?;

  final int matchedCount;
  final List<WebSourceSummary> sourceSummary;
  final List<WebGroup> groups;
  final List<WebResult> frequentSites, recent;
  final WebInput input;
  final bool stale;
  final int? enabledProfileCount;
}

class WebEngine {
  const WebEngine({
    required this.engineId,
    required this.label,
    required this.keyword,
    required this.hostname,
  });

  WebEngine.fromJson(Map<String, dynamic> json)
    : engineId = json['engineId'] as String,
      label = json['label'] as String,
      keyword = json['keyword'] as String,
      hostname = json['hostname'] as String;

  final String engineId, label, keyword, hostname;
}

class WebProfile {
  const WebProfile({
    required this.profileId,
    required this.browserId,
    required this.browserLabel,
    required this.label,
    this.enabled = false,
    this.status = '',
    this.errorCode,
    this.lastGoodSyncMs,
  });

  WebProfile.fromJson(Map<String, dynamic> json)
    : profileId = json['profileId'] as String,
      browserId = json['browserId'] as String,
      browserLabel = json['browserLabel'] as String,
      label = json['label'] as String,
      enabled = json['enabled'] == true,
      status = json['status'] as String? ?? '',
      errorCode = json['errorCode'] as String?,
      lastGoodSyncMs = json['lastGoodSyncMs'] as int?;

  final String profileId, browserId, browserLabel, label, status;
  final bool enabled;
  final int? lastGoodSyncMs;
  final String? errorCode;
}

class WebSyncStatus {
  const WebSyncStatus({this.syncing = false, this.profiles = const []});

  WebSyncStatus.fromJson(Map<String, dynamic> json)
    : syncing = json['syncing'] == true,
      profiles = _list(json['profiles'], WebProfile.fromJson);

  final bool syncing;
  final List<WebProfile> profiles;
}

class WebAction {
  const WebAction({
    required this.action,
    required this.label,
    this.enabled = true,
    this.browserId,
    this.reason,
  });

  WebAction.fromJson(Map<String, dynamic> json)
    : action = json['action'] as String,
      label = json['label'] as String,
      enabled = json['enabled'] == true,
      browserId = json['browserId'] as String?,
      reason = json['reason'] as String?;

  final String action, label;
  final bool enabled;
  final String? browserId, reason;
}

class WebBrowser {
  const WebBrowser({
    required this.browserId,
    required this.label,
    this.canOpen = false,
    this.canOpenPrivate = false,
  });

  WebBrowser.fromJson(Map<String, dynamic> json)
    : browserId = json['browserId'] as String,
      label = json['label'] as String,
      canOpen = json['canOpen'] == true,
      canOpenPrivate = json['canOpenPrivate'] == true;

  final String browserId, label;
  final bool canOpen, canOpenPrivate;
}

class WebCapabilities {
  const WebCapabilities({
    this.localIndex = false,
    this.networkIcons = false,
    this.nativeHistory = false,
    this.defaultPrivate = false,
    this.browsers = const [],
  });

  WebCapabilities.fromJson(Map<String, dynamic> json)
    : localIndex = json['localIndex'] == true,
      networkIcons = json['networkIcons'] == true,
      nativeHistory = json['nativeHistory'] == true,
      defaultPrivate = json['defaultPrivate'] == true,
      browsers = _list(json['browsers'], WebBrowser.fromJson);

  final bool localIndex, networkIcons, nativeHistory, defaultPrivate;
  final List<WebBrowser> browsers;
}
