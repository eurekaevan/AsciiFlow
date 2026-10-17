# Graphical interface user guide

AsciiFlow converts a local video into color or monochrome ASCII video. The
graphical application uses English labels and handles one file at a time. It
shares the CLI's conversion pipeline and support limits; see the
[production support contract](production-support.md) for exact media and
hardware conditions.

## Launch the application

Extract the whole release archive before launching. Run `./asciiflow-gui` on
Linux or open `asciiflow-gui.exe` on Windows. Keep the Linux `lib/` directory or
the Windows FFmpeg DLLs beside the executables. Package installation and native
dependencies are described in the release package's `README.md`.

The AF window icon is embedded in the application. Release archives also contain
SVG, PNG and ICO assets; the ICO can be used for Windows shortcuts. The Windows
executable-file icon is not yet embedded as a resource.

A working desktop and OpenGL driver are required. Linux file dialogs use the
desktop portal. When `DISPLAY` is available, the application prefers
X11/XWayland so files can be dragged from the file manager. On native Wayland,
use **Choose video…** instead; that window backend does not support file drops.

For a source build, run:

```sh
cargo run --locked -p asciiflow-cli --features gui --bin asciiflow-gui
```

## Convert a video

1. Click **Choose video…**, or drop a video into a window that supports drops.
   Check the displayed filename; hover over it to see the full path.
2. Click **Save output as…** and choose an MP4 destination. Selecting a new
   input clears the previous destination and preview, so choose a destination
   again for each new input.
3. Under **Appearance**, choose the rendering font, columns, rows, character
   set, color and audio policy. Start with **Standard**, **Rows: Auto**, and
   the built-in font if unsure.
4. Optionally click **Preview first frame**. A preview needs an input but does
   not require an output destination. If settings change, refresh the preview.
5. Click **Start conversion**. If the destination exists, confirm replacement.
   The existing file is replaced only after successful conversion.
6. Wait for the completion message, then click **Open file location** to reveal
   the saved output in the file manager.

The selector includes MP4, MOV, M4V, MKV, AVI and WebM. The tested input extension
covers MP4, MKV, AVI and WebM; appearing in the selector does not qualify every
container, codec or color combination. Output is always MP4. There is no batch
queue or video playback in the GUI.

## Appearance controls

| Control | What it does |
| --- | --- |
| **Rendering font** | Selects the built-in 8×8 font or an installed scalable font. Type in the selector's search field to filter the list. |
| **Browse font…** | Selects a font file directly, including a font not found by the system scan. |
| **Columns** | Sets the number of character columns, rather than the output video width in pixels. More columns give a finer character grid. |
| **Rows** | Sets the number of character rows. Zero displays **Auto**, which derives rows from the source aspect and the selected font's reference cell aspect. |
| **Retain source colors** | Enables color rendering. Clear it for monochrome output. |
| **Character set** | Chooses **Standard**, **Detailed** or **Custom**. Standard and Detailed use the same presets as the CLI. |
| **Characters** | Edits the Custom sequence, ordered from sparse to dense. Switching to a preset preserves the custom text for when Custom is selected again. Hover over the character-set selector to inspect the effective sequence and count. |
| **Audio** | Chooses which compressed audio streams to copy; it does not transcode audio. |

**Audio** has three choices:

- **Auto** copies streams accepted by the actual MP4 muxer. The completion
  message lists skipped streams with their codec and reason.
- **Require passthrough** requires every input audio stream to be compatible;
  conversion fails if any stream cannot be copied.
- **No audio** produces video without audio tracks.

Copied audio still has to satisfy the existing video timeline rules. Player
compatibility for Vorbis or Opus in MP4 can be more limited than for AAC.

### Rendering fonts and interface fonts

Rendering fonts are scanned in the background at startup. Linux uses
Fontconfig; Windows scans its standard system and user Fonts directories.
Fonts registered elsewhere on Windows can be selected with **Browse font…**.
Unreadable or incompatible faces are omitted. If scanning fails, the error is
shown and the built-in font and file browser remain available.

Both scalable monospaced and proportional fonts are accepted. Characters use
a fixed grid, a common size and baseline; proportional glyphs are centered
horizontally without stretching. Automatic rows use ASCII M advance (or the
global maximum advance if M is absent) and ascender minus descender as the
reference cell. Missing glyphs or unsupported bitmap formats fail explicitly;
there is no rendering-font substitution, shaping or ligature support.

The interface font is separate from the rendering font. At startup it reads
the GNOME interface font, KDE's `kdeglobals`, or the Windows system message
font. Other Linux desktops are not yet qualified. If discovery fails, the
default interface font remains and a diagnostic is reported. Installed CJK
fonts provide fallback for filenames; no font is bundled specifically for
Chinese filenames. KDE and Windows font weight/style settings are not
currently mirrored.

## Preview

**Preview first frame** decodes and renders the first frame on the CPU with
the selected appearance settings, before output encoding. It supports 8-bit
BT.709 SDR input only. The displayed image is bounded to 1024 pixels on its
longest side and fitted to the preview canvas.

Preview does not play the video, include audio, or test the selected encoder
and hardware route. A successful preview therefore does not guarantee that
conversion will succeed. Conversely, an unsupported HDR or 10-bit preview does
not imply that its qualified hardware conversion path is unavailable.
The conversion uses the current settings, even if the displayed preview is old.

## Advanced processing

Expand **Advanced processing** when an explicit processing or output choice is
needed. Defaults are **Auto** processing and encoder, **H.264**, **8 bit**, and
**Preserve** dynamic range.

| Control | Choices and limits |
| --- | --- |
| **Processing** | Auto, CPU or Vulkan. CPU controls ASCII processing; it does not mean software video encoding. |
| **Encoder** | Auto, VAAPI (Linux), Media Foundation (Windows), or Software (custom build). Official LGPL packages require supported hardware encoding and exclude software H.264 encoding. |
| **Video codec** | H.264, HEVC or AV1. Availability depends on the platform, input and actual encoder. The initial Windows route provides H.264 only. |
| **Bit depth** | 8 bit or 10 bit. H.264 10-bit output is unsupported. HEVC/AV1 10-bit paths require qualified Linux hardware and compatible input. |
| **Dynamic range** | Preserve or Convert to SDR. Preserve is the default; conversion to SDR is explicit and uses the qualified Linux PQ-to-SDR path, with its fixed source-domain limits. |

The list exposes choices; it is not a claim that every combination works.
HEVC, AV1 and HDR need their qualified Linux paths. Windows uses software
decoding with hardware-only Media Foundation H.264 8-bit SDR encoding, and CPU
or Vulkan host processing. Missing hardware or incompatible explicit settings
produce an error rather than a silent software encoder fallback.

## Progress, cancellation and completed output

The bottom toolbar shows processed frames and, when known, the total. If the
total is unknown, it reports the processed count without inventing a percentage.
During conversion it also shows elapsed time and average processed frames per
second.

Click **Cancel** to request cancellation. The interface reports cancellation
while the worker stops. Closing the window during a job also requests
cancellation before closing. Failure or cancellation before the final output
commit preserves an existing destination and removes staging output; the
completion message identifies successful output.

**Open file location** refers to the last successfully committed output, even
if the next destination has changed or a later job was cancelled. Linux asks
the file manager to select the file, falling back to opening its parent folder.
Windows asks Explorer to select it. Errors opening the location appear in the
status area.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| The application will not start | Extract the full archive, retain its libraries, and check the desktop, OpenGL driver and package dependencies in the release README. |
| Dragging a file does nothing | On native Wayland use **Choose video…**. Linux drag-and-drop uses X11/XWayland when available. |
| The file dialog does not open | Check that the Linux desktop portal is installed and working for the current session. |
| A Chinese filename displays as boxes | Install an appropriate CJK font, make it available to Fontconfig on Linux, and restart the application. |
| A desired font is absent | Search by family/style or use **Browse font…**. The scan includes only usable scalable faces. |
| The font is missing a character | Select a font that contains every character in the chosen sequence, or edit the Custom sequence. |
| Preview rejects HDR or 10-bit input | Preview is CPU 8-bit BT.709 SDR only. Check conversion eligibility separately in the support contract. |
| Start conversion is disabled | Choose both an input and an MP4 output; wait for any current job to finish. |
| Conversion reports no suitable encoder | Check the hardware encoder and vendor driver. Official LGPL packages cannot use software H.264 as a fallback. |
| A successful output has no expected audio | Check Audio policy and the completion warnings. Auto may skip incompatible streams; use Require passthrough to make that a failure. |

For a bug report, retain the full visible error, input codec/container details,
settings, application build identity, OS and graphics/media driver versions.
Remove private paths or media details before sharing. Desktop acceptance
records remain in the release guide; they do not qualify all Windows devices,
Linux desktops or long repeated GUI sessions.
In particular, short repeated-conversion tests do not qualify persistent
multi-job process memory; the retained long-soak results cover single-job CLI
processes.
