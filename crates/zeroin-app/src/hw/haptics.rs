//! Haptic feedback on mice.
//!
//! Status in 0.1.0: no backend. Haptic mice are rare and current models expose
//! haptics only through vendor SDKs (e.g. Logitech, Razer) whose licences do not
//! allow redistribution in an MIT project, and no open, documented HID
//! protocol is available to target directly. The interface below is what the
//! app calls; [`detect`] returns `None`, so pulses are skipped and the setting
//! shows as unavailable.
//!
//! TODO: add a backend per device once a vendor publishes an open protocol or
//! a permissively licensed SDK.

/// Pulse strength.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum Pulse {
    /// Target hit.
    Hit,
    /// Missed click.
    Miss,
}

/// A detected haptic device.
#[allow(dead_code)]
pub trait HapticDevice: Send {
    /// Human-readable device name.
    fn name(&self) -> &str;
    /// Fire a pulse. Must not block.
    fn pulse(&mut self, p: Pulse);
}

/// Look for a supported haptic mouse. Always `None` in this release.
pub fn detect() -> Option<Box<dyn HapticDevice>> {
    None
}
