# Launcher benchmarks

- [Cold startup on Hyprland](launcher-cold-start.md) measures release startup to the first committed Flutter frame.
- [Resident startup and memory](launcher-resident.md) measures hidden prewarming, reopening, and retained process and GPU memory. The [native prototype patch](launcher-resident-prototype.patch) applies to the baseline commit documented in the report.
