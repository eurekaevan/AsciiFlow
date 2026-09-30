//! Internal C-4A surface bridge. No encoder submission or production planner.
use crate::DrmPrimeMapping;
use asciiflow_core::{Error, PixelFormat, Result};
use asciiflow_media::VaapiEncoderFrame;
use asciiflow_vulkan::{SdrPackFault, SdrPackFormat, SdrPackOutput, VulkanHdrToSdrQualification};

/// Keep the actual VAAPI allocation, not just its DMA-BUF import, alive until
/// Vulkan returns foreign ownership. Unknown completion quarantines the mapping
/// and surface: a bounded qualification failure must never authorize pool reuse.
pub fn pack_completed_sdr_surface(
    slot: &mut VulkanHdrToSdrQualification,
    surface: VaapiEncoderFrame,
    fault: Option<SdrPackFault>,
) -> Result<(VaapiEncoderFrame, SdrPackOutput)> {
    let format = surface.desc().format;
    let pack_format = match format {
        PixelFormat::Nv12 => SdrPackFormat::Nv12,
        PixelFormat::P010Le => SdrPackFormat::P010,
    };
    let mapping = DrmPrimeMapping::map_direct_write(surface)?;
    let planes = mapping.duplicate_external_planes_for(format)?;
    match slot.pack_completed_sdr(pack_format, Some(planes), fault) {
        Ok(output) => Ok((mapping.into_source(), output)),
        Err(error) => {
            if slot.is_device_abandoned() {
                std::mem::forget(mapping);
                return Err(Error::Vulkan(format!(
                    "C-4A completion unknown: VAAPI surface quarantined, device abandoned: {error}"
                )));
            }
            Err(error)
        }
    }
}
