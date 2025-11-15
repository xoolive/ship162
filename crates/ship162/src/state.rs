use rs162::{
    decode::mmsi::MmsiInfo,
    prelude::{NavigationStatus, ShipType},
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VesselState {
    pub mmsi: String,
    pub mmsi_info: Option<MmsiInfo>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub callsign: Option<String>,
    pub ship_name: Option<String>,
    pub ship_type: Option<ShipType>,
    pub to_bow: Option<u16>,
    pub to_stern: Option<u16>,
    pub to_port: Option<u8>,
    pub to_starboard: Option<u8>,
    pub destination: Option<String>,
    pub speed: Option<f32>,
    pub turn: Option<f32>,
    pub course: Option<f32>,
    pub heading: Option<u16>,
    pub status: Option<NavigationStatus>,
    pub last_update: u64,
    pub count: usize,
}

impl VesselState {
    pub fn new(mmsi: u32, mmsi_info: MmsiInfo) -> Self {
        Self {
            mmsi: format!("{:09}", mmsi),
            mmsi_info: Some(mmsi_info),
            latitude: None,
            longitude: None,
            callsign: None,
            ship_name: None,
            ship_type: None,
            to_bow: None,
            to_stern: None,
            to_port: None,
            to_starboard: None,
            destination: None,
            speed: None,
            turn: None,
            course: None,
            heading: None,
            status: None,
            last_update: 0,
            count: 0,
        }
    }

    pub fn dimensions_str(&self) -> String {
        if let (Some(bow), Some(stern), Some(port), Some(stbd)) =
            (self.to_bow, self.to_stern, self.to_port, self.to_starboard)
        {
            let length = bow + stern;
            let width = port + stbd;
            if length > 0 || width > 0 {
                return format!("{}x{}", length, width);
            }
        }
        "".to_string()
    }
}

#[derive(Debug, Default)]
pub struct AppState {
    vessels: HashMap<String, VesselState>,
    pub scroll_offset: usize,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_vessel(&mut self, mmsi: u32, mmsi_info: MmsiInfo) -> &mut VesselState {
        let mmsi_str = format!("{:09}", mmsi);
        let vessel = self
            .vessels
            .entry(mmsi_str)
            .or_insert_with(|| VesselState::new(mmsi, mmsi_info));
        vessel.count += 1;
        vessel
    }

    pub fn get_vessels(&self) -> Vec<&VesselState> {
        let mut vessels: Vec<_> = self.vessels.values().collect();
        vessels.sort_by_key(|v| v.mmsi.clone());
        vessels
    }

    pub fn vessel_count(&self) -> usize {
        self.vessels.len()
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, max_visible: usize) {
        let max_scroll = self.vessel_count().saturating_sub(max_visible);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }
}
