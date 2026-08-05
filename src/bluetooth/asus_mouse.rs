use super::info::{BT_INFO_MAP, BluetoothInfo, BluetoothType};
use crate::{PROXY, UserEvent, notify::NotifyEvent};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::{Context, Result, anyhow};
use dashmap::DashMap;
use hidapi::{DeviceInfo, HidApi, HidDevice};
use log::{info, warn};

const ASUS_VENDOR_ID: u16 = 0x0B05;
const STRIX_IMPACT_II_WIRELESS_PRODUCT_ID: u16 = 0x1949;
const ASUS_DEVICE_ADDRESS_PREFIX: u64 = 0xA5 << 56;

#[derive(Clone, Debug)]
struct AsusMouseModel {
    vendor_id: u16,
    product_id: u16,
    path_hint: &'static str,
    name: &'static str,
    parser: BatteryParser,
}

#[derive(Clone, Copy, Debug)]
enum BatteryParser {
    QuarterSteps,
}

#[derive(Clone, Debug)]
struct AsusMouseState {
    name: String,
    battery: u8,
    path: String,
    vendor_id: u16,
    product_id: u16,
}

pub async fn get_asus_mouse_devices_info() -> Result<DashMap<u64, BluetoothInfo>> {
    tokio::task::spawn_blocking(find_asus_mouse_devices_info)
        .await?
        .context("Failed to enumerate ASUS mouse devices")
}

fn supported_models() -> &'static [AsusMouseModel] {
    &[AsusMouseModel {
        vendor_id: ASUS_VENDOR_ID,
        product_id: STRIX_IMPACT_II_WIRELESS_PRODUCT_ID,
        path_hint: "mi_00",
        name: "ROG Strix Impact II Wireless",
        parser: BatteryParser::QuarterSteps,
    }]
}

fn find_asus_mouse_devices_info() -> Result<DashMap<u64, BluetoothInfo>> {
    let map = DashMap::new();

    for state in read_supported_mice()? {
        let address = stable_mouse_address(state.vendor_id, state.product_id, &state.path);
        map.insert(
            address,
            BluetoothInfo {
                name: state.name,
                battery: state.battery,
                battery_display: None,
                status: true,
                address,
                r#type: BluetoothType::AsusHid {
                    path: state.path,
                    vendor_id: state.vendor_id,
                    product_id: state.product_id,
                },
            },
        );
    }

    Ok(map)
}

fn read_supported_mice() -> Result<Vec<AsusMouseState>> {
    let api = HidApi::new().context("Failed to initialize HID API")?;
    let mut states = Vec::new();

    for model in supported_models() {
        for device in api
            .device_list()
            .filter(|device| matches_model(device, model))
        {
            match read_mouse_state(&api, device, model) {
                Ok(state) => states.push(state),
                Err(e) => warn!("Failed to read ASUS mouse battery: {e:#}"),
            }
        }
    }

    Ok(states)
}

fn matches_model(device: &DeviceInfo, model: &AsusMouseModel) -> bool {
    device.vendor_id() == model.vendor_id
        && device.product_id() == model.product_id
        && device
            .path()
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains(model.path_hint)
}

fn read_mouse_state(
    api: &HidApi,
    device_info: &DeviceInfo,
    model: &AsusMouseModel,
) -> Result<AsusMouseState> {
    let device = device_info
        .open_device(api)
        .with_context(|| format!("Failed to open {}", model.name))?;
    let response = query_battery_packet(&device)?;
    let battery = parse_battery_response(&response, model.parser)
        .ok_or_else(|| anyhow!("Unsupported ASUS mouse battery response: {:02X?}", response))?;
    let name = device_info
        .product_string()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(model.name)
        .trim()
        .to_owned();

    Ok(AsusMouseState {
        name,
        battery,
        path: device_info.path().to_string_lossy().into_owned(),
        vendor_id: model.vendor_id,
        product_id: model.product_id,
    })
}

fn query_battery_packet(device: &HidDevice) -> Result<Vec<u8>> {
    drain(device);

    let mut packet = [0u8; 65];
    packet[1] = 0x12;
    packet[2] = 0x07;
    device
        .write(&packet)
        .context("Failed to send ASUS mouse battery packet")?;

    let mut response = [0u8; 65];
    let len = device
        .read_timeout(&mut response, 1000)
        .context("Failed to read ASUS mouse battery packet")?;
    if len == 0 {
        return Err(anyhow!("ASUS mouse did not return a battery packet"));
    }

    Ok(response[..len].to_vec())
}

fn drain(device: &HidDevice) {
    let mut buf = [0u8; 65];
    while matches!(device.read_timeout(&mut buf, 1), Ok(len) if len > 0) {}
}

fn parse_battery_response(response: &[u8], parser: BatteryParser) -> Option<u8> {
    let offset = match response {
        [0x12, 0x07, ..] => 0,
        [_, 0x12, 0x07, ..] => 1,
        _ => return None,
    };

    match parser {
        BatteryParser::QuarterSteps => response
            .get(offset + 4)
            .and_then(|step| (*step <= 4).then_some(step.saturating_mul(25))),
    }
}

fn stable_mouse_address(vendor_id: u16, product_id: u16, path: &str) -> u64 {
    let hash = path.bytes().fold(0x811C9DC5u32, |hash, byte| {
        hash.wrapping_mul(0x01000193) ^ u32::from(byte)
    });

    ASUS_DEVICE_ADDRESS_PREFIX
        | (u64::from(vendor_id) << 40)
        | (u64::from(product_id) << 24)
        | u64::from(hash & 0x00FF_FFFF)
}

pub async fn watch_asus_mouse_async(
    exit_flag: &Arc<AtomicBool>,
    restart_flag: &Arc<AtomicUsize>,
) -> Result<()> {
    let mut local_generation = 0;
    let proxy = PROXY.get().unwrap();

    while !exit_flag.load(Ordering::Relaxed) {
        let current_generation = restart_flag.load(Ordering::Relaxed);
        if local_generation < current_generation {
            info!("Watch ASUS mouse restart by restart flag.");
            local_generation = current_generation;
        }

        let current = get_asus_mouse_devices_info().await?;
        let current_addresses = current.iter().map(|entry| *entry.key()).collect::<Vec<_>>();
        let existing_addresses = BT_INFO_MAP
            .iter()
            .filter_map(|entry| entry.value().is_asus_hid().then_some(*entry.key()))
            .collect::<Vec<_>>();

        let mut need_update = false;

        for entry in current.iter() {
            let address = *entry.key();
            let next = entry.value().clone();
            if let Some(mut existing) = BT_INFO_MAP.get_mut(&address) {
                if existing.battery != next.battery || !existing.status {
                    let old_battery = existing.battery;
                    existing.battery = next.battery;
                    existing.status = true;
                    existing.battery_display = None;
                    info!(
                        "ASUS mouse [{}]: Battery {old_battery} -> {}",
                        existing.name, existing.battery
                    );
                    let _ = proxy.send_event(UserEvent::Notify(NotifyEvent::LowBattery(
                        existing.name.clone(),
                        existing.battery,
                        address,
                    )));
                    need_update = true;
                }
            } else {
                let name = next.name.clone();
                BT_INFO_MAP.insert(address, next);
                let _ = proxy.send_event(UserEvent::Notify(NotifyEvent::Added(name)));
                need_update = true;
            }
        }

        for address in existing_addresses {
            if !current_addresses.contains(&address)
                && let Some((_, mut info)) = BT_INFO_MAP.remove(&address)
            {
                info.status = false;
                let name = info.name.clone();
                BT_INFO_MAP.insert(address, info);
                let _ = proxy.send_event(UserEvent::Notify(NotifyEvent::Removed(name)));
                need_update = true;
            }
        }

        if need_update {
            let _ = proxy.send_event(UserEvent::UpdateTray);
        }

        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BatteryParser, parse_battery_response, stable_mouse_address};

    #[test]
    fn parses_strix_impact_quarter_step_response_without_report_id() {
        let response = [0x12, 0x07, 0x00, 0x00, 0x03, 0x03];

        assert_eq!(
            parse_battery_response(&response, BatteryParser::QuarterSteps),
            Some(75)
        );
    }

    #[test]
    fn parses_strix_impact_quarter_step_response_with_report_id() {
        let response = [0x00, 0x12, 0x07, 0x00, 0x00, 0x04, 0x03];

        assert_eq!(
            parse_battery_response(&response, BatteryParser::QuarterSteps),
            Some(100)
        );
    }

    #[test]
    fn creates_stable_asus_mouse_address() {
        assert_eq!(
            stable_mouse_address(0x0B05, 0x1949, "hid#vid_0b05&pid_1949&mi_00"),
            stable_mouse_address(0x0B05, 0x1949, "hid#vid_0b05&pid_1949&mi_00")
        );
    }
}
