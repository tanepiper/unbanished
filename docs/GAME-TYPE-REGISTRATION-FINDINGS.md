# Resource class registrations and identity pointers

This is the binary-side crosswalk for the 170 serialized resource type IDs in the package
inventory. It joins each observed ID and class-name candidate to Ghidra's Application `BaseTypeInfo`
registration where an exact class-name registration is present, and keeps string xrefs distinct
from registration evidence.

## Registration catalog

[`GAME-TYPE-REGISTRATIONS.tsv`](GAME-TYPE-REGISTRATIONS.tsv) contains one row per resource type ID:
the observed ID, class name, resource count and versions, example path, Application and Runtime type
string addresses/xref functions, the Application `BaseTypeInfo` global, registration function,
parent class, allocating callback, in-place callback, and registration status.

Generate it from the existing Ghidra type-string crosswalk and the Application pseudocode export:

```sh
python3 tools/schema/export_type_registrations.py
```

The `types.json` IDs are matched to names using CRC32 of UTF-16LE class names and observed package
headers. The registration fields are extracted only when an Application
`Object::BaseTypeInfo::BaseTypeInfo` call has the exact class name in its registered-class
argument. The two relevant evidence inputs are `docs/GAME-TYPE-STRING-REFERENCES.tsv` and
`analysis/export/Application-steam-x64.exe/decomp.c`, exported from Ghidra 12.1.4. No Hopper data is
used.

The current catalog has 170 resource-type rows. An exact Application registration match was found
for 116 classes. Those rows provide a static type-info global address, registration function,
parent name, and both factory callbacks. Among the 116, 78 derive from `Description` and 15 from
`Object::External`; the rest are tutorial/UI/graphics classes with their own parent types. The
remaining 54 rows have no exact matching Application registration call in the exported
pseudocode. Fifty-two of those have Runtime type-name string references; such references include
`_TypeName`/`TypeName` methods and data xrefs, so they are not treated as evidence of a Runtime
registration. `Font` and `NameDatabase` have neither a defined Application registration match nor a
Runtime string address in the current crosswalk.

## Example: citizen description

The `CitizenDescription` row joins type ID `0xe16728ca`, observed in
`Template/Citizen_citizen.crs`, to the Application string at `0x140168978`, the static type-info
global `DAT_1401cf190`, and registration function `0x140154650`. Its parent is `Description`;
its allocating callback is `0x140030ad0`. Its in-place callback is the Ghidra label at
`0x140030b10`. These are binary identities: the global and callback addresses must be qualified
by
the Application image hash recorded in the entity lifecycle trace.

The serialized ID identifies the external resource class. A `BaseTypeInfo` global is a static
registration record in the executable, not an allocated component pointer. The runtime object
pointer layer is still separate and requires a field, owner, and lifetime observation.

## An independent check of the type ids

GameFrame's `tools/triage/hashnames.py` was run over all 5,812 extracted resources, taking
candidate names from the strings of `Application-steam-x64.exe` and `Runtime-steam-x64.dll`,
with no hint about the hash. It found CRC-32 of the UTF-16 class name on its own: 6,323 aligned
values matched, against 629 expected by chance. It named 169 of the 170 type ids in
`GAME-TYPE-REGISTRATIONS.tsv`. The one it missed in that scan, `TutorialImage` (`0x00305aa7`), is
named when asked for directly (`--value 0x00305aa7`): the scan leaves out values below 2^24, which
are too common in data to count. The run also named classes whose hashes occur inside resources
without being registered resource types, such as `ResourceLimitUI`, `BuildUI`, `DestroyUI`,
`StatusIconUI` and `ConsumeProduceComponent`; each is already described in these documents.
(*Measured*, 2026-10-22.)

```bash
python3 $GAMEFRAME/tools/triage/hashnames.py game-data/extracted --aligned \
  --names-from game-data/original/Application-steam-x64.exe --names-from game-data/original/Runtime-steam-x64.dll
```

## Limits and next mapping step

This catalog does not claim all 170 class registrations have been located across both binaries.
It records confirmed Application constructor calls and leaves the rest unresolved rather than
promoting a type-name method or arbitrary data xref into a registration. Some resource types may be
engine-owned in Runtime, while others need additional registration-table tracing.

The next useful crosswalk is from serialized reference fields and package paths to the loaders and
runtime object fields that consume them. For each result, record the source resource path and hash,
serialized class/field, resolved type-info and callback, loaded owner/slot, and binary image address.
The save-local path-ID layer is indexed separately in
[SAVE-REFERENCE-IDENTITIES.md](SAVE-REFERENCE-IDENTITIES.md).
