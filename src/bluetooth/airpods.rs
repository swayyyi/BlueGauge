use super::{
    btc::is_supported_airpods_instance_id,
    info::{BT_INFO_MAP, BluetoothType},
};
use crate::{PROXY, UserEvent};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::Result;
use log::info;
use windows::{
    Devices::Bluetooth::Advertisement::{
        BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
        BluetoothLEScanningMode,
    },
    Foundation::TypedEventHandler,
    Storage::Streams::DataReader,
    core::Ref,
};

const APPLE_COMPANY_ID: u16 = 76;

#[derive(Clone, Debug)]
struct AirPodsAdvertisement {
    model_id: u16,
    left: Option<u8>,
    right: Option<u8>,
    case_box: Option<u8>,
}

fn level(value: u8) -> Option<u8> {
    (value <= 10).then_some(value * 10)
}

fn parse_packet(data: &[u8]) -> Option<AirPodsAdvertisement> {
    if data.len() != 27 || data[0] != 0x07 || data[1] != 25 {
        return None;
    }

    let model_id = u16::from_le_bytes([data[3], data[4]]);
    if !is_supported_airpods_model(model_id) {
        return None;
    }

    let broadcast_from_left = (data[5] & (1 << 5)) != 0;
    let current = level(data[6] & 0x0f);
    let another = level(data[6] >> 4);
    let case_box = level(data[7] & 0x0f);

    let (left, right) = if broadcast_from_left {
        (current, another)
    } else {
        (another, current)
    };

    Some(AirPodsAdvertisement {
        model_id,
        left,
        right,
        case_box,
    })
}

fn is_supported_airpods_model(model_id: u16) -> bool {
    matches!(
        model_id,
        0x2002 | 0x200E | 0x200F | 0x2013 | 0x2014 | 0x2019 | 0x201B | 0x2024 | 0x2027
    )
}

fn read_manufacturer_data(
    args: &BluetoothLEAdvertisementReceivedEventArgs,
) -> Result<Option<AirPodsAdvertisement>> {
    let advertisement = args.Advertisement()?;
    let manufacturer_data = advertisement.ManufacturerData()?;

    for index in 0..manufacturer_data.Size()? {
        let entry = manufacturer_data.GetAt(index)?;
        if entry.CompanyId()? != APPLE_COMPANY_ID {
            continue;
        }

        let buffer = entry.Data()?;
        let reader = DataReader::FromBuffer(&buffer)?;
        let mut bytes = Vec::with_capacity(reader.UnconsumedBufferLength()? as usize);
        while reader.UnconsumedBufferLength()? > 0 {
            bytes.push(reader.ReadByte()?);
        }

        return Ok(parse_packet(&bytes));
    }

    Ok(None)
}

fn apply_advertisement(adv: AirPodsAdvertisement) -> bool {
    let mut changed = false;

    for mut entry in BT_INFO_MAP.iter_mut() {
        let BluetoothType::Classic(instance_id) = &entry.r#type else {
            continue;
        };

        if !is_supported_airpods_instance_id(instance_id)
            || !instance_id
                .to_ascii_uppercase()
                .contains(&format!("PID&{:04X}", adv.model_id))
        {
            continue;
        }

        let levels = [adv.left, adv.right, adv.case_box];
        let Some(minimum) = levels.into_iter().flatten().min() else {
            continue;
        };

        let detail = levels
            .into_iter()
            .enumerate()
            .filter_map(|(index, value)| value.map(|value| (index, value)))
            .map(|(index, value)| match index {
                0 => format!("L {value}%"),
                1 => format!("R {value}%"),
                _ => format!("C {value}%"),
            })
            .collect::<Vec<_>>()
            .join(" · ");

        if entry.battery != minimum || entry.battery_display.as_deref() != Some(&detail) {
            info!("AirPods [{}]: {}", entry.name, detail);
            entry.battery = minimum;
            entry.battery_display = Some(detail);
            changed = true;
        }
    }

    changed
}

fn run_airpods_watcher(exit_flag: Arc<AtomicBool>) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let watcher = BluetoothLEAdvertisementWatcher::new()?;
    watcher.SetScanningMode(BluetoothLEScanningMode::Active)?;

    let handler = TypedEventHandler::<
        BluetoothLEAdvertisementWatcher,
        BluetoothLEAdvertisementReceivedEventArgs,
    >::new(
        move |_watcher: Ref<BluetoothLEAdvertisementWatcher>,
              args: Ref<BluetoothLEAdvertisementReceivedEventArgs>| {
            if let Some(args) = args.as_ref() {
                if let Ok(Some(adv)) = read_manufacturer_data(args) {
                    let _ = tx.send(adv);
                }
            }
            Ok(())
        },
    );

    let token = watcher.Received(&handler)?;
    watcher.Start()?;
    info!(
        "Started AirPods advertisement watcher: {:?}",
        watcher.Status()?
    );

    let proxy = PROXY.get().unwrap().clone();
    while !exit_flag.load(Ordering::Relaxed) {
        if let Ok(adv) = rx.recv_timeout(std::time::Duration::from_millis(500)) {
            if apply_advertisement(adv) {
                let _ = proxy.send_event(UserEvent::UpdateTray);
            }
        }
    }

    watcher.RemoveReceived(token)?;
    watcher.Stop()?;
    info!("Stopped AirPods advertisement watcher");
    Ok(())
}

pub async fn watch_airpods_async(
    exit_flag: &Arc<AtomicBool>,
    _restart_flag: &Arc<AtomicUsize>,
) -> Result<()> {
    let exit_flag = Arc::clone(exit_flag);
    tokio::task::spawn_blocking(move || run_airpods_watcher(exit_flag)).await??;
    Ok(())
}

#[cfg(test)]
fn decode_packet_for_test(data: &[u8]) -> Option<(u16, Option<u8>, Option<u8>, Option<u8>)> {
    parse_packet(data).map(|adv| (adv.model_id, adv.left, adv.right, adv.case_box))
}

#[cfg(test)]
mod tests {
    use super::decode_packet_for_test;

    #[test]
    fn decodes_airpods_pro_battery_packet() {
        let mut packet = vec![0u8; 27];
        packet[0] = 0x07;
        packet[1] = 25;
        packet[3] = 0x0E;
        packet[4] = 0x20;
        packet[5] = 1 << 5;
        packet[6] = 0x88;
        packet[7] = 0x07;

        assert_eq!(
            decode_packet_for_test(&packet),
            Some((0x200E, Some(80), Some(80), Some(70)))
        );
    }
}
