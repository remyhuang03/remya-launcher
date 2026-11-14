import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

/// Signature for spawning the calculator sidecar process.
typedef CalculatorProcessStarter = Future<Process> Function(
  String executable,
  List<String> arguments, {
  Map<String, String>? environment,
  bool includeParentEnvironment,
  bool runInShell,
  ProcessStartMode mode,
});

/// Lifecycle state of [CalculatorClient].
enum CalculatorClientState {
  stopped,
  starting,
  handshaking,
  ready,
  restarting,
  disabled,
  closed,
}

/// Transport-level failure categories raised by [CalculatorClient].
enum CalculatorTransportErrorCode {
  binaryUnavailable,
  invalidExecutable,
  spawnFailed,
  handshakeFailed,
  unsupportedProtocol,
  protocolError,
  frameTooLarge,
  timeout,
  processCrashed,
  disabled,
  closed,
}

/// Exception thrown when the calculator NDJSON stdio transport fails.
class CalculatorTransportException implements Exception {
  const CalculatorTransportException(this.code, this.message, {this.cause});

  final CalculatorTransportErrorCode code;
  final String message;
  final Object? cause;

  @override
  String toString() => 'CalculatorTransportException(${code.name}): $message';
}

/// Asynchronous NDJSON stdio transport client for the calculator sidecar.
class CalculatorClient {
  CalculatorClient({
    this._executablePath,
    this._executableResolver,
    this._launcherExecutablePath,
    this._processStarter,
    Map<String, String>? environment,
    this._handshakeTimeout = const Duration(seconds: 2),
    this._requestTimeout = const Duration(seconds: 1),
    this._shutdownTimeout = const Duration(milliseconds: 500),
    this._initialBackoff = const Duration(milliseconds: 50),
    this._maxBackoff = const Duration(seconds: 2),
    int maxConsecutiveFailures = 5,
    int maxFrameBytes = defaultMaxFrameBytes,
    DateTime Function()? clock,
    this._sleeper,
  }) : _environmentOverride = environment,
       _maxConsecutiveFailures = maxConsecutiveFailures,
       _maxFrameBytes = maxFrameBytes,
       _clock = clock ?? DateTime.now {
    if (maxConsecutiveFailures < 1) {
      throw ArgumentError.value(
        maxConsecutiveFailures,
        'maxConsecutiveFailures',
        'Must be at least 1.',
      );
    }
    if (maxFrameBytes < 128) {
      throw ArgumentError.value(
        maxFrameBytes,
        'maxFrameBytes',
        'Must be at least 128 bytes.',
      );
    }
  }

  /// Protocol major version supported by this transport.
  static const int protocolVersion = 1;

  /// Expected executable basename of the bundled calculator sidecar.
  static const String sidecarBinaryName = 'remya-launcher-calculator';

  /// Default per-frame wire size limit in bytes (64 KiB).
  static const int defaultMaxFrameBytes = 64 * 1024;

  static const Set<String> _retrySafeOps = {
    'preview',
    'catalogSearch',
    'complete',
    'historyList',
  };

  static const List<String> _allowedAbsolutePathEnvKeys = [
    'HOME',
    'XDG_STATE_HOME',
    'XDG_DATA_HOME',
    'XDG_CONFIG_HOME',
    'XDG_CACHE_HOME',
    'XDG_RUNTIME_DIR',
    'TZDIR',
  ];

  static const List<String> _allowedPathListEnvKeys = [
    'XDG_DATA_DIRS',
    'XDG_CONFIG_DIRS',
  ];

  final String? _executablePath;
  final Future<String> Function()? _executableResolver;
  final String? _launcherExecutablePath;
  final CalculatorProcessStarter? _processStarter;
  final Map<String, String>? _environmentOverride;
  final Duration _handshakeTimeout;
  final Duration _requestTimeout;
  final Duration _shutdownTimeout;
  final Duration _initialBackoff;
  final Duration _maxBackoff;
  final int _maxConsecutiveFailures;
  final int _maxFrameBytes;
  final DateTime Function() _clock;
  final Future<void> Function(Duration)? _sleeper;

  final Completer<void> _closedCompleter = Completer<void>();

  CalculatorClientState _state = .stopped;
  _ActiveProcess? _active;
  Future<void>? _startFuture;
  Future<void>? _closeFuture;
  Map<String, Object?>? _lastHelloEnvelope;
  CalculatorTransportException? _permanentError;

  bool _closed = false;
  int _nextIdCounter = 0;
  int _requestSequence = 0;
  int _latestPreviewSequence = 0;
  int _latestCompleteSequence = 0;
  int _latestCatalogSequence = 0;

  int _consecutiveFailures = 0;
  DateTime? _nextRetryAllowedAt;
  int _crashCount = 0;
  int _restartCount = 0;
  int _decodeErrorCount = 0;
  bool _hasStartedOnce = false;

  /// Current transport lifecycle state.
  CalculatorClientState get state => _state;

  /// Total unexpected sidecar crashes or protocol failures observed.
  int get crashCount => _crashCount;

  /// Total sidecar restarts initiated after an initial launch.
  int get restartCount => _restartCount;

  /// Total invalid stdout frames or UTF-8 decode failures observed.
  int get decodeErrorCount => _decodeErrorCount;

  /// Handshake `data` payload from the active or most recent `hello` response.
  Map<String, Object?>? get handshakeData =>
      switch (_lastHelloEnvelope?['data']) {
        final Map<String, Object?> data => Map<String, Object?>.unmodifiable(
          data,
        ),
        _ => null,
      };

  /// Sends a single protocol request and resolves with the whole response envelope.
  ///
  /// Transport, framing, timeout, handshake, and lifecycle errors throw
  /// [CalculatorTransportException]. Protocol error responses (`ok: false`) are
  /// returned as envelopes without throwing.
  Future<Map<String, Object?>> request(
    String op, [
    Map<String, Object?> fields = const {},
  ]) async {
    _validateOp(op);
    _throwIfUnavailable();

    if (op == 'shutdown') {
      return _sendShutdownRequest(fields);
    }

    // Validate serializability and frame size before spawning a sidecar.
    _encodeRequestFrame('r0', op, fields);

    final seq = ++_requestSequence;
    if (op == 'preview') {
      _latestPreviewSequence = seq;
    } else if (op == 'complete') {
      _latestCompleteSequence = seq;
    } else if (op == 'catalogSearch') {
      _latestCatalogSequence = seq;
    }

    final isRetrySafe = _retrySafeOps.contains(op);
    var attemptsRemaining = isRetrySafe ? 1 : 0;

    while (true) {
      _throwIfUnavailable();

      final wasRunning = _active != null;
      await _ensureReady();
      _throwIfUnavailable();

      // If a cold-start request itself is `hello`, return the handshake envelope
      // directly instead of issuing a redundant second `hello`.
      if (op == 'hello' && !wasRunning && _lastHelloEnvelope != null) {
        return _lastHelloEnvelope!;
      }

      final active = _active;
      if (active == null) {
        throw const CalculatorTransportException(
          .processCrashed,
          'Calculator sidecar is unavailable.',
        );
      }

      try {
        final response = await _sendOnProcess(
          active,
          op,
          fields,
          sequence: seq,
          isRetrySafe: isRetrySafe,
          timeout: op == 'fxRefresh'
              ? const Duration(seconds: 12)
              : _requestTimeout,
        );
        _consecutiveFailures = 0;
        _nextRetryAllowedAt = null;
        return response;
      } on _RetryableCrash catch (signal) {
        if (!isRetrySafe ||
            attemptsRemaining <= 0 ||
            _closed ||
            _permanentError != null ||
            _isSuperseded(op, seq)) {
          throw signal.error;
        }
        attemptsRemaining--;
      }
    }
  }

  /// Permanently closes the transport, interrupting all pending requests.
  Future<void> close() {
    if (_closeFuture != null) {
      return _closeFuture!;
    }
    _closed = true;
    _state = .closed;
    if (!_closedCompleter.isCompleted) {
      _closedCompleter.complete();
    }

    const closedError = CalculatorTransportException(
      .closed,
      'CalculatorClient has been closed.',
    );
    final active = _active;
    if (active != null) {
      _failAllPending(active, closedError, allowRetrySignal: false);
    }

    _closeFuture = _performClose();
    return _closeFuture!;
  }

  Future<void> _performClose() async {
    final startFuture = _startFuture;
    if (startFuture != null) {
      try {
        await startFuture.timeout(_shutdownTimeout, onTimeout: () {});
      } on Object {
        // Startup errors during close are discarded; callers already fail closed.
      }
    }

    final active = _active;
    if (active == null) {
      return;
    }

    await _shutdownProcess(active, sendShutdownCommand: true);
    if (identical(_active, active)) {
      _active = null;
    }
  }

  Future<Map<String, Object?>> _sendShutdownRequest(
    Map<String, Object?> fields,
  ) async {
    final startFuture = _startFuture;
    if (startFuture != null) {
      await startFuture;
      _throwIfUnavailable();
    }

    final id = _allocateRequestId();
    final active = _active;
    if (active == null) {
      _state = .stopped;
      return <String, Object?>{
        'v': protocolVersion,
        'id': id,
        'ok': true,
        'data': <String, Object?>{'stopped': true},
      };
    }

    active.intentionalExit = true;
    final completer = Completer<Map<String, Object?>>();
    final pending = _PendingRequest(
      id: id,
      op: 'shutdown',
      revision: null,
      sequence: ++_requestSequence,
      isRetrySafe: false,
      completer: completer,
    );

    final encoded = _encodeRequestFrame(id, 'shutdown', fields);
    active.pending[id] = pending;
    pending.timer = Timer(_shutdownTimeout, () {
      final removed = active.pending.remove(id);
      if (removed != null && !removed.completer.isCompleted) {
        removed.completer.completeError(
          const CalculatorTransportException(
            .timeout,
            'Calculator sidecar shutdown timed out.',
          ),
        );
      }
    });

    _writeFrame(active, encoded);

    try {
      final response = await completer.future;
      await _closeStdin(active);
      await _finishProcessExit(active);
      return response;
    } on CalculatorTransportException catch (error) {
      // Sidecars may exit cleanly immediately on shutdown before flushing ack.
      if (error.code == .processCrashed && active.exitCodeValue == 0) {
        await _cleanupProcess(active);
        return <String, Object?>{
          'v': protocolVersion,
          'id': id,
          'ok': true,
          'data': <String, Object?>{'stopped': true},
        };
      }
      await _forceTerminateProcess(active);
      rethrow;
    } finally {
      if (identical(_active, active)) {
        _active = null;
      }
      if (!_closed && _permanentError == null) {
        _state = .stopped;
      }
    }
  }

  Future<void> _ensureReady() async {
    while (true) {
      _throwIfUnavailable();
      if (_active != null && _state == .ready) {
        return;
      }
      final existing = _startFuture;
      if (existing != null) {
        await Future.any([existing, _closedCompleter.future]);
        _throwIfUnavailable();
        if (_active != null && _state == .ready) {
          return;
        }
        continue;
      }

      final startCompleter = Completer<void>();
      final startFuture = startCompleter.future..ignore();
      _startFuture = startFuture;
      try {
        await _startAndHandshake();
        startCompleter.complete();
        return;
      } on Object catch (error, stackTrace) {
        startCompleter.completeError(error, stackTrace);
        rethrow;
      } finally {
        if (identical(_startFuture, startFuture)) {
          _startFuture = null;
        }
      }
    }
  }

  Future<void> _startAndHandshake() async {
    final retryAt = _nextRetryAllowedAt;
    if (retryAt != null) {
      final delay = retryAt.difference(_clock());
      if (delay > Duration.zero) {
        _state = .restarting;
        await _waitWithCancel(delay);
        _throwIfUnavailable();
      }
    }

    _state = _hasStartedOnce ? .restarting : .starting;
    if (_hasStartedOnce) {
      _restartCount++;
    }

    final executable = await _resolveExecutable();
    _throwIfUnavailable();

    final controlledEnv = _buildControlledEnvironment(
      _environmentOverride ?? Platform.environment,
    );

    final starter = _processStarter ?? Process.start;
    final Process process;
    try {
      process = await starter(
        executable,
        const [],
        environment: controlledEnv,
        includeParentEnvironment: false,
        runInShell: false,
        mode: .normal,
      );
    } on Object catch (error) {
      _recordFailure(
        CalculatorTransportException(
          .spawnFailed,
          'Failed to start calculator sidecar.',
          cause: error,
        ),
      );
    }

    final active = _ActiveProcess(process);
    // Prevent unhandled async pipe errors when the child closes stdin early.
    unawaited(process.stdin.done.catchError((Object _) {}));

    if (_closed) {
      active.intentionalExit = true;
      await _forceTerminateProcess(active);
      throw const CalculatorTransportException(
        .closed,
        'CalculatorClient has been closed.',
      );
    }

    _hasStartedOnce = true;
    _active = active;
    _attachProcessStreams(active);
    _state = .handshaking;

    try {
      final helloEnvelope = await _sendOnProcess(
        active,
        'hello',
        const {},
        sequence: 0,
        isRetrySafe: false,
        timeout: _handshakeTimeout,
      );
      _validateHandshakeResponse(helloEnvelope);
      _lastHelloEnvelope = helloEnvelope;
      if (!_closed) {
        _state = .ready;
      }
    } on CalculatorTransportException catch (error) {
      if (identical(_active, active)) {
        _active = null;
      }
      await _forceTerminateProcess(active);
      if (error.code == .unsupportedProtocol) {
        _permanentError = error;
        _state = .disabled;
        rethrow;
      }
      if (error.code == .closed) {
        rethrow;
      }
      final wrapped = error.code == .handshakeFailed
          ? error
          : CalculatorTransportException(
              .handshakeFailed,
              error.message,
              cause: error,
            );
      if (!active.brokenHandled) {
        _recordFailure(wrapped);
      }
      if (_permanentError != null) {
        throw _permanentError!;
      }
      throw wrapped;
    }
  }

  void _validateHandshakeResponse(Map<String, Object?> envelope) {
    if (envelope['v'] != protocolVersion) {
      throw CalculatorTransportException(
        .unsupportedProtocol,
        'Unsupported calculator protocol version: ${envelope['v']}.',
      );
    }
    if (envelope['ok'] != true) {
      final errorObj = envelope['error'];
      throw CalculatorTransportException(
        .handshakeFailed,
        'Calculator sidecar rejected hello handshake: $errorObj',
      );
    }
    final data = envelope['data'];
    if (data is! Map<String, Object?>) {
      throw const CalculatorTransportException(
        .unsupportedProtocol,
        'Calculator hello response is missing protocol metadata.',
      );
    }
    final negotiated = data['protocolVersion'];
    if (negotiated != protocolVersion) {
      throw CalculatorTransportException(
        .unsupportedProtocol,
        'Unsupported calculator protocolVersion: $negotiated.',
      );
    }
  }

  Future<Map<String, Object?>> _sendOnProcess(
    _ActiveProcess active,
    String op,
    Map<String, Object?> fields, {
    required int sequence,
    required bool isRetrySafe,
    required Duration timeout,
  }) {
    final id = _allocateRequestId();
    final revision = switch (fields['revision']) {
      final int value => value,
      _ => null,
    };

    final encoded = _encodeRequestFrame(id, op, fields);
    final completer = Completer<Map<String, Object?>>();
    final pending = _PendingRequest(
      id: id,
      op: op,
      revision: revision,
      sequence: sequence,
      isRetrySafe: isRetrySafe,
      completer: completer,
    );

    active.pending[id] = pending;
    pending.timer = Timer(timeout, () {
      _handleRequestTimeout(active, pending);
    });

    _writeFrame(active, encoded);
    return completer.future;
  }

  Uint8List _encodeRequestFrame(
    String id,
    String op,
    Map<String, Object?> fields,
  ) {
    final envelope = <String, Object?>{
      ...fields,
      'v': protocolVersion,
      'id': id,
      'op': op,
    };
    final String encodedJson;
    try {
      encodedJson = jsonEncode(envelope);
    } on Object catch (error) {
      throw CalculatorTransportException(
        .protocolError,
        'Failed to serialize calculator request.',
        cause: error,
      );
    }
    final bytes = utf8.encode('$encodedJson\n');
    if (bytes.length > _maxFrameBytes) {
      throw CalculatorTransportException(
        .frameTooLarge,
        'Request frame (${bytes.length} bytes) exceeds $_maxFrameBytes-byte limit.',
      );
    }
    return bytes;
  }

  void _writeFrame(_ActiveProcess active, Uint8List bytes) {
    if (active.exited || active.stdinClosed) {
      throw const CalculatorTransportException(
        .processCrashed,
        'Calculator sidecar stdin is closed.',
      );
    }
    try {
      active.process.stdin.add(bytes);
    } on Object catch (error) {
      throw CalculatorTransportException(
        .processCrashed,
        'Failed to write to calculator sidecar stdin.',
        cause: error,
      );
    }
  }

  void _attachProcessStreams(_ActiveProcess active) {
    active.stdoutSub = active.process.stdout.listen(
      (chunk) => _onStdoutChunk(active, chunk),
      onError: (Object error) => _onProcessBroken(
        active,
        CalculatorTransportException(
          .protocolError,
          'Error reading calculator sidecar stdout.',
          cause: error,
        ),
      ),
      onDone: () => _onStdoutDone(active),
      cancelOnError: true,
    );

    active.stderrSub = active.process.stderr.listen((chunk) {
      const maxStderrBytes = 4096;
      final remaining = maxStderrBytes - active.stderrBuffer.length;
      if (remaining <= 0) {
        return;
      }
      if (chunk.length <= remaining) {
        active.stderrBuffer.add(chunk);
      } else {
        active.stderrBuffer.add(chunk.sublist(0, remaining));
      }
    }, onError: (Object _) {});

    unawaited(
      active.process.exitCode.then((code) {
        active.exitCodeValue = code;
        active.exited = true;
        if (!active.exitCompleter.isCompleted) {
          active.exitCompleter.complete();
        }
        // Give buffered stdout pipe data one turn to drain before failing
        // remaining requests if stdout has not already closed.
        active.exitDrainTimer = Timer(const Duration(milliseconds: 25), () {
          if (!active.stdoutDone) {
            _onStdoutDone(active);
          }
        });
      }),
    );
  }

  void _onStdoutChunk(_ActiveProcess active, List<int> chunk) {
    if (active.stdoutDone || active.framingFailed) {
      return;
    }

    var start = 0;
    while (start < chunk.length) {
      final newline = chunk.indexOf(10, start);
      final end = newline < 0 ? chunk.length : newline;
      final segmentLength = end - start;
      final projected =
          active.stdoutBuffer.length + segmentLength + (newline >= 0 ? 1 : 0);
      if (projected > _maxFrameBytes) {
        active.stdoutBuffer.clear();
        active.framingFailed = true;
        _decodeErrorCount++;
        _onProcessBroken(
          active,
          CalculatorTransportException(
            .frameTooLarge,
            'Calculator sidecar stdout frame exceeded $_maxFrameBytes bytes.',
          ),
        );
        return;
      }

      if (segmentLength > 0) {
        active.stdoutBuffer.add(
          chunk is Uint8List
              ? Uint8List.sublistView(chunk, start, end)
              : chunk.sublist(start, end),
        );
      }

      if (newline < 0) {
        break;
      }

      final lineBytes = active.stdoutBuffer.takeBytes();
      _handleStdoutLine(active, lineBytes);
      if (active.framingFailed) {
        return;
      }
      start = end + 1;
    }
  }

  void _handleStdoutLine(_ActiveProcess active, Uint8List lineBytes) {
    final String line;
    try {
      line = utf8.decode(lineBytes, allowMalformed: false);
    } on FormatException catch (error) {
      active.framingFailed = true;
      _decodeErrorCount++;
      _onProcessBroken(
        active,
        CalculatorTransportException(
          .protocolError,
          'Calculator sidecar emitted invalid UTF-8 on stdout.',
          cause: error,
        ),
      );
      return;
    }

    final Object? decoded;
    try {
      decoded = jsonDecode(line);
    } on FormatException catch (error) {
      active.framingFailed = true;
      _decodeErrorCount++;
      _onProcessBroken(
        active,
        CalculatorTransportException(
          .protocolError,
          'Calculator sidecar emitted invalid JSON on stdout.',
          cause: error,
        ),
      );
      return;
    }

    if (decoded is! Map<String, Object?>) {
      active.framingFailed = true;
      _decodeErrorCount++;
      _onProcessBroken(
        active,
        const CalculatorTransportException(
          .protocolError,
          'Calculator sidecar stdout line must be a JSON object.',
        ),
      );
      return;
    }

    final version = decoded['v'];
    final id = decoded['id'];
    final ok = decoded['ok'];
    if (version != protocolVersion ||
        id is! String ||
        id.isEmpty ||
        ok is! bool) {
      active.framingFailed = true;
      _decodeErrorCount++;
      _onProcessBroken(
        active,
        const CalculatorTransportException(
          .protocolError,
          'Calculator sidecar emitted a malformed response envelope.',
        ),
      );
      return;
    }

    final pending = active.pending.remove(id);
    if (pending == null) {
      // Stale, duplicate, or out-of-window response ID: drop safely.
      return;
    }

    pending.timer?.cancel();
    pending.timer = null;

    if (pending.revision != null &&
        decoded.containsKey('revision') &&
        decoded['revision'] != pending.revision) {
      if (!pending.completer.isCompleted) {
        pending.completer.completeError(
          CalculatorTransportException(
            .protocolError,
            'Calculator response revision ${decoded['revision']} did not match request revision ${pending.revision}.',
          ),
        );
      }
      return;
    }

    if (!pending.completer.isCompleted) {
      pending.completer.complete(decoded);
    }
  }

  void _onStdoutDone(_ActiveProcess active) {
    if (active.stdoutDone) {
      return;
    }
    active.stdoutDone = true;

    final hasPartialLine = active.stdoutBuffer.isNotEmpty;
    active.stdoutBuffer.clear();

    if (active.intentionalExit || _closed) {
      _failAllPending(
        active,
        CalculatorTransportException(
          _closed ? .closed : .processCrashed,
          _closed
              ? 'CalculatorClient has been closed.'
              : 'Calculator sidecar terminated during shutdown.',
        ),
        allowRetrySignal: false,
      );
      return;
    }

    if (hasPartialLine && !active.framingFailed) {
      _decodeErrorCount++;
    }

    final error = hasPartialLine
        ? const CalculatorTransportException(
            .protocolError,
            'Calculator sidecar closed stdout with an incomplete NDJSON frame.',
          )
        : CalculatorTransportException(
            .processCrashed,
            'Calculator sidecar exited unexpectedly'
            '${active.exitCodeValue != null ? ' (code ${active.exitCodeValue})' : ''}.',
          );
    _onProcessBroken(active, error);
  }

  void _handleRequestTimeout(_ActiveProcess active, _PendingRequest pending) {
    final removed = active.pending.remove(pending.id);
    if (removed == null || removed.completer.isCompleted) {
      return;
    }

    const timeoutError = CalculatorTransportException(
      .timeout,
      'Calculator request timed out.',
    );

    // Do not retry the exact request that triggered a hard timeout; kill and
    // reset the sidecar so subsequent queries recover cleanly.
    removed.completer.completeError(timeoutError);
    _onProcessBroken(active, timeoutError);
  }

  void _onProcessBroken(
    _ActiveProcess active,
    CalculatorTransportException error,
  ) {
    if (active.brokenHandled) {
      return;
    }
    active.brokenHandled = true;

    if (identical(_active, active)) {
      _active = null;
    }

    if (!active.intentionalExit && !_closed) {
      _crashCount++;
      _applyFailureBackoff(error);
    }

    unawaited(_forceTerminateProcess(active));

    if (_closed) {
      _failAllPending(
        active,
        const CalculatorTransportException(
          .closed,
          'CalculatorClient has been closed.',
        ),
        allowRetrySignal: false,
      );
      return;
    }

    if (_permanentError != null) {
      _failAllPending(active, _permanentError!, allowRetrySignal: false);
      return;
    }

    _failAllPending(
      active,
      error,
      allowRetrySignal: error.code == .processCrashed,
    );
  }

  void _failAllPending(
    _ActiveProcess active,
    CalculatorTransportException error, {
    required bool allowRetrySignal,
  }) {
    final entries = active.pending.values.toList(growable: false);
    active.pending.clear();
    for (final pending in entries) {
      pending.timer?.cancel();
      pending.timer = null;
      if (pending.completer.isCompleted) {
        continue;
      }
      if (allowRetrySignal &&
          pending.isRetrySafe &&
          !_isSuperseded(pending.op, pending.sequence)) {
        pending.completer.completeError(_RetryableCrash(error));
      } else {
        pending.completer.completeError(error);
      }
    }
  }

  void _applyFailureBackoff(CalculatorTransportException error) {
    _consecutiveFailures++;
    if (_consecutiveFailures >= _maxConsecutiveFailures) {
      _permanentError = CalculatorTransportException(
        .disabled,
        'Calculator sidecar disabled after $_consecutiveFailures consecutive failures.',
        cause: error,
      );
      _state = .disabled;
      return;
    }

    if (!_closed) {
      _state = .stopped;
    }

    // Tolerate the first crash without backoff; apply capped exponential
    // backoff for repeated failures within the same session.
    if (_consecutiveFailures <= 1) {
      _nextRetryAllowedAt = null;
      return;
    }

    var delay = _initialBackoff;
    for (var i = 2; i < _consecutiveFailures; i++) {
      final doubled = delay * 2;
      delay = doubled > _maxBackoff ? _maxBackoff : doubled;
    }
    if (delay > _maxBackoff) {
      delay = _maxBackoff;
    }
    _nextRetryAllowedAt = _clock().add(delay);
  }

  Never _recordFailure(CalculatorTransportException error) {
    _crashCount++;
    _applyFailureBackoff(error);
    if (_permanentError != null) {
      throw _permanentError!;
    }
    throw error;
  }

  Future<void> _shutdownProcess(
    _ActiveProcess active, {
    required bool sendShutdownCommand,
  }) async {
    active.intentionalExit = true;

    if (sendShutdownCommand && !active.exited && !active.stdinClosed) {
      try {
        final id = _allocateRequestId();
        final bytes = _encodeRequestFrame(id, 'shutdown', const {});
        _writeFrame(active, bytes);
      } on Object {
        // Ignore write errors during shutdown; fall through to close/kill.
      }
    }

    await _closeStdin(active);
    await _finishProcessExit(active);
  }

  Future<void> _finishProcessExit(_ActiveProcess active) async {
    if (!active.exited) {
      await active.exitCompleter.future.timeout(
        _shutdownTimeout,
        onTimeout: () {},
      );
    }
    if (!active.exited) {
      active.process.kill(.sigterm);
      await active.exitCompleter.future.timeout(
        const Duration(milliseconds: 150),
        onTimeout: () {},
      );
    }
    if (!active.exited) {
      active.process.kill(.sigkill);
      await active.exitCompleter.future.timeout(
        const Duration(milliseconds: 100),
        onTimeout: () {},
      );
    }
    await _cleanupProcess(active);
  }

  Future<void> _forceTerminateProcess(_ActiveProcess active) async {
    active.intentionalExit = true;
    if (!active.exited) {
      active.process.kill(.sigkill);
    }
    await _closeStdin(active);
    if (!active.exited) {
      await active.exitCompleter.future.timeout(
        const Duration(milliseconds: 100),
        onTimeout: () {},
      );
    }
    await _cleanupProcess(active);
  }

  Future<void> _closeStdin(_ActiveProcess active) async {
    if (active.stdinClosed) {
      return;
    }
    active.stdinClosed = true;
    try {
      await active.process.stdin.close().timeout(
        const Duration(milliseconds: 100),
        onTimeout: () {},
      );
    } on Object {
      // Stdin may already be broken/closed if the sidecar exited.
    }
  }

  Future<void> _cleanupProcess(_ActiveProcess active) async {
    active.stdoutDone = true;
    active.exitDrainTimer?.cancel();
    active.exitDrainTimer = null;
    await active.stdoutSub?.cancel();
    active.stdoutSub = null;
    await active.stderrSub?.cancel();
    active.stderrSub = null;
  }

  Future<void> _waitWithCancel(Duration delay) async {
    if (_sleeper != null) {
      await Future.any([_sleeper(delay), _closedCompleter.future]);
      return;
    }
    final completer = Completer<void>();
    final timer = Timer(delay, () {
      if (!completer.isCompleted) {
        completer.complete();
      }
    });
    try {
      await Future.any([completer.future, _closedCompleter.future]);
    } finally {
      timer.cancel();
    }
  }

  Future<String> _resolveExecutable() async {
    if (_executableResolver != null) {
      final path = await _executableResolver();
      return _processStarter == null ? _verifyExecutableFile(path) : path;
    }
    if (_executablePath != null) {
      return _processStarter == null
          ? _verifyExecutableFile(_executablePath)
          : _executablePath;
    }
    if (_processStarter != null && _launcherExecutablePath == null) {
      return sidecarBinaryName;
    }
    return _resolveSiblingExecutable(
      _launcherExecutablePath ?? Platform.resolvedExecutable,
    );
  }

  Future<String> _resolveSiblingExecutable(String launcherPath) async {
    final String canonicalLauncher;
    try {
      canonicalLauncher = await File(launcherPath).resolveSymbolicLinks();
    } on FileSystemException catch (error) {
      throw CalculatorTransportException(
        .binaryUnavailable,
        'Launcher executable path could not be resolved.',
        cause: error,
      );
    }

    final String canonicalBundleDir;
    try {
      canonicalBundleDir = await File(canonicalLauncher).parent
          .resolveSymbolicLinks();
    } on FileSystemException catch (error) {
      throw CalculatorTransportException(
        .binaryUnavailable,
        'Launcher bundle directory could not be resolved.',
        cause: error,
      );
    }

    final sep = Platform.pathSeparator;
    final candidatePath = canonicalBundleDir == sep
        ? '$sep$sidecarBinaryName'
        : '$canonicalBundleDir$sep$sidecarBinaryName';

    final linkType = await FileSystemEntity.type(
      candidatePath,
      followLinks: false,
    );
    if (linkType == .notFound) {
      throw CalculatorTransportException(
        .binaryUnavailable,
        'Calculator sidecar not found at $candidatePath.',
      );
    }

    final String canonicalSidecar;
    try {
      canonicalSidecar = await File(candidatePath).resolveSymbolicLinks();
    } on FileSystemException catch (error) {
      throw CalculatorTransportException(
        .binaryUnavailable,
        'Calculator sidecar symlink could not be resolved.',
        cause: error,
      );
    }

    if (_basename(canonicalSidecar) != sidecarBinaryName) {
      throw CalculatorTransportException(
        .invalidExecutable,
        'Calculator sidecar resolved to unexpected executable name.',
      );
    }

    if (!_isWithinDirectory(canonicalSidecar, canonicalBundleDir)) {
      throw CalculatorTransportException(
        .invalidExecutable,
        'Calculator sidecar resolved outside the application bundle.',
      );
    }

    return _verifyExecutableFile(canonicalSidecar, alreadyCanonical: true);
  }

  Future<String> _verifyExecutableFile(
    String path, {
    bool alreadyCanonical = false,
  }) async {
    final String canonicalPath;
    if (alreadyCanonical) {
      canonicalPath = path;
    } else {
      final entityType = await FileSystemEntity.type(path, followLinks: false);
      if (entityType == .notFound) {
        throw CalculatorTransportException(
          .binaryUnavailable,
          'Calculator sidecar not found at $path.',
        );
      }
      try {
        canonicalPath = await File(path).resolveSymbolicLinks();
      } on FileSystemException catch (error) {
        throw CalculatorTransportException(
          .binaryUnavailable,
          'Calculator sidecar path could not be resolved.',
          cause: error,
        );
      }
    }

    final resolvedType = await FileSystemEntity.type(
      canonicalPath,
      followLinks: false,
    );
    if (resolvedType != .file) {
      throw const CalculatorTransportException(
        .invalidExecutable,
        'Calculator sidecar must be a regular file.',
      );
    }

    final stat = await FileStat.stat(canonicalPath);
    if (stat.type != .file) {
      throw const CalculatorTransportException(
        .invalidExecutable,
        'Calculator sidecar must be a regular file.',
      );
    }

    if ((stat.mode & 0x49) == 0) {
      throw const CalculatorTransportException(
        .invalidExecutable,
        'Calculator sidecar does not have executable permission.',
      );
    }

    return canonicalPath;
  }

  static Map<String, String> _buildControlledEnvironment(
    Map<String, String> source,
  ) {
    final env = <String, String>{};
    for (final key in _allowedAbsolutePathEnvKeys) {
      final value = source[key];
      if (value != null && _isValidAbsolutePathEnv(value)) {
        env[key] = value;
      }
    }
    for (final key in _allowedPathListEnvKeys) {
      final value = source[key];
      if (value != null) {
        final filtered = _filterAbsolutePathList(value);
        if (filtered != null) {
          env[key] = filtered;
        }
      }
    }
    final tz = source['TZ'];
    if (tz != null && _isValidTimezoneEnv(tz)) {
      env['TZ'] = tz;
    }
    return env;
  }

  static bool _isValidAbsolutePathEnv(String value) =>
      value.isNotEmpty &&
      value.startsWith('/') &&
      !value.contains('\x00') &&
      !value.contains('\r') &&
      !value.contains('\n');

  static String? _filterAbsolutePathList(String value) {
    if (value.isEmpty ||
        value.contains('\x00') ||
        value.contains('\r') ||
        value.contains('\n')) {
      return null;
    }
    final valid = value
        .split(':')
        .where(_isValidAbsolutePathEnv)
        .toList(growable: false);
    return valid.isEmpty ? null : valid.join(':');
  }

  static bool _isValidTimezoneEnv(String value) =>
      value.isNotEmpty &&
      value.length <= 256 &&
      !value.contains('\x00') &&
      !value.contains('\r') &&
      !value.contains('\n');

  static String _basename(String path) {
    final sep = Platform.pathSeparator;
    final index = path.lastIndexOf(sep);
    return index < 0 ? path : path.substring(index + 1);
  }

  static bool _isWithinDirectory(String path, String directory) {
    final sep = Platform.pathSeparator;
    if (directory == sep) {
      return path.startsWith(sep);
    }
    final normalizedDir = directory.endsWith(sep)
        ? directory.substring(0, directory.length - 1)
        : directory;
    return path.startsWith('$normalizedDir$sep');
  }

  bool _isSuperseded(String op, int sequence) => switch (op) {
    'preview' => sequence < _latestPreviewSequence,
    'complete' => sequence < _latestCompleteSequence,
    'catalogSearch' => sequence < _latestCatalogSequence,
    _ => false,
  };

  void _validateOp(String op) {
    if (op.isEmpty) {
      throw const CalculatorTransportException(
        .protocolError,
        'Request op must not be empty.',
      );
    }
    for (final codeUnit in op.codeUnits) {
      if (codeUnit < 0x20 || codeUnit == 0x7F) {
        throw const CalculatorTransportException(
          .protocolError,
          'Request op contains invalid control characters.',
        );
      }
    }
  }

  void _throwIfUnavailable() {
    if (_closed) {
      throw const CalculatorTransportException(
        .closed,
        'CalculatorClient has been closed.',
      );
    }
    if (_permanentError != null) {
      throw _permanentError!;
    }
  }

  String _allocateRequestId() => 'r${++_nextIdCounter}';
}

final class _ActiveProcess {
  _ActiveProcess(this.process);

  final Process process;
  final BytesBuilder stdoutBuffer = BytesBuilder(copy: false);
  final BytesBuilder stderrBuffer = BytesBuilder(copy: false);
  final Map<String, _PendingRequest> pending = {};
  final Completer<void> exitCompleter = Completer<void>();

  StreamSubscription<List<int>>? stdoutSub;
  StreamSubscription<List<int>>? stderrSub;
  Timer? exitDrainTimer;
  bool stdinClosed = false;
  bool stdoutDone = false;
  bool framingFailed = false;
  bool brokenHandled = false;
  bool exited = false;
  bool intentionalExit = false;
  int? exitCodeValue;
}

final class _PendingRequest {
  _PendingRequest({
    required this.id,
    required this.op,
    required this.revision,
    required this.sequence,
    required this.isRetrySafe,
    required this.completer,
  });

  final String id;
  final String op;
  final int? revision;
  final int sequence;
  final bool isRetrySafe;
  final Completer<Map<String, Object?>> completer;
  Timer? timer;
}

final class _RetryableCrash implements Exception {
  const _RetryableCrash(this.error);

  final CalculatorTransportException error;
}
