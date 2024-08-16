use mpris::{Player, PlayerFinder};

fn get_active_player() -> Option<Player> {
    match PlayerFinder::new() {
        Ok(x) => match x.find_active() {
            Ok(x) => {
                return Some(x)
            }
            Err(mpris::FindingError::DBusError(e)) => {
                println!("[PCTL] Failed to find active MPRIS Player (DBus Error): {:#?}", e);
            },
            Err(mpris::FindingError::NoPlayerFound) => {},
        },
        Err(e) => {
            println!("[PCTL] Failed to create MPRIS PlayerFinder (DBus Error): {:#?}", e);
        }
    };
    None
}

pub fn pause_active() {
    match get_active_player() {
        Some(p) => {
            match p.get_playback_status() {
                Ok(pb) => {
                    if pb == mpris::PlaybackStatus::Playing {
                        if let Err(e) = p.pause() {
                            println!("[PCTL] Failed to pause the current MPRIS Player (DBus Error): {:#?}", e);
                        }
                    }
                },
                Err(e) => {
                    println!("[PCTL] Failed to get the playback status of the current MPRIS Player (DBus Error): {:#?}", e);
                }
            }
        },
        None => {},
    }
}

pub fn resume_active() {
    match get_active_player() {
        Some(p) => {
            match p.get_playback_status() {
                Ok(pb) => {
                    if pb == mpris::PlaybackStatus::Paused {
                        if let Err(e) = p.play() {
                            println!("[PCTL] Failed to resume the current MPRIS Player (DBus Error): {:#?}", e);
                        }
                    }
                },
                Err(e) => {
                    println!("[PCTL] Failed to get the playback status of the current MPRIS Player (DBus Error): {:#?}", e);
                }
            }
        },
        None => {},
    }
}