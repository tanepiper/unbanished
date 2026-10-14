#!/usr/bin/env python3
"""Crosswalk serialized resource type IDs to Ghidra BaseTypeInfo registrations."""
import argparse
import csv
import json
import re


REGISTRATION = re.compile(
    r'Object::BaseTypeInfo::BaseTypeInfo\s*\(\s*\(BaseTypeInfo \*\)&([^,]+),'
    r'\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,\s*\(char \*\)0x0\s*,'
    r'\s*(.*?)\);',
    re.S,
)
FUNCTION = re.compile(r'^// ==== ([0-9a-fA-F]+) .*?$', re.M)


def split_callbacks(text):
    parts, start, depth = [], 0, 0
    for index, char in enumerate(text):
        if char == '(':
            depth += 1
        elif char == ')':
            depth -= 1
        elif char == ',' and depth == 0:
            parts.append(text[start:index].strip())
            start = index + 1
    parts.append(text[start:].strip())
    return parts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--types', default='tools/schema/types.json')
    parser.add_argument('--references', default='docs/GAME-TYPE-STRING-REFERENCES.tsv')
    parser.add_argument('--decomp', default='analysis/export/Application-steam-x64.exe/decomp.c')
    parser.add_argument('-o', '--output', default='docs/GAME-TYPE-REGISTRATIONS.tsv')
    args = parser.parse_args()

    with open(args.types, encoding='utf-8') as source:
        types = json.load(source)
    with open(args.references, encoding='utf-8', newline='') as source:
        refs = list(csv.DictReader(source, delimiter='\t'))
    strings, xref_functions = {}, {}
    for row in refs:
        prefix = 'application' if row['program'] == 'Application-steam-x64.exe' else 'runtime'
        name = row['class_name']
        if row['string_address'] and row['string_address'] != '<not-found>':
            strings.setdefault((prefix, name), set()).add(row['string_address'])
        if row['function_address'] and row['function_address'] not in ('<none>', '<not-found>'):
            xref_functions.setdefault((prefix, name), set()).add(row['function_address'])

    with open(args.decomp, encoding='utf-8', errors='replace') as source:
        decomp = source.read()
    headings = list(FUNCTION.finditer(decomp))
    registrations = {}
    for match in REGISTRATION.finditer(decomp):
        class_name = match.group(2)
        if not any(class_name in item['names'] for item in types.values()):
            continue
        heading = next((h for h in reversed(headings) if h.start() <= match.start()), None)
        callbacks = split_callbacks(match.group(4))
        registration = {
            'type_info': match.group(1).strip(),
            'class_name': class_name,
            'parent': match.group(3).strip(),
            'allocating_callback': callbacks[0] if callbacks else '',
            'in_place_callback': callbacks[1] if len(callbacks) > 1 else '',
            'registration_function': heading.group(1).lower() if heading else '',
        }
        registrations[class_name] = registration

    columns = [
        'type_id', 'class_name', 'resource_count', 'file_versions', 'example_resource',
        'application_string_addresses', 'application_string_xref_functions',
        'runtime_string_addresses', 'runtime_string_xref_functions',
        'type_info_global', 'registration_function', 'parent_class',
        'allocating_callback', 'in_place_callback', 'application_registration_status',
    ]
    with open(args.output, 'w', encoding='utf-8', newline='') as output:
        writer = csv.DictWriter(output, fieldnames=columns, delimiter='\t', lineterminator='\n')
        writer.writeheader()
        for type_id, info in sorted(types.items()):
            class_name = info['names'][0] if len(info.get('names', [])) == 1 else ''
            registration = registrations.get(class_name, {})
            writer.writerow({
                'type_id': type_id,
                'class_name': class_name,
                'resource_count': info.get('count', ''),
                'file_versions': ','.join(map(str, info.get('versions', []))),
                'example_resource': info.get('example', ''),
                'application_string_addresses': ','.join(sorted(strings.get(('application', class_name), []))),
                'application_string_xref_functions': ','.join(sorted(xref_functions.get(('application', class_name), []))),
                'runtime_string_addresses': ','.join(sorted(strings.get(('runtime', class_name), []))),
                'runtime_string_xref_functions': ','.join(sorted(xref_functions.get(('runtime', class_name), []))),
                'type_info_global': registration.get('type_info', ''),
                'registration_function': registration.get('registration_function', ''),
                'parent_class': registration.get('parent', ''),
                'allocating_callback': registration.get('allocating_callback', ''),
                'in_place_callback': registration.get('in_place_callback', ''),
                'application_registration_status': 'confirmed' if registration else 'no-application-registration-match',
            })
    confirmed = sum(bool(registrations.get(info['names'][0])) for info in types.values() if len(info.get('names', [])) == 1)
    print(f"wrote {len(types)} resource type rows; {confirmed} have matching Application BaseTypeInfo registrations")
    print(f"output: {args.output}")


if __name__ == '__main__':
    main()
