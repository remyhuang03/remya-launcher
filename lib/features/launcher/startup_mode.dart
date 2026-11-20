const launcherPreferencesId = 'remya-launcher';
const residentStartupKey = 'resident-startup';

bool readResidentStartup(Map<String, dynamic> reply) {
  final value = reply['value'];
  if (reply['id'] == '$launcherPreferencesId.$residentStartupKey' &&
      reply['availability'] == 'available' &&
      value is bool) {
    return value;
  }
  throw const FormatException('Launcher startup preference is unavailable.');
}
