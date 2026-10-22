# Library code in the original binaries

This page lists the code in `Application-steam-x64.exe` and `Runtime-steam-x64.dll` that is not
the game's or the engine's own: the statically linked Microsoft C runtime (with the few C++
standard library classes it brings), the Steam DRM stub, and the exception funclets the compiler
adds. `tools/triage/library.py` finds it in the Ghidra export and writes `labels/library.tsv`.
The decompile tools show these labels, and `fnindex.py coverage` counts this code apart from the
game's own code. Every row is *observed* in the export. The evidence for each row is in its
fourth column and is explained below.

## How the code is named

| Evidence | Meaning | Application | Runtime |
|---|---|---:|---:|
| `fid` | Ghidra's Function ID matched the bytes against its Visual Studio 2012 to 2019 x64 databases during the default analysis of the export. The match lies in a cluster of at least three matches | 188 | 432 |
| `fid-conflict` | The same, but several library functions share the bytes (`FID_conflict:` in Ghidra). The label is one of them | 9 | 24 |
| `link-order` | No match, but the function lies in a run of at most 16 unnamed functions next to library functions. It calls only library code, or functions that the matched library code calls. The linker keeps each object file's code together, so the run belongs to the same library objects. The labels are `crt_<address>` | 89 | 223 |
| `start-code` | The C runtime's start code, checked by hand (below) | 2 | 3 |
| `steam-stub` | Code in the `.bind` section, which holds the image's entry point | 4 | 0 |
| `handler` | Unnamed `__except` filters and `__finally` blocks, placed with the `Unwind@` funclets. The labels are `funclet_<address>` | 17 | 53 |
| **Library code in all** | | **292 functions, 64,545 bytes** | **682 functions, 144,503 bytes** |

The Function ID databases that matched are those of Visual Studio 2012 (names such as
`_initp_misc_cfltcvt_tab` and `__crtCapturePreviousContext` are from that runtime). *Inferred:*
the game was built with Visual Studio 2012.

## Where it lies

| Binary | Range | Functions | What |
|---|---|---:|---|
| Application | `0x14003b4c4` to `0x1400427c0` | 181 | Start code, heap, locks, `qsort`, locale and multibyte tables |
| Application | `0x1401359cc` to `0x14013d554` | 107 | Maths (`sinf`, `cosf`, `acosf`, `sqrtf`), C++ exception handling, `std::exception`, float formatting (`_cftoe_l`) and parsing (`_atoflt_l`) |
| Application | `0x140206310` to `0x1402077c0` | 4 | The Steam stub in `.bind`. The image's entry point `0x140206310` is here |
| Runtime | `0x18007f72c` to `0x18009e874` | 567 | Start code, locale, `strtol`, `printf` and `wprintf` internals, the name undecorator (`UnDecorator`, `DName`) |
| Runtime | `0x1801285fc` to `0x18012e1a4` | 115 | C++ exception handling, `std::exception`, `std::bad_typeid`, floating-point status |

The game's code lies around the first block. It runs from `0x140001010` to `0x14003b1d0` and
from `0x140042810` to `0x140135990`. The game's static initialisers lie after the `Unwind@`
funclets, from `0x1401501ec`. They register the game's classes and stay game code.

## Start code

| Address | Label | What it does |
|---|---|---|
| `0x14003bb20` | `WinMainCRTStartup` | Sets up the security cookie, then calls `__tmainCRTStartup` |
| `0x14003bb34` | `__tmainCRTStartup` | Reads the startup info, sets up the heap, threads, I/O and environment, runs `_cinit`, then passes the image base and `_wincmdln()` to `WinMain` at `0x140042810` and exits with its result |
| `0x180080eac` | `_DllMainCRTStartup` | The Runtime's entry point. Sets up the security cookie, then calls `__DllMainCRTStartup` |
| `0x180080eec` | `__DllMainCRTStartup` | Calls `_CRT_INIT`, then `DllMain` at `0x1800a0ac0`, and `_CRT_INIT` again when the DLL detaches |
| `0x180080d4c` | `_CRT_INIT` | Runs `_cinit`, `_setargv`, `_mtinit` and their stop steps |

`WinMain` and `DllMain` are game and engine code. They are labelled in `labels/names.tsv`.

The Runtime replaces `operator delete` (`0x180006a10`) with one that frees through
`System::MemoryAllocator::Free`. The C++ exception classes' deleting destructors call it, so
`library.py` accepts it as a callee of library code.

## False matches

Function ID also names five short functions in the Runtime's own code: `0x1800077b0`
(`CAnimationValue::scalar_deleting_destructor`) and `0x180041350`, `0x180044010`,
`0x180045560`, `0x180045dc0` (`CMFCShadowWnd::scalar_deleting_destructor`). Each lies alone
between long runs of engine functions. They are the engine's own deleting destructors, whose
few bytes match MFC's. `library.py` leaves them in the engine's code and reports them with
`--report`.

## C++ standard library

The game uses its own containers (`Collection::Array` and the others in the Runtime) and no
standard containers. The only C++ standard library code is `std::exception`, `std::bad_exception`
and `std::bad_typeid`, which come with the runtime's exception support. None of
the string errors of the standard containers (`vector<T> too long`, `invalid string position`)
is in either binary.

## Running it again

```bash
python3 tools/triage/library.py --report   # writes labels/library.tsv, prints counts and doubts
python3 tools/triage/fnindex.py coverage   # the coverage figure, library code apart
```

Run it after a new Ghidra export. The file changes only when the export changes.

## Open questions

- Function ID ran with Ghidra's default threshold. A run with a lower threshold, or with
  signatures made from Visual Studio 2012's `libcmt.lib`, could name some of the 312 `crt_`
  functions.
- The 26 `qsort` copies inlined as templates in the game's code ([PARITY-AUDIT.md](PARITY-AUDIT.md))
  are counted as game code. They come from the game's own sort calls.
