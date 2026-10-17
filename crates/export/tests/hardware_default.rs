//! KODA Pro: an app can make hardware encoding the default of new export settings (its own process
//! only; FilmCraft keeps Off). Its own test binary, since the default is process-wide.

use filmcraft_export::{ExportSettings, HardwareEncoding, set_hardware_encoding_default};

#[test]
fn the_app_picks_the_hardware_encoding_default() {
    assert_eq!(ExportSettings::default().hardware_encoding, HardwareEncoding::Off, "FilmCraft's default");
    set_hardware_encoding_default(true);
    assert_eq!(ExportSettings::default().hardware_encoding, HardwareEncoding::Auto);
    // settings read without the field (built-in presets) take the default; an explicit choice stays
    let mut v = serde_json::to_value(ExportSettings::default()).unwrap();
    v.as_object_mut().unwrap().remove("hardwareEncoding");
    let read: ExportSettings = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(read.hardware_encoding, HardwareEncoding::Auto);
    v["hardwareEncoding"] = serde_json::json!("off");
    let read: ExportSettings = serde_json::from_value(v).unwrap();
    assert_eq!(read.hardware_encoding, HardwareEncoding::Off);
    set_hardware_encoding_default(false);
    assert_eq!(ExportSettings::default().hardware_encoding, HardwareEncoding::Off);
}
