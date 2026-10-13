//! Network throughput of the active interface, from interface byte counters.

use std::time::Instant;

use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetBestInterface, GetIfEntry2, GetIfTable2, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211, MIB_IF_ROW2,
    MIB_IF_TABLE2,
};

pub struct NetSample {
    pub kind: String,
    pub down: f64,
    pub up: f64,
    pub session_down: u64,
    pub session_up: u64,
}

pub struct NetSampler {
    /// "auto" or an adapter alias such as "Wi-Fi".
    want: String,
    last: Option<(u32, u64, u64, Instant)>,
    session_down: u64,
    session_up: u64,
}

impl NetSampler {
    pub fn new(want: &str) -> Self {
        Self {
            want: want.to_string(),
            last: None,
            session_down: 0,
            session_up: 0,
        }
    }

    pub fn sample(&mut self) -> NetSample {
        let row = if self.want.eq_ignore_ascii_case("auto") {
            default_route_row()
        } else {
            row_by_alias(&self.want)
        };
        let Some(row) = row else {
            self.last = None;
            return NetSample {
                kind: "offline".into(),
                down: 0.0,
                up: 0.0,
                session_down: self.session_down,
                session_up: self.session_up,
            };
        };
        let now = Instant::now();
        let (mut down, mut up) = (0.0, 0.0);
        // Rates only within one interface; switching just starts a new baseline.
        if let Some((idx, rx, tx, t)) = self.last
            && idx == row.InterfaceIndex
        {
            let dt = (now - t).as_secs_f64().max(1e-3);
            let (drx, dtx) = (row.InOctets.saturating_sub(rx), row.OutOctets.saturating_sub(tx));
            self.session_down += drx;
            self.session_up += dtx;
            down = drx as f64 / dt;
            up = dtx as f64 / dt;
        }
        self.last = Some((row.InterfaceIndex, row.InOctets, row.OutOctets, now));
        NetSample {
            kind: kind(&row),
            down,
            up,
            session_down: self.session_down,
            session_up: self.session_up,
        }
    }
}

fn kind(row: &MIB_IF_ROW2) -> String {
    match row.Type {
        IF_TYPE_IEEE80211 => "wi-fi".into(),
        IF_TYPE_ETHERNET_CSMACD => "ethernet".into(),
        _ => alias(row).to_lowercase(),
    }
}

fn alias(row: &MIB_IF_ROW2) -> String {
    let len = row.Alias.iter().position(|&c| c == 0).unwrap_or(row.Alias.len());
    String::from_utf16_lossy(&row.Alias[..len])
}

/// The interface Windows would use to reach the internet.
fn default_route_row() -> Option<MIB_IF_ROW2> {
    let mut index = 0u32;
    // 8.8.8.8 in network byte order; only used for the routing decision.
    let dest = u32::from_ne_bytes([8, 8, 8, 8]);
    // SAFETY: out-pointer to a local.
    if unsafe { GetBestInterface(dest, &mut index) } != 0 {
        return None;
    }
    let mut row = MIB_IF_ROW2 {
        InterfaceIndex: index,
        ..Default::default()
    };
    // SAFETY: row has InterfaceIndex set as GetIfEntry2 requires.
    unsafe { GetIfEntry2(&mut row) }.is_ok().then_some(row)
}

fn row_by_alias(want: &str) -> Option<MIB_IF_ROW2> {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    // SAFETY: GetIfTable2 allocates the table; freed below with FreeMibTable.
    if unsafe { GetIfTable2(&mut table) }.is_err() || table.is_null() {
        return None;
    }
    // SAFETY: the table holds NumEntries rows.
    let rows = unsafe { std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize) };
    let found = rows.iter().find(|r| alias(r).eq_ignore_ascii_case(want)).copied();
    // SAFETY: table came from GetIfTable2.
    unsafe { FreeMibTable(table as *const _) };
    found
}
