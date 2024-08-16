use std::{collections::HashMap, io, sync::Arc, time::Duration};

use aap::{AAPEvent, AAPSocket};
use bluer::{Adapter, Address, Device, ErrorKind};
use tokio::sync::Mutex;
use tray::AirpodsTray;

mod aap;
mod pctl;
mod tray;

const APPLE_PRODUCT_IDS: &[u32] = &[
    // Apple
    0x2002, // AirPods 1
    0x200f, // AirPods 2
    0x2013, // AirPods 3
    0x200e, // AirPods Pro 1
    0x2014, // AirPods Pro 2
    0x2024, // AirPods Pro 2 (USB-C)
    0x200a, // AirPods Max 1
];

#[tokio::main]
async fn main() {
    let session = bluer::Session::new().await.unwrap();

    let mut known_airpods: HashMap<Address, Arc<Mutex<Option<()>>>> = HashMap::new();

    loop {
        let adapter = match session.default_adapter().await {
            Ok(x) => Some(x),
            Err(e) => {
                match e.kind {
                    ErrorKind::NotFound => None,
                    _ => Err(e).unwrap()
                }
            }
        };

        let mut killed = vec![];
        for (addr, done) in known_airpods.iter() {
            if done.lock().await.is_some() {
                killed.push(*addr);
            }
        }
        for addr in killed {
            known_airpods.remove(&addr);
        }

        if let Some(adapter) = adapter {
            for address in adapter.device_addresses().await.unwrap().into_iter() {
                let device = adapter.device(address).unwrap();
                if device.is_connected().await.unwrap() {
                    let modalias = device.modalias().await.unwrap();
                    if let Some(modalias) = modalias {
                        if modalias.vendor == 76 && APPLE_PRODUCT_IDS.contains(&modalias.product) {
                            let adapter = adapter.clone();
                            let done = Arc::new(Mutex::new(None));
                            if known_airpods.insert(address, done.clone()).is_none() {
                                tokio::task::spawn(async move {
                                    per_device(device, adapter, done).await
                                });
                            }
                        }
                    }
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn per_device(airpods: Device, adapter: Adapter, done: Arc<Mutex<Option<()>>>) {
    let aap = match AAPSocket::init(adapter, airpods.address()).await {
        Ok(aap) => aap,
        Err(e) => {
            if e.kind == bluer::ErrorKind::Internal(bluer::InternalErrorKind::Io(io::ErrorKind::NotConnected)) {
                done.lock().await.replace(());
            } else {
                Err::<(), bluer::Error>(e).unwrap();
            }
            return;
        }
    };
    let mut rx = aap.subscribe().await;

    let tray = AirpodsTray {
        address: airpods.address(),
        name: airpods.name().await.unwrap(),
        aap,
        ear_detection: true,
    };

    let service = ksni::TrayService::new(tray);
    let handle = service.handle();
    service.spawn();

    loop {
        match rx.recv().await {
            Ok(event) => {
                if let AAPEvent::EarsChanged(ears) = event {
                    handle.update(|tray| {
                        if tray.ear_detection {
                            if ears == (true, true) {
                                pctl::resume_active();
                            } else {
                                pctl::pause_active();
                            }
                        }
                    });
                } else if event == AAPEvent::Disconnected {
                    handle.shutdown();
                    break;
                } else {
                    handle.update(|_| {});
                }
            },
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                break;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {},
        }
    }

    done.lock().await.replace(());
}