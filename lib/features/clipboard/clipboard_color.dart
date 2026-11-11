import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

Color? clipboardColor(String? hex) {
  if (hex == null || (hex.length != 7 && hex.length != 9)) return null;
  final rgb = int.tryParse(hex.substring(1, 7), radix: 16);
  final alpha = hex.length == 9
      ? int.tryParse(hex.substring(7), radix: 16)
      : 255;
  if (rgb == null || alpha == null) return null;
  return Color((alpha << 24) | rgb);
}

List<String> clipboardColorFormats(String hex, Color color) {
  final hsl = HSLColor.fromColor(color);
  final alpha = color.a == 1 ? '' : ' / ${_decimal(color.a)}';
  double linear(double component) => component <= .04045
      ? component / 12.92
      : math.pow((component + .055) / 1.055, 2.4).toDouble();
  final r = linear(color.r);
  final g = linear(color.g);
  final b = linear(color.b);
  double root(double value) => math.pow(value, 1 / 3).toDouble();
  final l = root(.4122214708 * r + .5363325363 * g + .0514459929 * b);
  final m = root(.2119034982 * r + .6806995451 * g + .1073969566 * b);
  final s = root(.0883024619 * r + .2817188376 * g + .6299787005 * b);
  final lightness = .2104542553 * l + .7936177850 * m - .0040720468 * s;
  final a = 1.9779984951 * l - 2.4285922050 * m + .4505937099 * s;
  final chromaB = .0259040371 * l + .7827717662 * m - .8086757660 * s;
  final chroma = math.sqrt(a * a + chromaB * chromaB);
  final hue = chroma < .00001
      ? 0
      : (math.atan2(chromaB, a) * 180 / math.pi) % 360;
  return [
    hex,
    'rgb(${(color.r * 255).round()} ${(color.g * 255).round()} ${(color.b * 255).round()}$alpha)',
    'hsl(${_decimal(hsl.hue)} ${_decimal(hsl.saturation * 100)}% ${_decimal(hsl.lightness * 100)}%$alpha)',
    'oklch(${_decimal(lightness)} ${_decimal(chroma)} ${_decimal(hue.toDouble())}$alpha)',
  ];
}

String _decimal(double value) =>
    value.toStringAsFixed(3).replaceFirst(RegExp(r'\.?0+$'), '');
