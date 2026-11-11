//! USB vendor/product identifiers from Windows device interface paths, and a
//! small original table of vendor IDs common on gaming mice.
//!
//! The table is written for ZeroIn from vendor IDs that manufacturers publish
//! on their own devices (USB-IF assignments); it is not derived from the
//! usb.ids database. Entries marked as chip makers supply controllers or
//! receivers to many brands, so the brand on the box may differ.

/// Vendor and product id parsed from a device path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbIds {
    /// USB vendor id.
    pub vid: u16,
    /// Product id.
    pub pid: u16,
    /// The path is a Bluetooth HID device rather than USB.
    pub bluetooth: bool,
}

impl UsbIds {
    /// "046D:C547".
    pub fn hex(&self) -> String {
        format!("{:04X}:{:04X}", self.vid, self.pid)
    }
}

fn hex4(s: &str) -> Option<u16> {
    let h = s.get(..4)?;
    if h.chars().all(|c| c.is_ascii_hexdigit()) {
        u16::from_str_radix(h, 16).ok()
    } else {
        None
    }
}

/// Find `key` ("VID" or "PID") followed by `_XXXX` (USB) or `&SSXXXX`
/// (Bluetooth, `SS` = id source) and return the 4-digit hex id.
fn find_id(upper: &str, key: &str) -> Option<(u16, bool)> {
    let mut from = 0;
    while let Some(pos) = upper[from..].find(key) {
        let at = from + pos + key.len();
        let rest = &upper[at..];
        if let Some(r) = rest.strip_prefix('_') {
            if let Some(v) = hex4(r) {
                return Some((v, false));
            }
        } else if let Some(r) = rest.strip_prefix('&') {
            // Bluetooth: `VID&02046d` (2-digit id source + id) or `PID&b023`.
            let digits = r.chars().take_while(|c| c.is_ascii_hexdigit()).count();
            let id = match digits {
                6 => hex4(&r[2..]),
                4 => hex4(r),
                _ => None,
            };
            if let Some(v) = id {
                return Some((v, true));
            }
        }
        from = at;
    }
    None
}

/// Parse a Raw Input device name such as
/// `\\?\HID#VID_046D&PID_C547&MI_01&Col01#8&...#{378de44c-...}` or a
/// Bluetooth LE path containing `VID&02046d_PID&b023`.
pub fn parse_device_path(path: &str) -> Option<UsbIds> {
    let upper = path.to_ascii_uppercase();
    let (vid, bt1) = find_id(&upper, "VID")?;
    let (pid, bt2) = find_id(&upper, "PID")?;
    let bluetooth =
        bt1 || bt2 || upper.contains("{00001124-") || upper.contains("{00001812-") || upper.contains("BTHENUM") || upper.contains("BTHLE");
    Some(UsbIds { vid, pid, bluetooth })
}

/// What a vendor id says about the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorKind {
    /// The id belongs to the brand that sells the mouse.
    Brand,
    /// A controller, sensor or receiver maker used by many brands.
    ChipMaker,
}

/// Vendor table entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vendor {
    /// Vendor id.
    pub vid: u16,
    /// Display name.
    pub name: &'static str,
    /// Brand or chip maker.
    pub kind: VendorKind,
}

use VendorKind::{Brand, ChipMaker};

/// Original curated table (sorted by id).
pub const VENDORS: &[Vendor] = &[
    Vendor { vid: 0x03F0, name: "HP (HyperX)", kind: Brand },
    Vendor { vid: 0x045E, name: "Microsoft", kind: Brand },
    Vendor { vid: 0x046D, name: "Logitech", kind: Brand },
    Vendor { vid: 0x04D9, name: "Holtek", kind: ChipMaker },
    Vendor { vid: 0x04F2, name: "Chicony", kind: ChipMaker },
    Vendor { vid: 0x05AC, name: "Apple", kind: Brand },
    Vendor { vid: 0x0738, name: "Mad Catz", kind: Brand },
    Vendor { vid: 0x093A, name: "PixArt", kind: ChipMaker },
    Vendor { vid: 0x0951, name: "Kingston (HyperX)", kind: Brand },
    Vendor { vid: 0x0B05, name: "ASUS", kind: Brand },
    Vendor { vid: 0x0C45, name: "Microdia", kind: ChipMaker },
    Vendor { vid: 0x1038, name: "SteelSeries", kind: Brand },
    Vendor { vid: 0x1532, name: "Razer", kind: Brand },
    Vendor { vid: 0x17EF, name: "Lenovo", kind: Brand },
    Vendor { vid: 0x1915, name: "Nordic Semiconductor", kind: ChipMaker },
    Vendor { vid: 0x1AF3, name: "Kingsis (ZOWIE)", kind: Brand },
    Vendor { vid: 0x1B1C, name: "Corsair", kind: Brand },
    Vendor { vid: 0x1BCF, name: "Sunplus Innovation", kind: ChipMaker },
    Vendor { vid: 0x1E7D, name: "ROCCAT", kind: Brand },
    // Seen on a user's Rapoo mouse whose HID strings read "Rapoo" (Nov 2026).
    Vendor { vid: 0x24AE, name: "Rapoo", kind: Brand },
    Vendor { vid: 0x2516, name: "Cooler Master", kind: Brand },
    Vendor { vid: 0x258A, name: "Sinowealth", kind: ChipMaker },
    Vendor { vid: 0x3367, name: "Endgame Gear", kind: Brand },
    Vendor { vid: 0x413C, name: "Dell", kind: Brand },
];

/// Look up a vendor id.
pub fn vendor(vid: u16) -> Option<&'static Vendor> {
    VENDORS.binary_search_by_key(&vid, |v| v.vid).ok().map(|i| &VENDORS[i])
}

/// True when a HID product string looks like a wireless receiver rather than
/// the mouse itself.
pub fn looks_like_receiver(product: &str) -> bool {
    let p = product.to_ascii_lowercase();
    ["receiver", "dongle", "unifying", "lightspeed", "hyperspeed", "2.4g", "wireless adapter"].iter().any(|k| p.contains(k))
}

/// Public page for browsing mouse specifications. The app only opens it in
/// the browser; no data is fetched or bundled.
pub const ELOSHAPES_BROWSE_URL: &str = "https://www.eloshapes.com/mouse/browse";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usb_paths() {
        let p = r"\\?\HID#VID_046D&PID_C547&MI_01&Col01#8&2a3b4c5d&0&0000#{378de44c-56ef-11d1-bc8c-00a0c91405dd}";
        assert_eq!(parse_device_path(p), Some(UsbIds { vid: 0x046D, pid: 0xC547, bluetooth: false }));
        let p = r"\\?\hid#vid_1532&pid_00b6&mi_00#7&1f2e&0&0000#{378de44c-56ef-11d1-bc8c-00a0c91405dd}";
        assert_eq!(parse_device_path(p).unwrap().hex(), "1532:00B6");
    }

    #[test]
    fn parses_bluetooth_paths() {
        let p = r"\\?\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&02046d_PID&b023_REV&0007_d1a2b3c4d5e6#9&abc&0&0000#{378de44c-56ef-11d1-bc8c-00a0c91405dd}";
        assert_eq!(parse_device_path(p), Some(UsbIds { vid: 0x046D, pid: 0xB023, bluetooth: true }));
    }

    #[test]
    fn rejects_paths_without_ids() {
        assert_eq!(parse_device_path(r"\\?\Root#RDP_MOU#0000#{378de44c-56ef-11d1-bc8c-00a0c91405dd}"), None);
        assert_eq!(parse_device_path(r"\\?\HID#VID_04ZZ&PID_1234"), None);
        assert_eq!(parse_device_path(""), None);
        // A malformed first match does not hide a later valid one.
        assert_eq!(parse_device_path("VIDEO#VID_1038&PID_1836").unwrap().vid, 0x1038);
    }

    #[test]
    fn vendor_table_is_sorted_and_looks_up() {
        assert!(VENDORS.windows(2).all(|w| w[0].vid < w[1].vid));
        assert_eq!(vendor(0x1532).unwrap().name, "Razer");
        assert_eq!(vendor(0x258A).unwrap().kind, VendorKind::ChipMaker);
        assert!(vendor(0xFFFF).is_none());
        assert_eq!(vendor(0x24AE).unwrap().name, "Rapoo");
    }

    #[test]
    fn receiver_names() {
        assert!(looks_like_receiver("USB Receiver"));
        assert!(looks_like_receiver("LIGHTSPEED Receiver"));
        assert!(!looks_like_receiver("Razer Viper V3 Pro"));
    }
}
