#!/usr/bin/env python3
"""Generate the offline emoji catalog from verified upstream snapshots."""

import argparse
from collections import Counter, defaultdict
from hashlib import sha256
from io import BytesIO
import json
import os
from pathlib import Path
import re
import tarfile
from urllib.request import urlopen

TOOL = Path(__file__).resolve().parent
ASSETS = TOOL.parents[1] / "assets" / "emoji"
LOCALES = {"en": "en", "zh-Hans": "zh", "zh-Hant": "zh-Hant",
           "ja": "ja", "ru": "ru", "de": "de", "es": "es"}


def read_sources(lock, cache):
    cache.mkdir(parents=True, exist_ok=True)
    sources = {}
    for name, source in lock.items():
        path = cache / source["file"]
        if path.exists():
            data = path.read_bytes()
        else:
            with urlopen(source["url"], timeout=60) as response:
                data = response.read()
        if sha256(data).hexdigest() != source["sha256"]:
            raise ValueError(f"SHA-256 mismatch for {name}: {path}")
        if not path.exists():
            path.write_bytes(data)
        sources[name] = data
    return sources


def decode_hex(codes):
    value = "".join(chr(int(code, 16)) for code in codes.split())
    value.encode("utf-8")  # Reject surrogate code points.
    return value


def code_id(value):
    return "-".join(f"{ord(char):04X}" for char in value)


def canonical_data(data, version):
    if f"# Version: {version}\n" not in data:
        raise ValueError(f"Expected Unicode Emoji {version}")
    groups = []
    official = {}
    spellings = []
    by_components = {}
    for line in data.splitlines():
        if line.startswith("# group: "):
            group = line.removeprefix("# group: ")
        elif line.startswith("# subgroup: "):
            subgroup = line.removeprefix("# subgroup: ")
        elif line and not line.startswith("#"):
            codes, status, glyph, added, label = re.fullmatch(
                r"([0-9A-F ]+?)\s*;\s*([\w-]+)\s*#\s*(\S+)\s+E([\d.]+)\s+(.+)", line
            ).groups()
            value = decode_hex(codes)
            if value != glyph:
                raise ValueError(f"Glyph/codepoint disagreement for {codes}")
            components = value.replace("\ufe0f", "")
            if status == "fully-qualified":
                if value in official or components in by_components:
                    raise ValueError(f"Ambiguous RGI entry for {codes}")
                if group not in groups:
                    groups.append(group)
                official[value] = (groups.index(group), subgroup, added, label)
                by_components[components] = value
            elif status in ("unqualified", "minimally-qualified"):
                spellings.append((value, components))
            elif status != "component":
                raise ValueError(f"Unknown qualification status: {status}")
    qualifiers = {value: by_components[components] for value, components in spellings}
    declared_total = int(re.search(r"^# fully-qualified\s+:\s*(\d+)", data, re.M)[1])
    if len(official) != declared_total or len(qualifiers) != len(spellings):
        raise ValueError("Unicode catalog count or qualification mapping disagrees")
    return groups, official, qualifiers


def package_text(package, member):
    with package.extractfile(member) as stream:
        return stream.read().decode("utf-8")


def package_json(package, member):
    return json.loads(package_text(package, "package/" + member))


def merge_annotations(packages, locale, resolve, unmapped):
    merged = defaultdict(dict)
    for prefix, name, key in (
        ("annotations", "cldrAnnotations", "annotations"),
        ("annotationsDerived", "cldrDerived", "annotationsDerived"),
    ):
        annotations = package_json(
            packages[name], f"{prefix}/{locale}/annotations.json"
        )[key]["annotations"]
        for emoji, entry in annotations.items():
            canonical = resolve(emoji)
            if canonical:
                merged[canonical].update(entry)
            else:
                unmapped.add((name, locale, code_id(emoji)))
    return merged


def emojibase_entries(package, locale, resolve, official, unmapped):
    localized = {}
    variants = defaultdict(list)
    for entry in package_json(package, f"{locale}/data.json"):
        canonical = resolve(entry["emoji"])
        if canonical and entry.get("gender") is not None:
            parts = canonical.split("\u200d")
            # Only relate explicitly gendered records to an existing RGI
            # neutral record in the same Unicode subgroup.
            if parts[-1] in ("♀️", "♂️"):
                neutral = resolve("\u200d".join(parts[:-1]))
            elif len(parts) > 1 and parts[0] in ("👩", "👨"):
                neutral = resolve("\u200d".join(["🧑", *parts[1:]]))
            else:
                neutral = None
            if neutral and official[neutral][:2] == official[canonical][:2]:
                variants[neutral].append(canonical)
        for record in (entry, *entry.get("skins", [])):
            value = resolve(record["emoji"])
            if value:
                localized[value] = record
                if canonical and value != canonical:
                    variants[canonical].append(value)
            else:
                unmapped.add(("emojibase", locale, record["hexcode"]))
    return localized, variants


def normalize_term(term):
    return " ".join(term.replace("_", " ").split())


def compile_catalog(lock, packages, unicode_data, aliases):
    version = lock["unicodeEmoji"]["version"]
    groups, official, qualifiers = canonical_data(unicode_data, version)

    def resolve(value):
        return value if value in official else qualifiers.get(value)

    unmapped = set()
    deltas = {}
    all_terms = {}
    localizations = {}
    localization_sources = {}
    variant_children = {}
    for locale, source_locale in LOCALES.items():
        locale_sources = Counter()
        differences = defaultdict(list)
        cldr = merge_annotations(packages, source_locale, resolve, unmapped)
        emojibase, variants = emojibase_entries(
            packages["emojibase"], source_locale.lower(), resolve, official, unmapped
        )
        if locale == "en":
            variant_children = variants
        for emoji, (_, _, _, official_name) in official.items():
            latest = cldr.get(emoji, {})
            previous = emojibase.get(emoji, {})
            latest_name = next(iter(latest.get("tts", [])), None)
            previous_name = previous.get("label")
            if latest_name and previous_name != latest_name:
                differences["renamed" if previous_name else "new"].append(
                    [code_id(emoji), previous_name, latest_name]
                )
            if not latest_name:
                differences["missing"].append(code_id(emoji))
                if previous_name:
                    differences["removed"].append(code_id(emoji))

            latest_words = latest.get("default")
            previous_words = previous.get("tags", [])
            if latest_words and not previous_words:
                differences["keywords_added"].append(code_id(emoji))
            elif previous_words and latest_words is not None and set(latest_words) != set(previous_words):
                differences["keyword_changed"].append(
                    [code_id(emoji), sorted(set(previous_words) - set(latest_words)),
                     sorted(set(latest_words) - set(previous_words))]
                )

            if latest_name:
                name, name_source = latest_name, "cldr"
            elif previous_name:
                name, name_source = previous_name, "emojibase"
            elif locale != "en":
                name, name_source = all_terms[("en", emoji)][0][0], "en-fallback"
            else:
                name, name_source = official_name, "unicodeEmoji"
            words, words_source = (latest_words, "cldr") if latest_words is not None else (
                previous_words, "emojibase"
            )
            all_terms[locale, emoji] = [
                (name, name_source, "name"),
                *((word, words_source, "keyword") for word in words),
            ]
            locale_sources[name_source] += 1
        localization_sources[locale] = dict(locale_sources)
        deltas[locale] = differences

    shortcodes = defaultdict(list)
    for preset in ("github", "emojibase", "iamcal"):
        for key, codes in package_json(
            packages["emojibase"], f"en/shortcodes/{preset}.json"
        ).items():
            emoji = resolve(decode_hex(key.replace("-", " ")))
            if not emoji:
                unmapped.add((f"emojibase/{preset}", "en", key))
                continue
            for code in [codes] if isinstance(codes, str) else codes:
                all_terms["en", emoji].append(
                    (normalize_term(code), f"emojibase/{preset}", "shortcode")
                )
                candidates = shortcodes[code.strip(":").lower()]
                if emoji not in candidates:
                    candidates.append(emoji)
    for raw, words in package_json(packages["emojilib"], "dist/emoji-en-US.json").items():
        emoji = resolve(raw)
        if emoji:
            all_terms["en", emoji].extend(
                (normalize_term(word), "emojilib", "keyword") for word in words
            )
        else:
            unmapped.add(("emojilib", "en", code_id(raw)))
    for locale, kinds in aliases.items():
        for kind, entries in kinds.items():
            for emoji, words in entries.items():
                if emoji not in official:
                    raise ValueError(f"Curated alias has no RGI entry: {emoji}")
                all_terms[locale, emoji].extend((word, "project", kind) for word in words)

    for locale in LOCALES:
        rows = []
        for emoji in official:
            # Alias provenance stays available in the optional audit output.
            words = dict.fromkeys(re.sub(r"\s+", " ", term.replace("|", " ")).strip()
                                  for term, _, _ in all_terms[locale, emoji])
            rows.append("|".join(word for word in words if word))
        localizations[locale] = rows
    positions = {emoji: index for index, emoji in enumerate(official)}
    catalog = {
        "version": version,
        "common": [f"{emoji}|{group}|{added}" for emoji, (group, _, added, _) in official.items()],
        "locales": localizations,
        "groups": groups,
        "variants": {parent: sorted(set(children), key=positions.__getitem__)
                     for parent, children in variant_children.items()},
        "qualifiers": qualifiers,
        "shortcodes": shortcodes,
    }
    rgi_ids = [code_id(emoji) for emoji in official]
    catalog_ids = [code_id(row.split("|")[0]) for row in catalog["common"]]
    if catalog_ids != rgi_ids or any(not all(rows) for rows in localizations.values()):
        raise ValueError("Catalog order or RGI coverage mismatch")
    tones = {emoji for emoji in official if any("🏻" <= char <= "🏿" for char in emoji)}
    children = {emoji for variants in catalog["variants"].values() for emoji in variants}
    if not tones <= children:
        raise ValueError(f"Skin tone variants have no base: {tones - children}")
    new_ids = [code_id(emoji) for emoji, (_, _, added, _) in official.items()
               if added == "18.0"]
    if version == "18.0" and (len(new_ids) != 19 or any(
        len(catalog["variants"].get(thumb, [])) != 5 for thumb in ("🫹", "🫺")
    )):
        raise ValueError("Missing Emoji 18 characters or thumb tone variants")
    conflicts = {code: [code_id(item) for item in matches]
                 for code, matches in shortcodes.items() if len(matches) > 1}
    metadata = {"count": len(official), "qualificationAliases": len(qualifiers),
                "newEmoji18Ids": new_ids, "skinToneVariants": len(tones),
                "localizations": localization_sources,
                "cldrDelta": {locale: {kind: len(values) for kind, values in delta.items()}
                              for locale, delta in deltas.items()},
                "unmappedSourceEntries": len(unmapped),
                "unmappedSourceExamples": sorted(unmapped)[:20],
                "ambiguousShortcodes": len(conflicts)}
    return catalog, metadata, (all_terms, deltas, sorted(unmapped), conflicts)


def json_bytes(value, *, compact=False):
    settings = {"separators": (",", ":")} if compact else {"indent": 2}
    return (json.dumps(value, ensure_ascii=False, **settings) + "\n").encode()


def notices(lock, sources, packages):
    licenses = {
        "unicodeEmoji": sources["unicodeLicense"].decode(),
        "cldrAnnotations": package_text(packages["cldrAnnotations"], "package/LICENSE"),
        "cldrDerived": package_text(packages["cldrDerived"], "package/LICENSE"),
        "emojibase": package_text(packages["emojibase"], "package/LICENSE"),
        "emojilib": package_text(packages["emojilib"], "package/LICENSE"),
        "emojiPickerI18n": package_text(packages["emojiPickerI18n"], "LICENSE"),
        "unormDart": package_text(packages["unormDart"], "LICENSE"),
    }
    return "\n\n".join(
        f"{name} {lock[name]['version']} ({lock[name]['license']})\n{lock[name]['url']}\n\n"
        f"{license_text.strip()}" for name, license_text in licenses.items()
    ) + "\n"


def verify_bundle():
    manifest = json.loads((ASSETS / "manifest.json").read_bytes())
    if manifest["schemaVersion"] != 1:
        raise ValueError("Unexpected emoji asset schema version")
    for key, path in {
        "generatorSha256": Path(__file__),
        "aliasSha256": TOOL / "aliases.json",
        "sourceSetSha256": TOOL / "sources.lock.json",
        "catalogSha256": ASSETS / "catalog.json",
        "noticeSha256": ASSETS / "THIRD_PARTY_NOTICES.txt",
    }.items():
        if sha256(path.read_bytes()).hexdigest() != manifest[key]:
            raise ValueError(f"Emoji manifest hash mismatch: {path}")
    print(f"Verified bundled Emoji {manifest['sources']['unicodeEmoji']['version']} assets")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    default_cache = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache"))
    parser.add_argument("--cache", type=Path, default=default_cache / "remya-emoji-sources")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="Compare regenerated outputs with tracked files")
    mode.add_argument("--offline-check", action="store_true", help="Verify tracked hashes without upstream inputs")
    parser.add_argument("--audit", type=Path, help="Write detailed source and annotation differences here")
    args = parser.parse_args()
    if args.offline_check:
        if args.audit:
            parser.error("--audit requires upstream inputs")
        verify_bundle()
        return
    lock_bytes = (TOOL / "sources.lock.json").read_bytes()
    lock = json.loads(lock_bytes)
    alias_bytes = (TOOL / "aliases.json").read_bytes()
    sources = read_sources(lock, args.cache)
    packages = {key: tarfile.open(fileobj=BytesIO(data))
                for key, data in sources.items() if lock[key]["file"].endswith(".tgz")}
    catalog, metadata, audit = compile_catalog(
        lock, packages, sources["unicodeEmoji"].decode(), json.loads(alias_bytes)
    )
    catalog_bytes = json_bytes(catalog, compact=True)
    notice_bytes = notices(lock, sources, packages).encode()
    manifest = {
        "schemaVersion": 1,
        "generatorVersion": 1,
        "generatorSha256": sha256(Path(__file__).read_bytes()).hexdigest(),
        "aliasSha256": sha256(alias_bytes).hexdigest(),
        "sourceSetSha256": sha256(lock_bytes).hexdigest(),
        "catalogSha256": sha256(catalog_bytes).hexdigest(),
        "noticeSha256": sha256(notice_bytes).hexdigest(),
        "sources": lock,
        **metadata,
    }
    if args.audit:
        terms, delta, unmapped, conflicts = audit
        args.audit.write_bytes(json_bytes({
            "cldrDelta": delta,
            "unmapped": unmapped,
            "shortcodeConflicts": conflicts,
            "terms": [[locale, code_id(emoji), values]
                      for (locale, emoji), values in terms.items()],
        }))
    for name, data in {"catalog.json": catalog_bytes, "manifest.json": json_bytes(manifest),
                       "THIRD_PARTY_NOTICES.txt": notice_bytes}.items():
        path = ASSETS / name
        if args.check:
            if path.read_bytes() != data:
                parser.error(f"{path} differs from regenerated output")
        else:
            path.write_bytes(data)
    print(f"Emoji {catalog['version']}: {metadata['count']} fully qualified, "
          f"{metadata['qualificationAliases']} qualification aliases")


if __name__ == "__main__":
    main()
