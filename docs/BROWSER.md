# Browser verification

Use `env -u NO_COLOR tools/dev-web.sh --release` and open
`http://127.0.0.1:8731/` with **no `?data=` query**. `NO_COLOR=1` is rejected by
Trunk 0.21.14; unsetting it avoids that CLI error. Local server ports must be
available. During concurrent source editing, add `--no-autoreload` so a rebuild
does not discard the loaded files and audio probe.

## Repeatable checks

1. Choose all four WinData packages together, in any order. The loading panel
   disappears and the viewer lists 271 models, 406 textures, 72 sounds and 55
   dialogs from 5,832 unique resources. Package priorities determine precedence.
2. Reload. Cancel the chooser: the panel stays available. Select an unrelated
   file: the panel requests `.pkg` files. Select an invalid `.pkg`: a parse error
   appears and the chooser remains enabled. Select the same package again after
   fixing it: the change handler runs again because the input value was cleared.
3. Paste [tools/browser-smoke.js](../tools/browser-smoke.js) into DevTools before
   the first Play. The optional `await browserSmoke.selectPackages()` puts real
   browser `File` objects through the file input's `change` handler using the
   local proxy. This bypasses the OS chooser and never calls the query loader.
4. Click Audio, select `Audio/Animal/Chicken_version0`, then Play. Run
   `browserSmoke.audio()` to inspect the decoded mono buffer and running context.
   Run `browserSmoke.sampleOutput(30)` and press Stop while it samples: the live
   signal must become zero, with `stopped: 1` and `ended: 1` in `audio()`.
5. Repeat for streamed stereo music (`Audio/Music/Awesomer_music`) and looping
   environmental audio (`Audio/Effects/Environment_forest`). Playing another
   entry stops the previous source. Stop must also silence looping audio.

The probe observes the viewer's actual AudioBufferSourceNode and AudioContext;
it does not substitute audio decoding or playback. `restore()` restores the
AudioContext constructor. Reload to discard all instrumentation.

## Recorded Chrome run (2026-10-14)

The Rust/WASM release viewer ran with WebGPU in local Chrome. Four real `File`
objects (patch1, data0, patch0, data1) were assigned through `DataTransfer` to
the native file input, whose normal change callback loaded all 5,832 resources.
The URL stayed `/`, without `?data=`.

| Sound | Browser buffer | Playback observations |
| --- | --- | --- |
| Chicken version 0 | 208,896 frames, mono, 44,100 Hz, 4.736871 s | PCM RMS 0.042877, peak 0.553284; running context; live analyser RMS 0.012838 before Stop, 0 after Stop; ended event fired |
| Awesomer music | 9,066,496 frames per channel, stereo, 44,100 Hz, 205.589478 s | Streamed `.str` decoded in browser; channel RMS 0.081489 / 0.074832, peaks 0.602234 / 0.619110; live signal nonzero; Stop produced zero and ended |
| Environment forest | 876,544 frames per channel, stereo, 48,000 Hz, 18.261333 s | `AudioBufferSourceNode.loop === true`; live RMS 0.006641 / 0.008992 followed by zero on Stop; ended fired |

The context ran at the browser output rate of 48,000 Hz, with browser resampling
from the 44,100 Hz buffers. No viewer errors occurred during these playback
checks. Chrome emitted its unrelated preload-integrity warning.

The final integrated WASM release was rebuilt and these three playback checks
were repeated after the audio cleanup changes. Selecting the packages again
closed the old AudioContext. Separate picker checks confirmed cancellation
preserved the panel, unrelated files were rejected, invalid packages reported
a parse error with the input enabled, the same invalid package triggered a new
load on reselection, and an injected File read rejection displayed its filename
and re-enabled the chooser. `node --check tools/browser-smoke.js` passed.

The integrated dialog preview was also exercised in Chrome: Options switched
between its Video and Audio pages, clicking the Music slider set its value to
0.60, and a trusted ArrowRight key changed it to 0.70. NewGame's town-name edit
accepted trusted `B`, `a`, `n` keys and Backspace. The selected widget names
(`tabOptions`, `sliderMusic`, `editName`) were reported through dialog events.
After the preview layout fix, the first screenshots immediately after selecting
Options and NewGame matched their respective screenshots after a subsequent
pointer-move redraw byte for byte; both dialogs were centered on the first frame.

The automation's native file-upload tool rejected this workspace path, so OS
chooser navigation was not exercised. The browser `File.arrayBuffer()` read,
multiple-file change callback, package parsing and rendering were exercised.
Canvas controls were driven through pointer events after a trusted key event
established user activation. Signal checks establish Web Audio graph output;
they do not establish subjective speaker sound quality or other-browser support.
