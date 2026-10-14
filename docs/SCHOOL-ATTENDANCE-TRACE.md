# School attendance and education progress

This note consolidates the school-related path traced so far. It records what connects the
shipped school resource to citizen state and keeps the per-attendance progress sender explicitly
open. Evidence is static Ghidra 12.1.4 pseudocode from the Steam x64 application and decoded
resource data from the shipped `.pkg` files; it is not a runtime observation.

## Identified path

| Stage | Evidence | Established behavior |
|---|---|---|
| Citizen configuration | `Template/Citizen_citizen.crs`; serializer `0x140119dd0` | Description pointers at `+0x20`, `+0x28`, `+0x30` resolve in order to teacher, priest and doctor profession descriptions. |
| School configuration | `Template/SchoolHouse_resource.crs`, `SchoolHouse_workplace.crs`, `SchoolHouse_attendance.crs` | The school template includes `workplace` and `attendance`; its workplace references `Game/Profession/Profession.rsc:teacher`; attendance capacity is 20. |
| Attendance provider search | `0x140116e00`, called by `0x140116d10` | Search registered attendance providers whose workplace description matches the citizen's teacher-description key; reject full or unreachable candidates and retain the best route-ranked candidate. |
| Citizen assignment | `0x140116d10` | On acceptance, increment the provider's attendee collection, set citizen `+0x208`, and set `+0x220`. The update path attempts this lookup after the age threshold when `+0x208` is empty and `+0x220` is false. |
| Attendance duration | `0x140118340` | Add the citizen update delta to `+0x218` only while `+0x208` is non-null. |
| Education message receiver | `0x140117730` case 4 → `0x140116c70` | Increment `+0x210` once per received message. Continue below the configured threshold while elapsed attendance time is within `CitizenDescription+0x11c * 3600`; otherwise clamp to `+0x118`, set state bit `0x400`, clear school attendance, and report status 9. |
| Completion effects | `0x140116c70`, `0x14011af20`, Citizen UI `0x140065440`, production output `0x1400cc010` | Completion removes the school provider association. The UI compares progress with the same threshold; production selects the higher raw-material quantity at or above it. |

The teacher, priest and doctor assignments are separate citizen slots: `+0x208`, `+0x230` and
`+0x228`. Shipped capacities are school 20, doctor house 30 and chapel 200; constructor fallback
capacity is 15. The wider citizen trace is in
[CITIZEN-NEEDS-AND-HOUSEHOLDS.md](CITIZEN-NEEDS-AND-HOUSEHOLDS.md), and component attachment and
update dispatch are in [ENTITY-COMPONENT-LIFECYCLE.md](ENTITY-COMPONENT-LIFECYCLE.md).

## Remaining sender question

The code that periodically invokes Citizen message 4 is still unidentified. The update routine
assigns an attendance provider and advances elapsed time, but it does not directly call the case-4
receiver in the observed pseudocode. The attendance serializer and shipped `WorkDescription`
serializer likewise contain no explicit message identifier: `0x140133d80` serializes numeric
fields at `+0x24`, `+0x28`, `+0x2c`, `+0x30`, boolean fields at `+0x34` and `+0x20`.

A literal search for integer 4 is not enough to identify the sender. Other registered callback
families also place 4 in a callback payload and invoke a virtual function at `param_2`'s `+0x08`
slot. Examples include `0x140045670` (registered from `0x140047200`) and `0x140122c80` (registered
from `0x1401231d0`). Those registrations establish callback tables and dispatch shape, but not
that their recipient is a citizen or that they advance education. They are therefore leads, not
attributions of the school message.

The next useful step is to resolve the recipient construction and callback-type registration around
the citizen AI action that chooses a school, then follow its completion/update callback to the
virtual message dispatch. Until that link is recovered, do not infer a per-second progress rate or
name a teacher-side sender.

