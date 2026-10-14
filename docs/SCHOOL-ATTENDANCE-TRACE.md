# School attendance and education progress

This note consolidates the school-related path traced so far. It connects the shipped school
resource, the Citizen AI action, scheduled component message, citizen education state, and the
visible/output thresholds. Evidence is static Ghidra 12.1.4 pseudocode from the Steam x64
application and decoded resource data from the shipped `.pkg` files; it is not a runtime
observation.

## Identified path

| Stage | Evidence | Established behavior |
|---|---|---|
| Citizen configuration | `Template/Citizen_citizen.crs`; serializer `0x140119dd0` | Description pointers at `+0x20`, `+0x28`, `+0x30` resolve in order to teacher, priest and doctor profession descriptions. |
| School configuration | `Template/SchoolHouse_resource.crs`, `SchoolHouse_workplace.crs`, `SchoolHouse_attendance.crs` | The school template includes `workplace` and `attendance`; its workplace references `Game/Profession/Profession.rsc:teacher`; attendance capacity is 20. |
| Attendance provider search | `0x140116e00`, called by `0x140116d10` | Search registered attendance providers whose workplace description matches the citizen's teacher-description key; reject full or unreachable candidates and retain the best route-ranked candidate. |
| Citizen assignment | `0x140116d10` | On acceptance, increment the provider's attendee collection, set citizen `+0x208`, and set `+0x220`. The update path attempts this lookup after the age threshold when `+0x208` is empty and `+0x220` is false. |
| Attendance duration | `0x140118340` | Add the citizen update delta to `+0x218` only while `+0x208` is non-null. |
| School AI action | `0x1401146c0` action 10 → `0x140112a30` | The action reads its citizen from AI `+0x38`, the assigned AttendanceComponent from citizen `+0x208`, resolves/validates the provider location, and schedules a CitizenComponent message with code 4 and the target position. |
| Scheduled message | `0x1400c0960` → `0x1400d06a0` → `0x1400d03c0` | The queue helper constructs a typed component-message object. Its fields include the CitizenComponent type ID at `+0x30`, message code at `+0x34`, target payload at `+0x28`, and flags at `+0x38`; dispatch finds the recipient component and invokes virtual slot `+0x78`. |
| Education message receiver | Citizen vtable `0x1401689a0`, slot 15 (`+0x78`) → `0x140117730` case 4 → `0x140116c70` | Increment `+0x210` once per received message. Continue below the configured threshold while elapsed attendance time is within `CitizenDescription+0x11c * 3600`; otherwise clamp to `+0x118`, set state bit `0x400`, clear school attendance, and report status 9. |
| Completion effects | `0x140116c70`, `0x14011af20`, Citizen UI `0x140065440`, production output `0x1400cc010` | Completion removes the school provider association. The UI compares progress with the same threshold; production selects the higher raw-material quantity at or above it. |

The teacher, priest and doctor assignments are separate citizen slots: `+0x208`, `+0x230` and
`+0x228`. Shipped capacities are school 20, doctor house 30 and chapel 200; constructor fallback
capacity is 15. The wider citizen trace is in
[CITIZEN-NEEDS-AND-HOUSEHOLDS.md](CITIZEN-NEEDS-AND-HOUSEHOLDS.md), and component attachment and
update dispatch are in [ENTITY-COMPONENT-LIFECYCLE.md](ENTITY-COMPONENT-LIFECYCLE.md).

## Scope and remaining timing questions

The identified message-4 sender is the Citizen AI's action-10 routine `0x140112a30`; it is not a
teacher-side callback. The routine passes code 4 to `0x1400c0960`, which packages it as a typed
component message. The package's type ID resolves to `CitizenComponent`, and the message object's
dispatch method resolves that component on the target entity before calling its `+0x78` virtual
handler. The confirmed CitizenComponent vtable at `0x1401689a0` has `0x140117730` in slot 15,
closing the route to the case-4 education handler.

This establishes the sender and receiver path but not its real-time cadence, queue ordering relative
to movement completion, or the progress-per-time scale. `0x140112a30` checks that an attendance
provider and usable destination exist, then queues work; do not infer a fixed progress rate or claim
that each message is sent on a particular arrival/timer event without tracing the task queue's
execution ordering. The attendance and `WorkDescription` serializers do not themselves encode an
explicit message identifier.

The relevant identities and pointers are cataloged in [GAME-IDENTITY-MAP.md](GAME-IDENTITY-MAP.md).

