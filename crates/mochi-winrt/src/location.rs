use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator};
use windows_core::Interface;

pub fn locate_once() -> Option<(f64, f64)> {
    let status = Geolocator::RequestAccessAsync().ok()?.join().ok()?;
    if status != GeolocationAccessStatus::Allowed {
        return None;
    }
    let locator = Geolocator::new().ok()?;
    let acc: windows::Foundation::IReference<u32> =
        windows::Foundation::PropertyValue::CreateUInt32(5000)
            .ok()?
            .cast()
            .ok()?;
    let _ = locator.SetDesiredAccuracyInMeters(&acc);
    let pos = locator.GetGeopositionAsync().ok()?.join().ok()?;
    let pt = pos.Coordinate().ok()?.Point().ok()?.Position().ok()?;
    let lat = mochi_weather::port::coarse_coordinate(pt.Latitude);
    let lon = mochi_weather::port::coarse_coordinate(pt.Longitude);
    Some((lat, lon))
}
