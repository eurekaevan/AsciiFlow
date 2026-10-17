# Locked Rust dependency license supplements

Git attributes disable line-ending conversion for this directory so that Windows
checkouts preserve the original upstream bytes and their fixed SHA-256 hashes.

The following locked crate archives omit their upstream workspace-level license
texts. These files are copied verbatim from their exact crate-recorded Git commits:

- `accesskit-0.17.1/`: accesskit 0.17.1, accesskit_atspi_common 0.10.1,
  accesskit_consumer 0.26.0, accesskit_unix 0.13.1 and accesskit_windows 0.24.1,
  [AccessKit/accesskit commit 405d578cdfd8496ee79020d24cfaac8c29a8a48f](https://github.com/AccessKit/accesskit/tree/405d578cdfd8496ee79020d24cfaac8c29a8a48f).
  These crates offer MIT or Apache-2.0; the accompanying Chromium BSD notice is
  retained for code derived from Chromium.
- `accesskit_winit-0.23.1/`: accesskit_winit 0.23.1, Apache-2.0 only, from
  the same AccessKit commit.
- `atspi-proxies-0.6.0/`: atspi-proxies 0.6.0, Apache-2.0 or MIT,
  [odilia-app/atspi commit f7976706c3c8cff7117fd50d64b4f14693e81e72](https://github.com/odilia-app/atspi/tree/f7976706c3c8cff7117fd50d64b4f14693e81e72).
- `zbus_xml-4.0.0/`: zbus_xml 4.0.0, MIT,
  [dbus2/zbus commit 25876f399b068749c337374505c35e92dc9e5872](https://github.com/dbus2/zbus/tree/25876f399b068749c337374505c35e92dc9e5872).
- `egui-0.31.1/`: ecolor, eframe, egui, egui-winit, egui_glow, emath and epaint
  0.31.1, [emilk/egui commit 1669e52a7ccfc3489c1b0999b9ed48894a0b3887](https://github.com/emilk/egui/tree/1669e52a7ccfc3489c1b0999b9ed48894a0b3887).
- `profiling-1.0.18/`: profiling 1.0.18,
  [aclysma/profiling commit 8271551172eb6fa4cba47369aedd93790c623df9](https://github.com/aclysma/profiling/tree/8271551172eb6fa4cba47369aedd93790c623df9).

- `clipboard-win-5.4.1/`: clipboard-win 5.4.1,
  [DoumanAsh/clipboard-win commit 3b27cf2bfd1adcfa6e0264eb51c1025ddaf0f342](https://github.com/DoumanAsh/clipboard-win/tree/3b27cf2bfd1adcfa6e0264eb51c1025ddaf0f342).

The egui and profiling upstreams offer MIT or Apache-2.0; clipboard-win uses
BSL-1.0. The collector verifies the exact crate
name, version, license declaration, Git identity and fixed text hashes before
using these supplements. Any changed dependency requires a new review; missing
licenses for other crates continue to fail packaging. The release inventory
records each collected text hash and its upstream URL. No network fetch is
performed while collecting notices.

The embedded epaint_default_fonts font notices are copied from its actual locked
crate, including the Hack, Ubuntu, Noto Emoji and emoji-icon font attributions.
