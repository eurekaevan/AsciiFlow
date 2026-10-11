# Build the official Windows x86_64 LGPL ZIP using the x64 MSVC developer environment.
param(
    [Parameter(Mandatory = $true)][string]$FfmpegSourceArchive,
    [Parameter(Mandatory = $true)][string]$NewWorkDirectory
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw 'Windows x86_64 is required'
}

function Invoke-Checked {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command exited with code $LASTEXITCODE" }
}

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$archive = (Resolve-Path $FfmpegSourceArchive).Path
$work = [IO.Path]::GetFullPath($NewWorkDirectory)
$status = & git status --porcelain --untracked-files=normal
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect release checkout' }
if ($status) { throw 'Release packaging requires a clean checkout' }
if (Test-Path $work) { throw 'Release work directory must not exist' }
$releaseDate = $env:RELEASE_DATE
if (-not $releaseDate) { $releaseDate = [DateTime]::UtcNow.ToString('yyyy-MM-dd') }
if ($releaseDate -notmatch '^[0-9]{4}-[0-9]{2}-[0-9]{2}$') { throw 'Release date must use YYYY-MM-DD' }
$null = [DateTime]::ParseExact($releaseDate, 'yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture)
$name = "asciiflow-$releaseDate-windows-x86_64-lgpl"
$dist = Join-Path $work 'dist'
$package = Join-Path $work $name
$null = New-Item -ItemType Directory -Path $dist
$msysRoot = if ($env:MSYS2_ROOT) { $env:MSYS2_ROOT } else { 'C:\msys64' }
$bash = Join-Path $msysRoot 'usr\bin\bash.exe'
$cygpath = Join-Path $msysRoot 'usr\bin\cygpath.exe'
# Pass paths as arguments, rather than interpolating them into shell source.
$recipeUnix = & $cygpath -u (Join-Path $root 'third_party\ffmpeg\scripts\build-windows-lgpl.sh')
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve FFmpeg recipe for MSYS2' }
$archiveUnix = & $cygpath -u $archive
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve source archive for MSYS2' }
$ffmpegUnix = & $cygpath -u (Join-Path $work 'ffmpeg')
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve FFmpeg work directory for MSYS2' }
Invoke-Checked $bash @('--noprofile', '--norc', $recipeUnix, $archiveUnix, $ffmpegUnix)
$env:FFMPEG_DIR = Join-Path $work 'ffmpeg\sysroot'
$originalPath = $env:PATH
$env:PATH = "$(Join-Path $env:FFMPEG_DIR 'bin');$originalPath"
$env:CARGO_TARGET_DIR = Join-Path $work 'target'
$env:ASCIIFLOW_RELEASE_DATE = $releaseDate
$target = 'x86_64-pc-windows-msvc'

# Inspect the actual libraries and encoder options, without requesting a GPU.
$profileCheck = @'
import ctypes, hashlib, json, os, pathlib, subprocess, sys
prefix, output = map(pathlib.Path, sys.argv[1:])
ffmpeg = prefix / 'bin/ffmpeg.exe'
def run(*args):
    result = subprocess.run([str(ffmpeg), *args], text=True, capture_output=True, check=True, timeout=60)
    return result.stdout + result.stderr
receipt = {'platform': 'windows-x86_64', 'status': 'RUNNING', 'libraries': {}}
with os.add_dll_directory(str(prefix / 'bin')):
    for component in ('avcodec', 'avformat', 'avutil', 'swscale'):
        paths = list((prefix / 'bin').glob(component + '-*.dll'))
        assert len(paths) == 1, (component, paths)
        path = paths[0]
        library = ctypes.CDLL(str(path))
        license = getattr(library, component + '_license')
        license.restype = ctypes.c_char_p
        actual = license().decode()
        assert actual == 'LGPL version 2.1 or later', (component, actual)
        receipt['libraries'][path.name] = {'license': actual, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
receipt['license'] = run('-L')
receipt['buildconf'] = run('-buildconf')
for category, option, required in (
    ('decoders', '-decoders', ('h264', 'hevc', 'aac')),
    ('encoders', '-encoders', ('h264_mf',)),
    ('demuxers', '-demuxers', ('mov',)),
    ('muxers', '-muxers', ('mp4', 'null')),
    ('protocols', '-protocols', ('file',)),
    ('pixel_formats', '-pix_fmts', ('nv12', 'yuv420p')),
):
    actual = run('-hide_banner', option)
    tokens = {name for token in actual.split() for name in token.split(',')}
    assert set(required) <= tokens, (category, required, actual)
    receipt[category] = actual
assert not any(name in receipt['encoders'] for name in ('libx264', 'libx265', 'libfdk', 'libxvid'))
receipt['mf_options'] = run('-hide_banner', '-h', 'encoder=h264_mf')
assert 'hw_encoding' in receipt['mf_options'], receipt['mf_options']
receipt['hardware_execution'] = 'NOT_VERIFIED; hosted runner profile/startup checks do not exercise an encoder device'
receipt['status'] = 'PASS'
output.write_text(json.dumps(receipt, indent=2) + '\n')
'@
Invoke-Checked python @('-c', $profileCheck, $env:FFMPEG_DIR, (Join-Path $work 'ffmpeg-profile.json'))
# freetype-sys 0.23.0's bundled libpng refers to an unpackaged relative zlib
# directory. Use the locked libz-sys headers without modifying either crate.
$metadata = & cargo metadata --locked --format-version 1 --filter-platform $target
if ($LASTEXITCODE -ne 0) { throw 'Cannot locate locked native font dependencies' }
$zlib = @((($metadata -join "`n") | ConvertFrom-Json).packages | Where-Object { $_.name -eq 'libz-sys' })
if ($zlib.Count -ne 1) { throw 'Expected one locked libz-sys package' }
$zlibHeaders = Join-Path (Split-Path -Parent $zlib[0].manifest_path) 'src\zlib'
foreach ($header in @('zlib.h', 'zconf.h')) {
    if (-not (Test-Path (Join-Path $zlibHeaders $header))) { throw "Missing locked zlib header: $header" }
}
$zlibHeaders = $zlibHeaders.Replace('\', '/')
$env:CFLAGS_x86_64_pc_windows_msvc = "$($env:CFLAGS_x86_64_pc_windows_msvc) /I`"$zlibHeaders`"".Trim()
$env:CC_SHELL_ESCAPED_FLAGS = '1' # cc must preserve a quoted header path containing spaces.
Invoke-Checked cargo @('build', '--locked', '--release', '--target', $target, '-p', 'asciiflow-cli', '--features', 'lgpl-prebuilt')
# This device-free test also compiles the opt-in Windows hardware round-trip gate.
Invoke-Checked cargo @('test', '--locked', '--lib', '--target', $target, '-p', 'asciiflow-media', '--features', 'lgpl-prebuilt', 'ffmpeg::encoder::audio_regression_tests::lgpl_profile_rejects_software_without_generic_h264_fallback', '--', '--exact')
$null = New-Item -ItemType Directory -Path $package, (Join-Path $package 'sources'), (Join-Path $package 'notices\rust-std'), (Join-Path $package 'build')
Copy-Item (Join-Path $env:CARGO_TARGET_DIR "$target\release\asciiflow.exe") $package
foreach ($component in @('avcodec', 'avformat', 'avutil', 'swscale')) {
    $libraries = @(Get-ChildItem (Join-Path $env:FFMPEG_DIR "bin\$component-*.dll"))
    if ($libraries.Count -ne 1) { throw "Expected one $component DLL" }
    Copy-Item $libraries[0].FullName $package
}
Copy-Item LICENSE, THIRD_PARTY_NOTICES.md, docs/production-support.md $package
Copy-Item docs/releases.md (Join-Path $package 'README.md')
Invoke-Checked python @('scripts/collect-release-notices.py', (Join-Path $package 'notices\rust-dependencies'), '--target', $target)
$rustSysroot = & rustc --print sysroot
if ($LASTEXITCODE -ne 0) { throw 'Cannot locate Rust standard-library notices' }
Copy-Item (Join-Path $rustSysroot 'share\doc\rust\COPYRIGHT-library.html') (Join-Path $package 'notices\rust-std')
Copy-Item (Join-Path $rustSysroot 'share\doc\rust\licenses') (Join-Path $package 'notices\rust-std') -Recurse
Copy-Item $archive (Join-Path $package 'sources\ffmpeg-8.1.3.tar.xz')
Copy-Item (Join-Path $work 'ffmpeg\ffmpeg-8.1.3\COPYING.LGPLv2.1') (Join-Path $package 'notices')
Invoke-Checked git @('archive', '--format=tar.gz', "--prefix=asciiflow-$releaseDate/", '-o', (Join-Path $package "sources\asciiflow-$releaseDate.tar.gz"), 'HEAD')
Copy-Item third_party/ffmpeg/scripts/build-windows-lgpl.sh, scripts/package-windows-release.ps1 (Join-Path $package 'build')
foreach ($file in @('configure-command.txt', 'compiler.txt', 'source.sha256', 'license.txt', 'buildconf.txt')) {
    Copy-Item (Join-Path $work "ffmpeg\$file") (Join-Path $package 'build')
}
Copy-Item (Join-Path $work 'ffmpeg-profile.json') (Join-Path $package 'build')
$environment = @(& rustc --version --verbose; & cargo --version; & git rev-parse HEAD; [Environment]::OSVersion.ToString(); "CFLAGS_x86_64_pc_windows_msvc=$env:CFLAGS_x86_64_pc_windows_msvc"; "CC_SHELL_ESCAPED_FLAGS=$env:CC_SHELL_ESCAPED_FLAGS")
$environment | Set-Content (Join-Path $package 'build\environment.txt')

# Windows embeds the bundled FreeType/libpng/zlib code; retain its native notices and exact sources too.
$nativeNotices = @'
import hashlib, json, os, pathlib, shutil, sys, tomllib
package = pathlib.Path(sys.argv[1])
cargo_home = pathlib.Path(os.environ.get('CARGO_HOME', pathlib.Path.home() / '.cargo'))
locked = tomllib.loads(pathlib.Path('Cargo.lock').read_text())['package']
records = []
for name in ('freetype-sys', 'libz-sys'):
    matches = [entry for entry in locked if entry['name'] == name]
    assert len(matches) == 1, matches
    entry = matches[0]
    stem = name + '-' + entry['version']
    sources = list((cargo_home / 'registry/src').glob('*/' + stem))
    archives = list((cargo_home / 'registry/cache').glob('*/' + stem + '.crate'))
    assert len(sources) == len(archives) == 1, (sources, archives)
    assert hashlib.sha256(archives[0].read_bytes()).hexdigest() == entry['checksum'], stem
    shutil.copyfile(archives[0], package / 'sources' / archives[0].name)
    selected = {
        'freetype-sys': ('freetype2/LICENSE.TXT', 'freetype2/docs/FTL.TXT',
                         'freetype2/src/bdf/README', 'freetype2/src/pcf/README',
                         'freetype2/include/freetype/internal/fthash.h',
                         'freetype2/src/gzip/zlib.h', 'freetype2/src/autofit/ft-hb.c',
                         'freetype2/subprojects/dlg/LICENSE', 'libpng/LICENSE'),
        'libz-sys': ('src/zlib/LICENSE',),
    }[name]
    files = []
    for relative in selected:
        source = sources[0] / relative
        destination = package / 'notices/native' / stem / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        files.append({'path': destination.relative_to(package).as_posix(), 'sha256': hashlib.sha256(destination.read_bytes()).hexdigest()})
    records.append({'crate': stem, 'source_sha256': entry['checksum'], 'notices': files})
(package / 'notices/native/inventory.json').write_text(json.dumps(records, indent=2) + '\n')
(package / 'notices/native/README.md').write_text(
    'This software is based in part on the work of the FreeType Team.\n'
    'Bundled FreeType uses the FreeType License (FTL), with libpng and zlib under their upstream licenses.\n'
    'Exact locked crate archives in ../../sources contain all bundled native sources and notices.\n')
'@
Invoke-Checked python @('-c', $nativeNotices, $package)
@'
# Corresponding source

FFmpeg 8.1.3 is unmodified; its exact archive, LGPL text and build recipe are included.
The application source archive contains the exact compiled commit and Cargo.lock.
The locked freetype-sys and libz-sys crate archives contain the embedded native
FreeType, libpng and zlib source. See ../notices/native for their attribution.
Build from a clean checkout with x64 MSVC, Windows SDK, MSYS2 make/NASM, LLVM,
CMake, Python and Rust with rust-docs, using the enclosed release recipes.

The adjacent FFmpeg DLLs are dynamically linked and can be replaced by compatible
modified LGPL builds. No EULA restricts modification or reverse engineering for
debugging such modifications. Windows, Media Foundation, graphics drivers and
the Microsoft Visual C++ 2015-2022 x64 Redistributable are host dependencies.
'@ | Set-Content (Join-Path $package 'sources\SOURCE.md')

$zip = Join-Path $dist "$name.zip"
Compress-Archive -Path $package -DestinationPath $zip -CompressionLevel Optimal
$relocated = Join-Path $work 'relocated'
Expand-Archive -Path $zip -DestinationPath $relocated
$relocatedPackage = Join-Path $relocated $name
$env:PATH = $originalPath
Remove-Item Env:FFMPEG_DIR
# Check the complete import closure, allowing only package DLLs and Windows/VC system DLLs.
$runtimeCheck = @'
import hashlib, json, os, pathlib, re, subprocess, sys
package, output = map(pathlib.Path, sys.argv[1:])
system = pathlib.Path(os.environ['SystemRoot']) / 'System32'
receipt = {}
for binary in sorted([*package.glob('*.exe'), *package.glob('*.dll')]):
    listing = subprocess.check_output(['dumpbin.exe', '/DEPENDENTS', str(binary)], text=True)
    dependencies = re.findall(r'^\s+([\w.+-]+\.dll)\s*$', listing, re.MULTILINE | re.IGNORECASE)
    assert dependencies, (binary, listing)
    for dependency in dependencies:
        assert not any(name in dependency.lower() for name in ('libx264', 'libx265', 'libfdk', 'libxvid', 'msys-')), dependency
        if dependency.lower().startswith(('api-ms-win-', 'ext-ms-win-')):
            continue  # API-set contracts are resolved by the Windows loader.
        assert (package / dependency).is_file() or (system / dependency).is_file(), (binary.name, dependency)
    receipt[binary.name] = {'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'imports': dependencies, 'dumpbin': listing}
output.write_text(json.dumps(receipt, indent=2) + '\n')
'@
Invoke-Checked python @('-c', $runtimeCheck, $relocatedPackage, (Join-Path $work 'runtime-check.json'))
$executable = Join-Path $relocatedPackage 'asciiflow.exe'
$version = & $executable --version
if ($LASTEXITCODE -ne 0 -or $version -ne "asciiflow $releaseDate") { throw "Unexpected packaged build label: $version" }
& $executable --help | Set-Content (Join-Path $work 'help.txt')
if ($LASTEXITCODE -ne 0) { throw 'Relocated archive failed to start' }
$rejected = Join-Path $work 'rejected.mp4'
$rejection = & $executable tests/fixtures/media/no-audio.mp4 $rejected --backend cpu --decode software --encode software --audio none --width 16 --no-progress 2>&1
$rejection | Set-Content (Join-Path $work 'software-rejection.txt')
if ($LASTEXITCODE -eq 0 -or ($rejection -join "`n") -notmatch 'excluded from the official LGPL prebuilt' -or (Test-Path $rejected)) {
    throw 'LGPL package did not reject software encoding before creating output'
}
$mfDiagnostic = Join-Path $work 'mf-plan-probe.json'
$mfProbe = & $executable tests/fixtures/media/no-audio.mp4 --backend cpu --decode software --encode mf --audio none --width 16 --explain-plan --diagnostic-report $mfDiagnostic --no-progress 2>&1
$mfExit = $LASTEXITCODE
$mfProbe | Set-Content (Join-Path $work 'mf-plan-probe.txt')
$planCheck = @'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text())
exit_code = int(sys.argv[2])
assert report['mode'] == 'inspection', report
media = report['capabilities']['media']
assert media['software_decode']['state'] == 'Supported', report
assert media['software_encode']['state'] == 'Unsupported', report
mf = media['media_foundation_h264_encode']
if mf['state'] == 'Supported':
    assert exit_code == 0 and report['status'] == 'success', report
    assert report['selected_plan']['encode'] == 'MediaFoundation', report
else:
    assert mf['state'] == 'Unsupported' and mf['reason'], report
    assert exit_code != 0 and report['status'] == 'failure', report
    assert report['selected_plan'] is None and report['failure']['stage'] == 'Planning', report
'@
Invoke-Checked python @('-c', $planCheck, $mfDiagnostic, [string]$mfExit)
Copy-Item (Join-Path $work 'runtime-check.json'), (Join-Path $work 'software-rejection.txt'), $mfDiagnostic, (Join-Path $work 'mf-plan-probe.txt') $dist
@"
Windows x86_64 official LGPL package. Release date: $releaseDate.

Includes FFmpeg 8.1.3 DLLs, exact corresponding sources and third-party notices.
Requires Windows 10/11 Media Foundation, a compatible H.264 hardware encoder and
the Microsoft Visual C++ 2015-2022 x64 Redistributable. Windows N editions need
the Media Feature Pack. Use --decode software --encode mf; software H.264
encoding and software AV1 decoding are unavailable in this package.

CPU and Vulkan host processing are supported; hardware decoding and zero-copy
GPU interop are unavailable in the first Windows package. Vulkan requires a
compatible driver. See README.md and production-support.md for supported routes.

CI checks the LGPL/codec profile, DLL imports, software-encoding rejection,
the explicit MF plan/probe result and relocated startup. An unavailable MF GPU
must produce a clear planning failure. Real video encoding is not verified here.
SHA256SUMS covers the release ZIP.
"@ | Set-Content (Join-Path $dist 'RELEASE_NOTES.md')
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $name.zip" | Set-Content (Join-Path $dist 'SHA256SUMS')
Write-Output "Verified release assets: $dist"
