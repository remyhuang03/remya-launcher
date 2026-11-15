import 'package:material_ui/material_ui.dart';
import 'package:remya_ui/settings_ui.dart';

import 'apps_client.dart';

RemyaSettingsPage appsSettingsPage(AppsClient client) => RemyaSettingsPage(
  id: 'apps',
  title: 'Apps',
  icon: Icons.apps_rounded,
  category: .system,
  children: [
    RemyaSettingsPage(
      id: 'apps-preferences',
      title: 'Preferences',
      icon: Icons.tune_rounded,
      builder: (_) => _AppsPreferencesPage(client: client),
    ),
    RemyaSettingsPage(
      id: 'hidden-apps',
      title: 'Hidden apps',
      icon: Icons.visibility_off_outlined,
      builder: (_) => _HiddenAppsPage(client: client),
    ),
  ],
);

class _AppsPreferencesPage extends StatefulWidget {
  const _AppsPreferencesPage({required this.client});

  final AppsClient client;

  @override
  State<_AppsPreferencesPage> createState() => _AppsPreferencesPageState();
}

class _AppsPreferencesPageState extends State<_AppsPreferencesPage> {
  late Future<Map<String, dynamic>> _preferences = widget.client.preferences();
  bool _saving = false;
  String? _error;

  Future<void> _save(String key, Object value) async {
    if (_saving) return;
    setState(() {
      _error = null;
      _saving = true;
    });
    try {
      final preferences = await widget.client.preferences({key: value});
      if (mounted) setState(() => _preferences = Future.value(preferences));
    } on Object catch (error) {
      if (mounted) {
        setState(() => _error = 'Could not save preferences.\n$error');
      }
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _edit(_EditablePreference preference, Object value) async {
    final text = await showDialog<String>(
      context: context,
      builder: (_) => _PreferenceDialog(preference, value),
    );
    if (!mounted || text == null) return;
    final updated = preference == .frequentDays ? int.parse(text) : text;
    if (updated != value) await _save(preference.name, updated);
  }

  SettingRow _toggle(
    String key,
    String label,
    Map<String, dynamic> preferences, {
    String? description,
    bool enabled = true,
  }) => SettingRow(
    id: key,
    label: SettingLabel(label, description: description),
    control: Semantics(
      label: label,
      child: Switch(
        value: preferences[key] as bool,
        onChanged: !_saving && enabled ? (value) => _save(key, value) : null,
      ),
    ),
  );

  SettingRow _editRow(_EditablePreference preference, Object value) =>
      SettingRow(
        id: preference.name,
        label: SettingLabel(
          preference.label,
          description: preference.description,
        ),
        control: OutlinedButton(
          onPressed: _saving ? null : () => _edit(preference, value),
          child: Text(switch (preference) {
            .frequentDays => '$value days',
            .terminal => value == '' ? 'Automatic' : '$value',
          }, overflow: .ellipsis),
        ),
      );

  @override
  Widget build(BuildContext context) => FutureBuilder(
    future: _preferences,
    builder: (context, snapshot) {
      if (snapshot.hasError) {
        return SettingsErrorCard(
          message: 'Could not load Apps preferences.\n${snapshot.error}',
          onRetry: () => setState(() {
            _preferences = widget.client.preferences();
          }),
        );
      }
      final preferences = snapshot.data;
      if (preferences == null) return const RemyaLoadingBar();
      final showPackages = preferences['showPackages'] as bool;
      return Column(
        crossAxisAlignment: .stretch,
        children: [
          if (_error case final error?)
            SettingsErrorCard(message: error, actionLabel: null),
          SettingsGroup(
            title: 'Search and launch',
            children: [
              SettingRow(
                id: 'alwaysNew',
                label: const SettingLabel('When an app is already running'),
                control: Semantics(
                  label: 'When an app is already running',
                  child: DropdownButtonHideUnderline(
                    child: DropdownButton<bool>(
                      value: preferences['alwaysNew'] as bool,
                      items: const [
                        DropdownMenuItem(
                          value: false,
                          child: Text('Focus window'),
                        ),
                        DropdownMenuItem(
                          value: true,
                          child: Text('Always open a new window'),
                        ),
                      ],
                      onChanged: _saving
                          ? null
                          : (value) {
                              if (value != null) _save('alwaysNew', value);
                            },
                    ),
                  ),
                ),
              ),
              _editRow(.frequentDays, preferences['frequentDays'] as int),
              _toggle(
                'pinyin',
                'Pinyin matching',
                preferences,
                description: 'Match Chinese app names by pinyin or initials.',
              ),
              _editRow(.terminal, preferences['terminal'] as String),
            ],
          ),
          SettingsGroup(
            title: 'Install suggestions',
            children: [
              _toggle(
                'showPackages',
                'Show install suggestions',
                preferences,
                description:
                    'Search local package data when no installed app matches.',
              ),
              _toggle(
                'official',
                'Official repositories',
                preferences,
                enabled: showPackages,
              ),
              _toggle('flatpak', 'Flatpak', preferences, enabled: showPackages),
              _toggle(
                'aur',
                'AUR',
                preferences,
                description:
                    'Uses cached package data from an installed AUR helper.',
                enabled: showPackages,
              ),
            ],
          ),
        ],
      );
    },
  );
}

enum _EditablePreference {
  frequentDays('Frequent apps period', 'How far back to count app launches.'),
  terminal('Default terminal', 'Used for terminal apps and install commands.');

  const _EditablePreference(this.label, this.description);

  final String label;
  final String description;
}

class _PreferenceDialog extends StatefulWidget {
  const _PreferenceDialog(this.preference, this.value);

  final _EditablePreference preference;
  final Object value;

  @override
  State<_PreferenceDialog> createState() => _PreferenceDialogState();
}

class _PreferenceDialogState extends State<_PreferenceDialog> {
  final _form = GlobalKey<FormState>();
  late String _text = '${widget.value}';

  void _submit() {
    if (_form.currentState!.validate()) Navigator.pop(context, _text.trim());
  }

  @override
  Widget build(BuildContext context) {
    final preference = widget.preference;
    final number = preference == .frequentDays;
    return AlertDialog(
      title: Text(preference.label),
      content: Form(
        key: _form,
        child: TextFormField(
          autofocus: true,
          initialValue: _text,
          keyboardType: number ? .number : .text,
          decoration: InputDecoration(
            labelText: number ? 'Days' : 'Terminal executable',
            helperText: number
                ? 'Enter 1 to 365 days.'
                : 'Leave empty to choose automatically.',
          ),
          onChanged: (value) => _text = value,
          onFieldSubmitted: (_) => _submit(),
          validator: number
              ? (text) {
                  final value = int.tryParse(text?.trim() ?? '');
                  return value == null || value < 1 || value > 365
                      ? 'Enter a whole number from 1 to 365.'
                      : null;
                }
              : null,
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Save')),
      ],
    );
  }
}

class _HiddenAppsPage extends StatefulWidget {
  const _HiddenAppsPage({required this.client});

  final AppsClient client;

  @override
  State<_HiddenAppsPage> createState() => _HiddenAppsPageState();
}

class _HiddenAppsPageState extends State<_HiddenAppsPage> {
  late Future<List<LauncherAppEntry>> _hidden = widget.client.hidden();
  bool _restoring = false;
  String? _error;

  Future<void> _restore(
    LauncherAppEntry app,
    List<LauncherAppEntry> hidden,
  ) async {
    if (_restoring) return;
    setState(() {
      _error = null;
      _restoring = true;
    });
    try {
      await widget.client.perform('unhide', appId: app.id);
      if (!mounted) return;
      setState(() {
        _hidden = Future.value(
          hidden.where((entry) => entry.id != app.id).toList(),
        );
      });
    } on Object catch (error) {
      if (mounted) {
        setState(() => _error = 'Could not restore ${app.name}.\n$error');
      }
    } finally {
      if (mounted) setState(() => _restoring = false);
    }
  }

  @override
  Widget build(BuildContext context) => FutureBuilder(
    future: _hidden,
    builder: (context, snapshot) {
      if (snapshot.hasError) {
        return SettingsErrorCard(
          message: 'Could not load hidden apps.\n${snapshot.error}',
          onRetry: () => setState(() => _hidden = widget.client.hidden()),
        );
      }
      final apps = snapshot.data;
      if (apps == null) return const RemyaLoadingBar();
      if (apps.isEmpty) {
        return const SettingsSurface(child: Text('No hidden apps.'));
      }
      return Column(
        crossAxisAlignment: .stretch,
        children: [
          if (_error case final error?)
            SettingsErrorCard(message: error, actionLabel: null),
          SettingsSurface(
            padding: .zero,
            child: settingsRows([
              for (final app in apps)
                SettingRow(
                  id: app.id,
                  label: SettingLabel(
                    app.name,
                    description: app.description.isEmpty
                        ? app.id
                        : app.description,
                  ),
                  control: Semantics(
                    label: 'Restore ${app.name}',
                    child: OutlinedButton(
                      onPressed: _restoring ? null : () => _restore(app, apps),
                      child: const Text('Restore'),
                    ),
                  ),
                ),
            ]),
          ),
        ],
      );
    },
  );
}
