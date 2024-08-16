use std::{cmp::min, io::ErrorKind, sync::Arc, time::Duration};

use bluer::{l2cap::{SeqPacket, Socket, SocketAddr}, Adapter, Address, Result};
use tokio::{sync::{broadcast, Mutex}, time::sleep};

struct AAPSocketInner {
    current_anc: ANC,
    ears_in: (bool, bool),
    event_tx: broadcast::Sender<AAPEvent>,
    batteries: BatteryState,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargingState {
    #[default]
    Unknown,
    Charging,
    NotCharging,
    Disconnected,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatteryState {
    pub single: Option<(u8, ChargingState)>,
    pub left: Option<(u8, ChargingState)>,
    pub right: Option<(u8, ChargingState)>,
    pub case: Option<(u8, ChargingState)>,
}

#[derive(Clone)]
pub struct AAPSocket(Arc<Mutex<AAPSocketInner>>, Arc<SeqPacket>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ANC {
    Off,
    NoiseCancelling,
    Transparency,
    Adaptive,
}

impl ANC {
    fn to_u8(&self) -> u8 {
        match self {
            ANC::Off => 0x01,
            ANC::NoiseCancelling => 0x02,
            ANC::Transparency => 0x03,
            ANC::Adaptive => 0x04,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AAPEvent {
    ANCChanged(ANC),
    EarsChanged((bool, bool)),
    BatteriesChanged(BatteryState),
    Disconnected,
}

impl AAPSocket {
    pub async fn init(adapter: Adapter, address: Address) -> Result<AAPSocket> {
        let socket = Socket::new_seq_packet().unwrap();
        socket.bind(SocketAddr::new(adapter.address().await.unwrap(), bluer::AddressType::BrEdr, 0)).unwrap();

        let sa = SocketAddr::new(address, bluer::AddressType::BrEdr, 0x1001);

        let stream = socket.connect(sa).await.expect("Failed to connect to AirPods.");

        let mtu = stream.as_ref().recv_mtu().unwrap();

        let (event_tx, _) = broadcast::channel::<AAPEvent>(16);

        let s = Self(Arc::new(Mutex::new(AAPSocketInner {
            current_anc: ANC::Off,
            ears_in: (true, true),
            event_tx: event_tx.clone(),
            batteries: Default::default(),
        })), Arc::new(stream));

        let s2 = s.clone();

        sleep(Duration::from_secs(1)).await;
        s.send(&vec![0x00, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).await?; // init command

        tokio::task::spawn(async move {
            let s = s2;

            loop {
                let mut buf = vec![ 0u8; mtu.into() ];
                match s.1.recv(&mut buf).await {
                    Ok(bytes) => {
                        let buf = &buf[0..bytes];

                        if buf.len() >= 5 {
                            if buf[4] == 0x09 { // command Settings
                                if buf[6] == 0x0D { // setting type ANC
                                    let anc = match buf[7] {
                                        1 => ANC::Off,
                                        2 => ANC::NoiseCancelling,
                                        3 => ANC::Transparency,
                                        4 => ANC::Adaptive,
                                        _ => {
                                            println!("Unknown ANC option {}", buf[7]);
                                            ANC::Off
                                        }
                                    };

                                    println!("ANC updated to {:?}", anc);
                                    {
                                        let mut s = s.0.lock().await;

                                        // Off and Adaptive are represented exactly the same in the ANC setting type ?!?!?!?!?!
                                        if !(anc == ANC::Off && (s.current_anc == ANC::Off || s.current_anc == ANC::Adaptive)) {
                                            s.current_anc = anc;
                                        }
                                    }
                                    s.0.lock().await.current_anc = anc;
                                    let _ = event_tx.send(AAPEvent::ANCChanged(anc));
                                } else {
                                    // println!("misc settings packet received, type {}", buf[6]);
                                }
                            } else if buf[4] == 0x04 { // command Battery
                                let mut state = BatteryState::default();
                                let count = buf[6];

                                for i in 0..count {
                                    let start_byte = 7 + i as usize * 5;
                                    
                                    let charge = min(100, buf[start_byte + 2]);
                                    let charging = match buf[start_byte + 3] {
                                        0 => ChargingState::Unknown,
                                        1 => ChargingState::Charging,
                                        2 => ChargingState::NotCharging,
                                        4 => ChargingState::Disconnected,
                                        _ => {
                                            println!("Unknown charging state {}", buf[start_byte + 3]);
                                            ChargingState::Unknown
                                        }
                                    };

                                    let data = Some((charge, charging));

                                    match buf[start_byte] {
                                        0x01 => state.single = data,
                                        0x02 => state.right = data,
                                        0x04 => state.left = data,
                                        0x08 => state.case = data,
                                        _ => {
                                            println!("Unknown battery type {}", buf[start_byte]);
                                        }
                                    }
                                }

                                println!("Batteries updated to {:?}", state);
                                s.0.lock().await.batteries = state;
                                let _ = event_tx.send(AAPEvent::BatteriesChanged(state));
                            } else if buf[4] == 0x06 { // Ears update
                                let new = (buf[7] == 0, buf[6] == 0);
                                s.0.lock().await.ears_in = new;
                                println!("Ears updated to {:?}", new);
                                let _ = event_tx.send(AAPEvent::EarsChanged(new));
                            } else {
                                if buf.len() >= 30 {
                                    // println!("misc len>=5 packet received, command {} len {}", buf[4], buf.len());
                                } else {
                                    // println!("misc len>=5 packet received, command {} {:X?}", buf[4], buf);
                                }
                            }
                        } else {
                            // println!("misc packet received {:X?}", buf);
                        }
                    },
                    Err(e) => {
                        match e.kind() {
                            ErrorKind::ConnectionReset => {
                                let _ = event_tx.send(AAPEvent::Disconnected);
                            },
                            _ => {
                                println!("Something went wrong: {:#?}", e);
                            }
                        }
                        break;
                    }
                }
            }
        });
        
        sleep(Duration::from_secs(1)).await;
        s.enable_notifications().await?;

        Ok(s)
    }

    async fn send(&self, data: &[u8]) -> Result<()> {
        self.1.send(data).await?; // init command
        Ok(())
    }

    pub async fn enable_notifications(&self) -> Result<()> {
        self.send(&vec![0x04, 0x00, 0x04, 0x00, 0x0f, 0x00, 0xff, 0xff, 0xff, 0xff]).await?;
        Ok(())
    }

    pub async fn set_anc(&self, anc: ANC) -> Result<()> {
        self.send(&vec![0x04, 0x00, 0x04, 0x00, 0x09, 0x00, 0x0D, anc.to_u8(), 0x00, 0x00, 0x00]).await?;
        self.0.lock().await.current_anc = anc;
        Ok(())
    }

    pub async fn get_anc(&self) -> ANC {
        self.0.lock().await.current_anc
    }

    pub async fn get_batteries(&self) -> BatteryState {
        self.0.lock().await.batteries
    }

    pub async fn subscribe(&self) -> broadcast::Receiver<AAPEvent> {
        self.0.lock().await.event_tx.subscribe()
    }
}