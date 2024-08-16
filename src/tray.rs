use std::vec;

use ksni::{Tray, MenuItem};

use crate::aap::{AAPSocket, ChargingState, ANC};

pub struct AirpodsTray {
    pub address: bluer::Address,
    pub name: Option<String>,
    pub aap: AAPSocket,
    pub ear_detection: bool,
}

impl Tray for AirpodsTray {
    fn title(&self) -> String {
        self.name.clone().unwrap_or("Unnamed AirPods".to_string())
    }

    fn id(&self) -> String {
        format!("airpodsd_{}", self.address)
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn icon_name(&self) -> String {
        "audio-headphones".into()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        use ksni::menu::*;

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (anc, batteries) = rt.block_on(async {
            (
                self.aap.get_anc().await,
                self.aap.get_batteries().await,
            )
        });

        let mut out = vec! [];

        if let Some(name) = self.name.as_ref() {
            out.push(
                StandardItem {
                    label: name.clone().replace("_", "__"),
                    enabled: false,
                    ..Default::default()
                }.into(),
            );
            out.push(MenuItem::Separator);
        }

        out.push(
            RadioGroup {
                selected: match anc {
                    ANC::Off => 0,
                    ANC::NoiseCancelling => 1,
                    ANC::Adaptive => 2,
                    ANC::Transparency => 3,
                },
                select: Box::new(|this: &mut Self, current| {
                    let anc = match current {
                        0 => ANC::Off,
                        1 => ANC::NoiseCancelling,
                        2 => ANC::Adaptive,
                        3 => ANC::Transparency,
                        _ => ANC::Off,
                    };

                    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                    rt.block_on(this.aap.set_anc(anc)).unwrap();
                }),
                options: vec! [
                    RadioItem {
                        label: "Off".into(),
                        ..Default::default()
                    },
                    RadioItem {
                        label: "Noise Cancelling".into(),
                        ..Default::default()
                    },
                    RadioItem {
                        label: "Adaptive".into(),
                        ..Default::default()
                    },
                    RadioItem {
                        label: "Transparency".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }.into()
        );

        out.push(MenuItem::Separator);

        if let Some(x) = batteries.single {
            if x.1 != ChargingState::Disconnected && x.1 != ChargingState::Unknown {
                out.push(
                    StandardItem {
                        label: format!("Battery: {}%{}", x.0, if x.1 == ChargingState::Charging { " (charging)" } else { "" }),
                        enabled: false,
                        ..Default::default()
                    }.into()
                )
            }
        }

        if let Some(x) = batteries.left {
            if x.1 != ChargingState::Disconnected && x.1 != ChargingState::Unknown {
                out.push(
                    StandardItem {
                        label: format!("Left: {}%{}", x.0, if x.1 == ChargingState::Charging { " (charging)" } else { "" }),
                        enabled: false,
                        ..Default::default()
                    }.into()
                )
            }
        }

        if let Some(x) = batteries.right {
            if x.1 != ChargingState::Disconnected && x.1 != ChargingState::Unknown {
                out.push(
                    StandardItem {
                        label: format!("Right: {}%{}", x.0, if x.1 == ChargingState::Charging { " (charging)" } else { "" }),
                        enabled: false,
                        ..Default::default()
                    }.into()
                )
            }
        }

        if let Some(x) = batteries.case {
            if x.1 != ChargingState::Disconnected && x.1 != ChargingState::Unknown {
                out.push(
                    StandardItem {
                        label: format!("Case: {}%{}", x.0, if x.1 == ChargingState::Charging { " (charging)" } else { "" }),
                        enabled: false,
                        ..Default::default()
                    }.into()
                )
            }
        }

        out.push(
            CheckmarkItem {
                label: "Ear detection".to_string(),
                checked: self.ear_detection,
                activate: Box::new(|this: &mut Self| {
                    this.ear_detection = !this.ear_detection
                }),
                ..Default::default()
            }.into()
        );

        out
    }
}
