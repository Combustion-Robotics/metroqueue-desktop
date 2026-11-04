//! DNS-SD advertising of the receiver (PROTOCOL.md §2): `_metroqueue._tcp` with TXT `id`, `v` and
//! `kind=desktop`, so phones list this computer in their device picker.

use std::collections::HashMap;

use mdns_sd::{ServiceDaemon, ServiceInfo};

use super::protocol::{KIND, PORT, PROTOCOL_VERSION, SERVICE_TYPE};

pub struct Advert {
    daemon: ServiceDaemon,
    fullname: String,
}

pub fn advertise(name: &str, id: &str) -> Option<Advert> {
    let props = HashMap::from([
        ("id".to_owned(), id.to_owned()),
        ("v".to_owned(), PROTOCOL_VERSION.to_string()),
        ("kind".to_owned(), KIND.to_owned()),
    ]);
    let host = format!("{}.local.", host_label(&super::computer_name()));
    let result = ServiceDaemon::new().and_then(|daemon| {
        let info = ServiceInfo::new(SERVICE_TYPE, name, &host, (), PORT, props)?.enable_addr_auto();
        let fullname = info.get_fullname().to_owned();
        daemon.register(info)?;
        Ok(Advert { daemon, fullname })
    });
    result.map_err(|e| tracing::warn!(error = %e, "metroqueue: mDNS advertising failed")).ok()
}

impl Drop for Advert {
    fn drop(&mut self) {
        // Unregister first: it sends the goodbye packets that make phones drop the entry at once.
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// The computer's name as one DNS label.
fn host_label(name: &str) -> String {
    let label: String = name
        .split('.')
        .next()
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    if label.trim_matches('-').is_empty() {
        "metroqueue-desktop".into()
    } else {
        label
    }
}
