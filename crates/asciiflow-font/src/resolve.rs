use crate::FontError;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontSource {
    Builtin,
    File,
    Fontconfig,
}

impl FontSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::File => "file",
            Self::Fontconfig => "fontconfig",
        }
    }
}

/// Owned initialization result; contains no Fontconfig or FreeType handles.
#[derive(Clone, Debug)]
pub struct ResolvedFont {
    pub requested: String,
    pub source: FontSource,
    pub path: Option<PathBuf>,
    pub face_index: isize,
    pub name: String,
}

fn error(request: &str, operation: &'static str, detail: impl ToString) -> FontError {
    FontError::Font {
        path: request.into(),
        operation,
        detail: detail.to_string(),
    }
}

fn classify(request: &str) -> FontSource {
    if request == "builtin-8x8" {
        FontSource::Builtin
    } else if Path::new(request).is_absolute()
        || request.starts_with("./")
        || request.starts_with("../")
        || request.contains(['/', '\\'])
        || Path::new(request).exists()
    {
        FontSource::File
    } else {
        FontSource::Fontconfig
    }
}

fn canonical_file(request: &str, path: &Path) -> Result<PathBuf, FontError> {
    let path = path.canonicalize().map_err(|cause| {
        let operation = if cause.kind() == std::io::ErrorKind::NotFound {
            "FontFileMissing"
        } else {
            "FontOpenFailed"
        };
        error(request, operation, format!("{}: {cause}", path.display()))
    })?;
    if !path.is_file() {
        return Err(error(
            request,
            "FontOpenFailed",
            format!("{} is not a file", path.display()),
        ));
    }
    Ok(path)
}

/// Resolve once before atlas construction. FreeType still validates the chosen face.
/// An explicit face override (including zero) is accepted only for file requests.
pub fn resolve_font(request: &str, face_index: Option<u32>) -> Result<ResolvedFont, FontError> {
    let source = classify(request);
    if source != FontSource::File && face_index.is_some() {
        return Err(error(
            request,
            "InvalidFaceIndex",
            "--font-face-index is only valid with an explicit font file; system-font matches select their collection face automatically",
        ));
    }
    match source {
        FontSource::Builtin => Ok(ResolvedFont {
            requested: request.into(),
            source,
            path: None,
            face_index: 0,
            name: request.into(),
        }),
        FontSource::File => {
            let path = canonical_file(request, Path::new(request))?;
            let face_index = isize::try_from(face_index.unwrap_or(0))
                .map_err(|cause| error(request, "InvalidFaceIndex", cause))?;
            Ok(ResolvedFont {
                requested: request.into(),
                source,
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path: Some(path),
                face_index,
            })
        }
        FontSource::Fontconfig => resolve_system(request),
    }
}

#[cfg(any(target_os = "linux", test))]
fn system_match(
    request: &str,
    path: &Path,
    index: i32,
    name: String,
) -> Result<ResolvedFont, FontError> {
    let face_index = isize::try_from(index)
        .ok()
        .filter(|index| *index >= 0)
        .ok_or_else(|| {
            error(
                request,
                "InvalidFaceIndex",
                format!("Fontconfig returned {index}"),
            )
        })?;
    Ok(ResolvedFont {
        requested: request.into(),
        source: FontSource::Fontconfig,
        path: Some(canonical_file(request, path)?),
        face_index,
        name,
    })
}

#[cfg(not(target_os = "linux"))]
fn resolve_system(request: &str) -> Result<ResolvedFont, FontError> {
    Err(error(
        request,
        "FontconfigUnavailable",
        "system font discovery requires Fontconfig on Linux; use builtin-8x8 or an explicit font path",
    ))
}

#[cfg(target_os = "linux")]
fn resolve_system(request: &str) -> Result<ResolvedFont, FontError> {
    use fontconfig_sys::{self as sys, constants::*, statics::LIB_RESULT};
    use std::ffi::{CStr, CString};

    // Never access the sys crate's panicking LIB: failed dlopen is an ordinary error.
    let lib = LIB_RESULT.as_ref().map_err(|cause| {
        error(
            request,
            "FontconfigUnavailable",
            format!("system font discovery requires Fontconfig; use builtin-8x8 or an explicit font path ({cause})"),
        )
    })?;
    if unsafe { (lib.FcInit)() } == 0 {
        return Err(error(
            request,
            "FontconfigUnavailable",
            "system font discovery requires Fontconfig; use builtin-8x8 or an explicit font path (initialization failed)",
        ));
    }
    let pattern_text =
        CString::new(request).map_err(|cause| error(request, "SystemFontNotResolved", cause))?;
    // The safe wrapper does not expose FcNameParse. Own both native patterns directly so
    // full Fontconfig pattern syntax works and each returned reference is destroyed once.
    struct Pattern<'a> {
        ptr: std::ptr::NonNull<sys::FcPattern>,
        lib: &'a sys::Fc,
    }
    impl Drop for Pattern<'_> {
        fn drop(&mut self) {
            unsafe { (self.lib.FcPatternDestroy)(self.ptr.as_ptr()) }
        }
    }
    let own = |ptr| {
        std::ptr::NonNull::new(ptr)
            .map(|ptr| Pattern { ptr, lib })
            .ok_or_else(|| {
                error(
                    request,
                    "SystemFontNotResolved",
                    "Fontconfig returned no pattern",
                )
            })
    };
    let pattern = own(unsafe { (lib.FcNameParse)(pattern_text.as_ptr().cast()) })?;
    if unsafe {
        (lib.FcConfigSubstitute)(
            std::ptr::null_mut(),
            pattern.ptr.as_ptr(),
            sys::FcMatchPattern,
        )
    } == 0
    {
        return Err(error(
            request,
            "SystemFontNotResolved",
            "Fontconfig configuration substitution failed",
        ));
    }
    unsafe { (lib.FcDefaultSubstitute)(pattern.ptr.as_ptr()) };
    let mut result = sys::FcResultNoMatch;
    let matched =
        own(unsafe { (lib.FcFontMatch)(std::ptr::null_mut(), pattern.ptr.as_ptr(), &mut result) })?;
    if result != sys::FcResultMatch {
        return Err(error(
            request,
            "SystemFontNotResolved",
            format!("Fontconfig match result {result}"),
        ));
    }
    let string = |key: &CStr| -> Result<String, FontError> {
        let mut value = std::ptr::null_mut();
        if unsafe { (lib.FcPatternGetString)(matched.ptr.as_ptr(), key.as_ptr(), 0, &mut value) }
            != sys::FcResultMatch
            || value.is_null()
        {
            return Err(error(
                request,
                "SystemFontNotResolved",
                format!("missing Fontconfig {}", key.to_string_lossy()),
            ));
        }
        // Fontconfig owns this NUL-terminated string for the matched pattern's lifetime.
        unsafe { CStr::from_ptr(value.cast()) }
            .to_str()
            .map(str::to_owned)
            .map_err(|cause| error(request, "SystemFontNotResolved", cause))
    };
    let path = string(FC_FILE)?;
    let name = string(FC_FULLNAME).or_else(|_| {
        let family = string(FC_FAMILY)?;
        Ok::<_, FontError>(match string(FC_STYLE) {
            Ok(style) => format!("{family} {style}"),
            Err(_) => family,
        })
    })?;
    let mut index = 0;
    match unsafe {
        (lib.FcPatternGetInteger)(matched.ptr.as_ptr(), FC_INDEX.as_ptr(), 0, &mut index)
    } {
        sys::FcResultMatch => {}
        sys::FcResultNoMatch | sys::FcResultNoId => index = 0,
        result => {
            return Err(error(
                request,
                "InvalidFaceIndex",
                format!("Fontconfig index result {result}"),
            ));
        }
    }
    system_match(request, Path::new(&path), index, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_font_atlas;
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/fonts")
            .join(name)
    }

    #[test]
    fn request_precedence_and_explicit_override() {
        assert_eq!(classify("builtin-8x8"), FontSource::Builtin);
        for path in [
            "/missing/font.ttf",
            "./missing.ttf",
            "../missing.ttf",
            "fonts/font.ttf",
            r"fonts\font.ttf",
        ] {
            assert_eq!(classify(path), FontSource::File);
            assert!(
                resolve_font(path, None)
                    .unwrap_err()
                    .to_string()
                    .contains("FontFileMissing")
            );
        }
        for pattern in ["monospace", "JetBrains Mono", "JetBrains Mono:style=Bold"] {
            assert_eq!(classify(pattern), FontSource::Fontconfig);
            assert!(
                resolve_font(pattern, Some(0))
                    .unwrap_err()
                    .to_string()
                    .contains("InvalidFaceIndex")
            );
        }
        assert!(resolve_font("builtin-8x8", Some(0)).is_err());
        let path = fixture("Inconsolata-Regular.ttf");
        let resolved = resolve_font(path.to_str().unwrap(), Some(0)).unwrap();
        assert_eq!(resolved.path.unwrap(), path.canonicalize().unwrap());
        assert_eq!(resolved.face_index, 0);
    }

    #[test]
    fn match_index_and_final_freetype_validation() {
        let mono = fixture("Inconsolata-Regular.ttf");
        let selected = system_match("test pattern", &mono, 3, "fixture".into()).unwrap();
        assert_eq!(selected.face_index, 3);
        assert!(system_match("test", &mono, -1, "fixture".into()).is_err());
        let selected = system_match("test", &mono, 0, "fixture".into()).unwrap();
        let (atlas, _) = build_font_atlas(
            selected.path.as_ref().unwrap(),
            selected.face_index,
            "@Ag ",
            12,
            20,
        )
        .unwrap();
        let (explicit, _) = build_font_atlas(&mono, 0, "@Ag ", 12, 20).unwrap();
        assert_eq!(atlas.as_r8_slice(), explicit.as_r8_slice());
        let proportional =
            system_match("test", &fixture("Abel-Regular.ttf"), 0, "fixture".into()).unwrap();
        let error = build_font_atlas(
            proportional.path.as_ref().unwrap(),
            proportional.face_index,
            "@Ag ",
            12,
            20,
        )
        .unwrap_err();
        assert!(error.to_string().contains("monospaced"));
    }

    #[test]
    #[ignore = "requires runtime Fontconfig and a system monospace face"]
    fn system_monospace_matches_explicit_atlas() {
        let system = resolve_font("monospace", None).unwrap();
        let path = system.path.as_ref().unwrap();
        let explicit = resolve_font(
            path.to_str().unwrap(),
            Some(system.face_index.try_into().unwrap()),
        )
        .unwrap();
        assert_eq!(
            crate::natural_cell_aspect(path, system.face_index).unwrap(),
            crate::natural_cell_aspect(explicit.path.as_ref().unwrap(), explicit.face_index)
                .unwrap(),
        );
        let (system_atlas, _) = build_font_atlas(path, system.face_index, "@Ag ", 12, 20).unwrap();
        let (file_atlas, _) = build_font_atlas(
            explicit.path.as_ref().unwrap(),
            explicit.face_index,
            "@Ag ",
            12,
            20,
        )
        .unwrap();
        assert_eq!(system_atlas.as_r8_slice(), file_atlas.as_r8_slice());
    }
}
